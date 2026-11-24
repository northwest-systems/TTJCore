use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serde_json::{Value, json};
use ttjcore::{document::Document, entity::record, store::Store};

const P: &str = "pj-review-1";
const T: &str = "tsk-review-1";
const A: &str = "tkt-review-1";
const B: &str = "tkt-review-2";
const J: &str = "job-review-1";
const K: &str = "job-review-2";
const R: &str = "run-review-1";
const S: &str = "ssn-review-1";
const W: &str = "wkr-review-1";

async fn database() -> Store {
    Store::connect("sqlite::memory:", true)
        .await
        .unwrap()
        .with_management_actors(["owner"])
        .unwrap()
}
async fn get(db: &Store, kind: &str, id: &str) -> Value {
    db.call(&format!("{kind}.get"), json!({"id":id}))
        .await
        .unwrap()
}
async fn call(db: &Store, method: &str, mut p: Value) -> ttjcore::Result<Value> {
    p["reason"] = json!("Verified review operation");
    db.call(method, p).await.map(|v| v["value"].clone())
}
async fn admin(db: &Store, method: &str, mut p: Value) -> ttjcore::Result<Value> {
    p["mode"] = json!("management");
    p["actor"] = json!(["owner"]);
    call(db, method, p).await
}
async fn execution(db: &Store, method: &str, mut p: Value) -> ttjcore::Result<Value> {
    p["actor"] = json!([W, S]);
    call(db, method, p).await
}
async fn state(
    db: &Store,
    kind: &str,
    id: &str,
    status: &str,
    mut p: Value,
) -> ttjcore::Result<Value> {
    p["id"] = json!(id);
    p["revision"] = get(db, kind, id).await["revision"].clone();
    p["status"] = json!(status);
    admin(db, "status.set", p).await
}
fn ticket(id: &str, jobs: &[&str]) -> Value {
    json!({"record":{"id":id,"task_id":T,"worker_id":W,"session_id":S,"objective":"Review","jobs":jobs.iter().map(|id| json!({"id":id,"objective":"Check","criteria":["Verified"]})).collect::<Vec<_>>()}})
}
async fn seed(db: &Store, jobs: &[&str]) {
    for (method, p) in [
        (
            "worker.create",
            json!({"record":{"id":W,"name":"Reviewer"}}),
        ),
        ("session.create", json!({"record":{"id":S,"worker_id":W}})),
        ("project.create", json!({"record":{"id":P,"name":"Review"}})),
        (
            "task.create",
            json!({"parent":P,"record":{"id":T,"objective":"Deliver","criteria":["Verified"]}}),
        ),
        ("ticket.create", ticket(A, jobs)),
    ] {
        admin(db, method, p).await.unwrap();
    }
}
async fn start(db: &Store, job: &str, id: &str) -> ttjcore::Result<Value> {
    execution(db,"run.start",json!({"job":job,"revision":get(db,"job",job).await["revision"],"id":id,"safety":"Previous process stopped and effects verified"})).await
}
async fn finish(db: &Store, id: &str) {
    execution(
        db,
        "run.finish",
        json!({"id":id,"revision":get(db,"run",id).await["revision"],"result":"succeeded"}),
    )
    .await
    .unwrap();
}
async fn wait(db: &Store, target: &str) {
    execution(db,"run.wait",json!({"id":R,"revision":get(db,"run",R).await["revision"],"wait":{"target":target,"condition":"Verified response","since":"2026-01-01T00:00:00Z","resume_point":"Check response"}})).await.unwrap();
}
async fn evidence(db: &Store, status: &str) -> Value {
    json!({"id":J,"revision":get(db,"job",J).await["revision"],"evidence_run":R,"run_revision":get(db,"run",R).await["revision"],"evidence":"Matched signed approval to the pending request","wait_met":true,"criteria_met":true,"result":"approved","status":status})
}

#[tokio::test]
async fn work_changes_require_live_execution_or_explicit_authorized_management() {
    let db = database().await;
    seed(&db, &[J]).await;
    start(&db, J, R).await.unwrap();
    for (method, kind, id, patch) in [
        ("job.update", "job", J, json!({"resume":"Continue here"})),
        (
            "task.update",
            "task",
            T,
            json!({"handoff":{"context":"Current context"}}),
        ),
        ("status.set", "job", J, json!({})),
    ] {
        let mut p = json!({"id":id,"revision":get(&db,kind,id).await["revision"],"patch":patch,"status":"waiting"});
        assert!(execution(&db, method, p.clone()).await.is_err());
        p["run"] = json!(R);
        p["actor"] = json!([W, "ssn-review-2"]);
        assert!(call(&db, method, p.clone()).await.is_err());
        p["actor"] = json!(["stranger"]);
        assert!(call(&db, method, p.clone()).await.is_err());
        execution(&db, method, p).await.unwrap();
    }
    let p =
        json!({"id":J,"revision":get(&db,"job",J).await["revision"],"patch":{"result":"passed"}});
    let mut unknown = p.clone();
    unknown["mode"] = json!("management");
    unknown["actor"] = json!(["stranger"]);
    assert!(call(&db, "job.update", unknown).await.is_err());
    admin(&db, "job.update", p).await.unwrap();
    finish(&db, R).await;
    for (method, kind, id, patch) in [
        ("job.update", "job", J, json!({"resume":"Late"})),
        (
            "task.update",
            "task",
            T,
            json!({"handoff":{"context":"Late"}}),
        ),
        ("status.set", "job", J, json!({})),
    ] {
        let p = json!({"id":id,"revision":get(&db,kind,id).await["revision"],"run":R,"patch":patch,"status":"doing","resolved":true});
        assert!(execution(&db, method, p.clone()).await.is_err());
        assert!(admin(&db, method, p).await.is_err());
    }
    let no_grant = Store::connect("sqlite::memory:", true)
        .await
        .unwrap()
        .with_management_actors(std::iter::empty::<String>())
        .unwrap();
    seed(&no_grant, &[J]).await;
    assert!(
        state(&no_grant, "job", J, "canceled", json!({}))
            .await
            .is_err()
    );
    assert_eq!(get(&db, "run", R).await["record"]["result"], "succeeded");
    assert_eq!(
        db.call("run.list", json!({"parent":J})).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn ended_sessions_cannot_start_retry_or_reopen_and_new_tickets_still_work() {
    let db = database().await;
    seed(&db, &[J]).await;
    start(&db, J, R).await.unwrap();
    finish(&db, R).await;
    start(&db, J, "run-review-2").await.unwrap();
    finish(&db, "run-review-2").await;
    let session = get(&db, "session", S).await;
    admin(
        &db,
        "session.update",
        json!({"id":S,"revision":session["revision"],"patch":{"ended":"2026-01-01T01:00:00Z"}}),
    )
    .await
    .unwrap();
    assert!(start(&db, J, "run-review-3").await.is_err());
    let session = get(&db, "session", S).await;
    assert!(
        admin(
            &db,
            "session.update",
            json!({"id":S,"revision":session["revision"],"patch":{"ended":null}})
        )
        .await
        .is_err()
    );
    state(
        &db,
        "ticket",
        A,
        "canceled",
        json!({"cancel_reason":"Session ended after confirmed stop"}),
    )
    .await
    .unwrap();
    admin(
        &db,
        "session.create",
        json!({"record":{"id":"ssn-review-2","worker_id":W}}),
    )
    .await
    .unwrap();
    let mut next = ticket(B, &[K]);
    next["record"]["session_id"] = json!("ssn-review-2");
    next["record"]["refs"] = json!([{"ticket_id":A,"kind":"rerun_of","note":"Previous execution stopped and effects verified"}]);
    admin(&db, "ticket.create", next).await.unwrap();
    call(&db,"run.start",json!({"actor":[W,"ssn-review-2"],"job":K,"revision":get(&db,"job",K).await["revision"],"safety":"Previous execution stopped and effects verified"})).await.unwrap();
    let fresh = database().await;
    seed(&fresh, &[J]).await;
    admin(&fresh,"session.update",json!({"id":S,"revision":get(&fresh,"session",S).await["revision"],"patch":{"ended":"2026-01-01T01:00:00Z"}})).await.unwrap();
    assert!(start(&fresh, J, R).await.is_err());
}

#[tokio::test]
async fn late_evidence_resolves_only_the_named_wait_and_transitions_atomically() {
    let db = database().await;
    seed(&db, &[J]).await;
    admin(&db, "ticket.create", ticket(B, &[K])).await.unwrap();
    start(&db, J, R).await.unwrap();
    wait(&db, B).await;
    finish(&db, R).await;
    let before = (
        get(&db, "run", R).await,
        get(&db, "job", J).await,
        get(&db, "ticket", A).await,
    );
    let mut p = evidence(&db, "completed").await;
    p["criteria_met"] = json!(false);
    assert!(admin(&db, "job.accept_evidence", p).await.is_err());
    assert_eq!(
        before,
        (
            get(&db, "run", R).await,
            get(&db, "job", J).await,
            get(&db, "ticket", A).await
        )
    );
    for (key, value) in [
        ("wait_met", json!(false)),
        ("evidence", json!("")),
        ("run_revision", json!(0)),
        ("evidence_run", json!("run-review-99")),
    ] {
        let mut invalid = evidence(&db, "completed").await;
        invalid[key] = value;
        assert!(admin(&db, "job.accept_evidence", invalid).await.is_err());
        assert_eq!(
            before,
            (
                get(&db, "run", R).await,
                get(&db, "job", J).await,
                get(&db, "ticket", A).await
            )
        );
    }
    for status in ["paused", "blocked"] {
        state(&db, "ticket", A, status, json!({})).await.unwrap();
        assert!(
            admin(&db, "job.accept_evidence", evidence(&db, "completed").await)
                .await
                .is_err()
        );
        state(
            &db,
            "ticket",
            A,
            "waiting",
            json!({"release":true,"resolved":true}),
        )
        .await
        .unwrap();
    }
    let mut p = evidence(&db, "doing").await;
    p["actor"] = json!(["outsider"]);
    p["mode"] = json!("management");
    assert!(call(&db, "job.accept_evidence", p).await.is_err());
    let mut p = evidence(&db, "doing").await;
    p["actor"] = json!([W, "ssn-review-2"]);
    assert!(call(&db, "job.accept_evidence", p).await.is_err());
    let mut p = evidence(&db, "doing").await;
    p["run"] = json!(R);
    assert!(execution(&db, "job.accept_evidence", p).await.is_err());
    execution(&db, "job.accept_evidence", evidence(&db, "doing").await)
        .await
        .unwrap();
    let after = get(&db, "run", R).await;
    assert!(after["record"]["wait"]["resolved"].is_string());
    assert_eq!(after["record"]["result"], before.0["record"]["result"]);
    assert_eq!(after["record"]["ended"], before.0["record"]["ended"]);
    assert_eq!(get(&db, "job", J).await["record"]["status"], "doing");
    assert_eq!(get(&db, "ticket", A).await["record"]["status"], "doing");
    let b = get(&db, "ticket", B).await;
    admin(
        &db,
        "ticket.update",
        json!({"id":B,"revision":b["revision"],"patch":{"depends":[A]}}),
    )
    .await
    .unwrap();
    state(
        &db,
        "job",
        J,
        "completed",
        json!({"criteria_met":true,"result":"approved"}),
    )
    .await
    .unwrap();
    assert_eq!(get(&db, "ticket", A).await["record"]["status"], "completed");
    assert_eq!(get(&db, "ticket", B).await["record"]["status"], "todo");
    assert!(
        admin(&db, "job.accept_evidence", evidence(&db, "doing").await)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn paused_waits_round_trip_while_unrepresentable_hidden_guards_are_rejected() {
    let db = database().await;
    seed(&db, &[J]).await;
    start(&db, J, R).await.unwrap();
    wait(&db, "external approval").await;
    state(&db, "job", J, "paused", json!({})).await.unwrap();
    finish(&db, R).await;
    let value = db
        .call("document.export", json!({"project":P}))
        .await
        .unwrap();
    let document = Document::parse(&value).unwrap();
    let directory = std::env::temp_dir().join(format!("ttj-review-{}", uuid::Uuid::new_v4()));
    let paths = document.write_split(&directory).unwrap();
    assert!(Document::new().write_split(&directory).is_err());
    let files: Vec<_> = paths
        .iter()
        .map(|path| Document::read(path).unwrap())
        .collect();
    assert_eq!(Document::merge(files.clone()).unwrap(), document);
    for split in [false, true] {
        let target = database().await;
        if split {
            for file in &files {
                admin(
                    &target,
                    "document.import",
                    json!({"documents":[file.to_value()]}),
                )
                .await
                .unwrap();
            }
        } else {
            admin(&target, "document.import", json!({"documents":[value]}))
                .await
                .unwrap();
        }
        assert_eq!(
            target
                .call("document.export", json!({"project":P}))
                .await
                .unwrap(),
            value
        );
        assert!(
            state(
                &target,
                "job",
                J,
                "doing",
                json!({"release":true,"resolved":true})
            )
            .await
            .is_err()
        );
        state(&target, "job", J, "waiting", json!({"release":true}))
            .await
            .unwrap();
        admin(
            &target,
            "job.accept_evidence",
            evidence(&target, "completed").await,
        )
        .await
        .unwrap();
        assert_eq!(
            get(&target, "ticket", A).await["record"]["status"],
            "completed"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
    let hidden = database().await;
    seed(&hidden, &[J]).await;
    state(&hidden, "task", T, "waiting", json!({}))
        .await
        .unwrap();
    state(&hidden, "task", T, "paused", json!({}))
        .await
        .unwrap();
    assert!(
        hidden
            .call("document.export", json!({"project":P}))
            .await
            .is_err()
    );
    let blocked = database().await;
    seed(&blocked, &[J]).await;
    state(&blocked, "job", J, "blocked", json!({}))
        .await
        .unwrap();
    state(&blocked, "job", J, "paused", json!({}))
        .await
        .unwrap();
    assert!(
        blocked
            .call("document.export", json!({"project":P}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn exports_refuse_to_drop_deleted_completion_prerequisites() {
    let db = database().await;
    seed(&db, &[J]).await;
    admin(&db, "ticket.create", ticket(B, &[K])).await.unwrap();
    state(&db, "job", K, "canceled", json!({})).await.unwrap();
    let a = get(&db, "ticket", A).await;
    admin(
        &db,
        "ticket.update",
        json!({"id":A,"revision":a["revision"],"patch":{"depends":[B]}}),
    )
    .await
    .unwrap();
    admin(
        &db,
        "ticket.delete",
        json!({"id":B,"revision":get(&db,"ticket",B).await["revision"]}),
    )
    .await
    .unwrap();
    assert!(
        db.call("document.export", json!({"project":P}))
            .await
            .is_err()
    );
    assert_eq!(
        db.call("project.snapshot", json!({"id":P})).await.unwrap()["dependencies"][0]["deleted"],
        true
    );
}

#[tokio::test]
async fn open_run_restoration_and_updates_require_current_job_and_completed_prerequisites() {
    let source = database().await;
    seed(&source, &[J, K]).await;
    let mut later = source
        .call("document.export", json!({"project":P}))
        .await
        .unwrap();
    later["project"]["tasks"][0]["status"] = json!("doing");
    later["tickets"][0]["status"] = json!("doing");
    later["tickets"][0]["jobs"][1]["status"] = json!("doing");
    later["tickets"][0]["jobs"][1]["runs"] = json!([{"id":R,"started":"2026-01-01T00:00:00Z"}]);
    let target = database().await;
    assert!(
        admin(&target, "document.import", json!({"documents":[later]}))
            .await
            .is_err()
    );
    assert!(target.call("project.get", json!({"id":P})).await.is_err());
    later["tickets"][0]["jobs"][0]["status"] = json!("completed");
    let valid = database().await;
    admin(&valid, "document.import", json!({"documents":[later]}))
        .await
        .unwrap();
    execution(&valid,"run.wait",json!({"id":R,"revision":get(&valid,"run",R).await["revision"],"wait":{"target":"external"}})).await.unwrap();
    finish(&valid, R).await;
    let mut held = later.clone();
    held["tickets"][0]["jobs"][1]["runs"][0]["result"] = json!("interrupted");
    held["tickets"][0]["jobs"][1]["runs"][0]["wait"] = json!({"target":"external approval"});
    let guarded = database().await;
    admin(&guarded, "document.import", json!({"documents":[held]}))
        .await
        .unwrap();
    assert!(start(&guarded, K, "run-review-2").await.is_err());
    let mut ended = later.clone();
    ended["sessions"][0]["ended"] = json!("2026-01-01T01:00:00Z");
    assert!(
        admin(&target, "document.import", json!({"documents":[ended]}))
            .await
            .is_err()
    );
    let mut prerequisite = later;
    let mut other = ticket(B, &["job-review-3"])["record"].clone();
    other["status"] = json!("todo");
    prerequisite["tickets"].as_array_mut().unwrap().push(other);
    prerequisite["tickets"][0]["depends"] = json!([B]);
    assert!(
        admin(
            &target,
            "document.import",
            json!({"documents":[prerequisite]})
        )
        .await
        .is_err()
    );
    start(&source, J, R).await.unwrap();
    let run = record::Entity::find_by_id(R)
        .one(&source.db)
        .await
        .unwrap()
        .unwrap();
    let mut model: record::ActiveModel = run.into();
    model.parent = Set(Some(K.into()));
    model.update(&source.db).await.unwrap();
    let run = get(&source, "run", R).await;
    assert!(
        execution(
            &source,
            "run.finish",
            json!({"id":R,"revision":run["revision"],"result":"succeeded"})
        )
        .await
        .is_err()
    );
    assert!(execution(&source,"job.update",json!({"id":K,"revision":get(&source,"job",K).await["revision"],"run":R,"patch":{"resume":"Late job"}})).await.is_err());
}
