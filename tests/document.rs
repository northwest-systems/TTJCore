use serde_json::{Value, json};
use ttjcore::{PROTOCOL, document::Document, store::Store};

const PROJECT: &str = "pj-ABC123-1";
const TASK: &str = "tsk-ABC123-1";
const TICKET: &str = "tkt-ABC123-1";
const JOB: &str = "job-ABC123-1";
const RUN: &str = "run-ABC123-1";
const WORKER: &str = "wkr-ABC123-1";
const SESSION: &str = "ssn-ABC123-1";

async fn change(db: &Store, method: &str, mut p: Value) -> ttjcore::Result<Value> {
    p["actor"] = if method.starts_with("run.") || p.get("run").is_some() {
        json!([WORKER, SESSION])
    } else {
        json!(["owner"])
    };
    if !method.starts_with("run.") && p.get("run").is_none() {
        p["mode"] = json!("management");
    }
    p["reason"] = json!("Verified operation");
    db.call(method, p).await.map(|v| v["value"].clone())
}
async fn get(db: &Store, kind: &str, id: &str) -> Value {
    db.call(&format!("{kind}.get"), json!({"id":id}))
        .await
        .unwrap()
}
async fn seed(db: &Store) {
    for (method, p) in [
        (
            "worker.create",
            json!({"record":{"id":WORKER,"name":"Alpha"}}),
        ),
        (
            "session.create",
            json!({"record":{"id":SESSION,"worker_id":WORKER}}),
        ),
        (
            "project.create",
            json!({"record":{"id":PROJECT,"name":"Example"}}),
        ),
        (
            "task.create",
            json!({"parent":PROJECT,"record":{"id":TASK,"objective":"Deliver","criteria":["Verified"],"resume":"Check formal records","handoff":{"context":"Draft"}}}),
        ),
        ("ticket.create", ticket(TICKET, JOB)),
    ] {
        change(db, method, p).await.unwrap();
    }
}
fn ticket(id: &str, job: &str) -> Value {
    json!({"record":{"id":id,"task_id":TASK,"worker_id":WORKER,"session_id":SESSION,"objective":"Deliver ticket","exclusions":["Publishing"],"jobs":[{"id":job,"objective":"Prepare","criteria":["Checked"],"instructions":"Verify output"}]}})
}
async fn start(db: &Store, job: &str, run: &str) -> ttjcore::Result<Value> {
    let row = get(db, "job", job).await;
    change(
        db,
        "run.start",
        json!({"id":run,"job":job,"revision":row["revision"]}),
    )
    .await
}
async fn wait(db: &Store, run: &str, value: Value) -> ttjcore::Result<Value> {
    let row = get(db, "run", run).await;
    change(
        db,
        "run.wait",
        json!({"id":run,"revision":row["revision"],"wait":value}),
    )
    .await
}
async fn state(
    db: &Store,
    kind: &str,
    id: &str,
    status: &str,
    mut extra: Value,
) -> ttjcore::Result<Value> {
    let row = get(db, kind, id).await;
    extra["id"] = json!(id);
    extra["revision"] = row["revision"].clone();
    extra["status"] = json!(status);
    change(db, "status.set", extra).await
}

#[tokio::test]
async fn files_and_store_round_trip_formal_records_after_handoff_loss_and_restart() {
    let directory = std::env::temp_dir().join(format!("ttj-document-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let database = directory.join("test.sqlite");
    let url = format!(
        "sqlite://{}?mode=rwc",
        database.to_string_lossy().replace('\\', "/")
    );
    let db = connect(&url, true).await.unwrap();
    seed(&db).await;
    start(&db, JOB, RUN).await.unwrap();
    wait(&db,RUN,json!({"target":"external review","condition":"Approved","since":"2026-01-01T00:00:00Z","resume_point":"Verify review","timeout":"2026-01-02T00:00:00Z"})).await.unwrap();
    let task = get(&db, "task", TASK).await;
    change(
        &db,
        "task.update",
        json!({"id":TASK,"revision":task["revision"],"patch":{"handoff":null}}),
    )
    .await
    .unwrap();
    db.db.close().await.unwrap();
    let db = connect(&url, false).await.unwrap();
    let value = db
        .call("document.export", json!({"project":PROJECT}))
        .await
        .unwrap();
    let document = Document::parse(&value).unwrap();
    assert!(value["project"]["tasks"][0].get("handoff").is_none());
    assert_eq!(
        value["project"]["tasks"][0]["resume"],
        "Check formal records"
    );
    assert_eq!(
        value["tickets"][0]["jobs"][0]["runs"][0]["wait"]["resume_point"],
        "Verify review"
    );
    assert_eq!(value["tickets"][0]["exclusions"], json!(["Publishing"]));
    let combined = directory.join("combined.json");
    document.write(&combined).unwrap();
    assert_eq!(Document::read(&combined).unwrap(), document);
    let paths = document.write_split(directory.join("split")).unwrap();
    let files: Vec<_> = paths
        .iter()
        .map(|path| Document::read(path).unwrap())
        .collect();
    assert_eq!(Document::merge(files.clone()).unwrap(), document);
    let other = connect("sqlite::memory:", true).await.unwrap();
    for file in files {
        change(
            &other,
            "document.import",
            json!({"documents":[file.to_value()]}),
        )
        .await
        .unwrap();
    }
    assert_eq!(
        other
            .call("document.export", json!({"project":PROJECT}))
            .await
            .unwrap(),
        value
    );
    let run = get(&other, "run", RUN).await;
    change(
        &other,
        "run.finish",
        json!({"id":RUN,"revision":run["revision"],"result":"interrupted"}),
    )
    .await
    .unwrap();
    assert!(
        wait(&other, RUN, json!({"resolved":"2026-01-01T01:00:00Z"}))
            .await
            .is_err()
    );
    db.db.close().await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn handoff_is_single_volatile_slot_and_job_result_is_independent() {
    let db = connect("sqlite::memory:", true).await.unwrap();
    seed(&db).await;
    start(&db, JOB, RUN).await.unwrap();
    let run = get(&db, "run", RUN).await;
    change(
        &db,
        "run.finish",
        json!({"id":RUN,"revision":run["revision"],"result":"succeeded"}),
    )
    .await
    .unwrap();
    assert_eq!(get(&db, "job", JOB).await["record"]["status"], "doing");
    let job = get(&db, "job", JOB).await;
    change(
        &db,
        "job.update",
        json!({"id":JOB,"revision":job["revision"],"patch":{"result":"failed"}}),
    )
    .await
    .unwrap();
    assert_eq!(get(&db, "job", JOB).await["record"]["status"], "doing");
    assert!(
        state(
            &db,
            "task",
            TASK,
            "completed",
            json!({"criteria_met":true,"result":"Verified"})
        )
        .await
        .is_err()
    );
    state(
        &db,
        "job",
        JOB,
        "completed",
        json!({"criteria_met":true,"result":"changes_requested"}),
    )
    .await
    .unwrap();
    let job = get(&db, "job", JOB).await;
    assert_eq!(job["record"]["status"], "completed");
    assert_eq!(job["record"]["result"], "changes_requested");
    state(
        &db,
        "task",
        TASK,
        "completed",
        json!({"criteria_met":true,"result":"Owner verified scope"}),
    )
    .await
    .unwrap();
    let task = get(&db, "task", TASK).await;
    assert!(task["record"].get("handoff").is_none());
    assert!(task["record"].get("result").is_none());
    assert!(task["record"].get("project").is_none());
    assert!(task["record"].get("tickets").is_none());
    assert!(change(&db,"task.update",json!({"id":TASK,"revision":task["revision"],"run":RUN,"patch":{"handoff":{"context":"Late"}}})).await.is_err());
}

#[tokio::test]
async fn mixed_wait_dependencies_remain_acyclic_when_paused_or_blocked() {
    let db = connect("sqlite::memory:", true).await.unwrap();
    seed(&db).await;
    change(&db, "ticket.create", ticket("tkt-ABC123-2", "job-ABC123-2"))
        .await
        .unwrap();
    start(&db, JOB, RUN).await.unwrap();
    start(&db, "job-ABC123-2", "run-ABC123-2").await.unwrap();
    wait(&db, RUN, json!({"target":"tkt-ABC123-2"}))
        .await
        .unwrap();
    state(&db, "job", JOB, "blocked", json!({})).await.unwrap();
    state(&db, "ticket", TICKET, "paused", json!({}))
        .await
        .unwrap();
    assert!(
        wait(&db, "run-ABC123-2", json!({"target":TICKET}))
            .await
            .is_err()
    );
    assert!(
        get(&db, "run", "run-ABC123-2").await["record"]
            .get("wait")
            .is_none()
    );
    wait(&db, RUN, json!({"resolved":"2026-01-01T01:00:00Z"}))
        .await
        .unwrap();
    wait(&db, "run-ABC123-2", json!({"target":TICKET}))
        .await
        .unwrap();
    let document = db
        .call("document.export", json!({"project":PROJECT}))
        .await
        .unwrap();
    Document::parse(&document).unwrap();
}

#[tokio::test]
async fn cancellation_closes_runs_preserves_reason_and_new_ticket_references_old() {
    let db = connect("sqlite::memory:", true).await.unwrap();
    seed(&db).await;
    start(&db, JOB, RUN).await.unwrap();
    assert!(
        state(&db, "ticket", TICKET, "canceled", json!({}))
            .await
            .is_err()
    );
    state(
        &db,
        "ticket",
        TICKET,
        "canceled",
        json!({"cancel_reason":"Session cannot resume"}),
    )
    .await
    .unwrap();
    let old = get(&db, "ticket", TICKET).await;
    assert_eq!(old["record"]["cancel_reason"], "Session cannot resume");
    assert_eq!(old["record"]["worker_id"], WORKER);
    assert_eq!(old["record"]["session_id"], SESSION);
    assert_eq!(old["record"]["jobs"][0]["runs"][0]["result"], "canceled");
    assert_eq!(old["record"]["jobs"][0]["runs"][0]["id"], RUN);
    assert_eq!(get(&db, "run", RUN).await["active"], false);
    assert!(get(&db, "run", RUN).await["record"].get("ended").is_none());
    change(
        &db,
        "session.create",
        json!({"record":{"id":"ssn-ABC123-2","worker_id":WORKER}}),
    )
    .await
    .unwrap();
    let mut next = ticket("tkt-ABC123-2", "job-ABC123-2");
    next["record"]["session_id"] = json!("ssn-ABC123-2");
    next["record"]["refs"] = json!([{"ticket_id":TICKET,"kind":"rerun_of","note":"Prior execution stopped and environment verified"}]);
    change(&db, "ticket.create", next).await.unwrap();
    assert!(start(&db, "job-ABC123-2", "run-ABC123-2").await.is_err());
    let job = get(&db, "job", "job-ABC123-2").await;
    db.call("run.start",json!({"actor":[WORKER,"ssn-ABC123-2"],"reason":"Verified prior stop","job":job["record"]["id"],"revision":job["revision"]})).await.unwrap();
    state(&db, "task", TASK, "canceled", json!({}))
        .await
        .unwrap();
    let next = get(&db, "ticket", "tkt-ABC123-2").await;
    assert_eq!(next["record"]["status"], "canceled");
    assert_eq!(next["record"]["cancel_reason"], "Verified operation");
    Document::parse(
        &db.call("document.export", json!({"project":PROJECT}))
            .await
            .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn import_is_atomic_and_forbidden_fields_do_not_enter_storage() {
    let db = connect("sqlite::memory:", true).await.unwrap();
    seed(&db).await;
    let invalid = json!({"protocol":PROTOCOL,"workers":[{"id":"wkr-ABC123-2","name":"New"}],"project":{"id":PROJECT,"name":"Conflict"}});
    assert!(
        change(&db, "document.import", json!({"documents":[invalid]}))
            .await
            .is_err()
    );
    assert!(
        db.call("worker.get", json!({"id":"wkr-ABC123-2"}))
            .await
            .is_err()
    );
    for key in [
        "claims",
        "generation",
        "run_seq",
        "takeover",
        "activity",
        "observed",
        "assignee",
    ] {
        let mut p = ticket("tkt-ABC123-2", "job-ABC123-2");
        p["record"][key] = json!([]);
        assert!(change(&db, "ticket.create", p).await.is_err());
    }
    let empty = change(&db,"ticket.create",json!({"record":{"task_id":TASK,"worker_id":WORKER,"session_id":SESSION,"objective":"No jobs yet","jobs":[],"status":"todo"}})).await.unwrap();
    assert!(empty["record"].get("jobs").is_none());
    assert_eq!(empty["record"]["status"], "todo");
    let huge = "wkr-ZZZ999-999999999999999999999999999999";
    change(
        &db,
        "worker.create",
        json!({"record":{"id":huge,"name":"Large counter"}}),
    )
    .await
    .unwrap();
    // A fresh database isolates the selected ID series from the seed series.
    let separate = connect("sqlite::memory:", true).await.unwrap();
    change(
        &separate,
        "worker.create",
        json!({"record":{"id":huge,"name":"Large counter"}}),
    )
    .await
    .unwrap();
    let next = change(
        &separate,
        "worker.create",
        json!({"record":{"name":"Next"}}),
    )
    .await
    .unwrap();
    assert_eq!(
        next["record"]["id"],
        "wkr-ZZZ999-1000000000000000000000000000000"
    );
    let value = db
        .call("document.export", json!({"project":PROJECT}))
        .await
        .unwrap();
    assert_eq!(value.as_object().unwrap().len(), 5);
    for key in ["projects", "tasks", "jobs", "runs"] {
        assert!(value.get(key).is_none());
    }
    assert_eq!(value["tickets"][0]["task_id"], TASK);
    assert!(value["tickets"][0]["jobs"][0].is_object());
    assert!(value["tickets"][0]["jobs"][0].get("runs").is_none());
}

#[tokio::test]
async fn generated_sequences_cover_all_seven_kinds_and_survive_purge() {
    let db = connect("sqlite::memory:", true).await.unwrap();
    let worker = change(&db, "worker.create", json!({"record":{"name":"First"}}))
        .await
        .unwrap();
    let next_worker = change(&db, "worker.create", json!({"record":{"name":"Second"}}))
        .await
        .unwrap();
    let session = change(
        &db,
        "session.create",
        json!({"record":{"worker_id":worker["record"]["id"]}}),
    )
    .await
    .unwrap();
    let next_session = change(
        &db,
        "session.create",
        json!({"record":{"worker_id":worker["record"]["id"]}}),
    )
    .await
    .unwrap();
    let project = change(&db, "project.create", json!({"record":{"name":"First"}}))
        .await
        .unwrap();
    let next_project = change(&db, "project.create", json!({"record":{"name":"Second"}}))
        .await
        .unwrap();
    let task = change(&db,"task.create",json!({"parent":project["record"]["id"],"record":{"objective":"Deliver","criteria":["Checked"]}})).await.unwrap();
    let next_task = change(&db,"task.create",json!({"parent":project["record"]["id"],"record":{"objective":"Next","criteria":["Checked"]}})).await.unwrap();
    let jobs: Vec<_> = (0..12)
        .map(|n| json!({"objective":format!("Job {n}"),"criteria":["Checked"]}))
        .collect();
    let record = json!({"task_id":task["record"]["id"],"worker_id":worker["record"]["id"],"session_id":session["record"]["id"],"objective":"Work","jobs":jobs});
    let first = change(&db, "ticket.create", json!({"record":record}))
        .await
        .unwrap();
    let second = change(&db,"ticket.create",json!({"record":{"task_id":next_task["record"]["id"],"worker_id":worker["record"]["id"],"session_id":session["record"]["id"],"objective":"Next","jobs":[{"objective":"New job","criteria":["Checked"]}]}})).await.unwrap();
    for (first, next) in [
        (&worker, &next_worker),
        (&session, &next_session),
        (&project, &next_project),
        (&task, &next_task),
        (&first, &second),
    ] {
        let a = first["record"]["id"].as_str().unwrap();
        let b = next["record"]["id"].as_str().unwrap();
        assert!(ttjcore::document::is_id(a));
        assert_eq!(ttjcore::document::sequence(a).unwrap().1, 1);
        assert_eq!(ttjcore::document::sequence(b).unwrap().1, 2);
        assert_eq!(
            ttjcore::document::sequence(a).unwrap().0,
            ttjcore::document::sequence(b).unwrap().0
        );
    }
    for (index, job) in first["record"]["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(
            ttjcore::document::sequence(job["id"].as_str().unwrap())
                .unwrap()
                .1,
            index as u64 + 1
        );
    }
    let job_id = first["record"]["jobs"][0]["id"].as_str().unwrap();
    let actor = json!([worker["record"]["id"], session["record"]["id"]]);
    let job = get(&db, "job", job_id).await;
    let run = db
        .call(
            "run.start",
            json!({"actor":actor,"reason":"Start","job":job_id,"revision":job["revision"]}),
        )
        .await
        .unwrap();
    let run = &run["value"]["run"];
    assert_eq!(
        ttjcore::document::sequence(run["record"]["id"].as_str().unwrap())
            .unwrap()
            .1,
        1
    );
    db.call("run.finish",json!({"actor":actor,"reason":"Attempt failed","id":run["record"]["id"],"revision":run["revision"],"result":"failed"})).await.unwrap();
    let job = get(&db, "job", job_id).await;
    assert!(
        db.call(
            "run.start",
            json!({"actor":actor,"reason":"Retry","job":job_id,"revision":job["revision"]})
        )
        .await
        .is_err()
    );
    let retry = db.call("run.start",json!({"actor":actor,"reason":"Retry","job":job_id,"revision":job["revision"],"safety":"Previous execution stopped"})).await.unwrap();
    assert_eq!(
        ttjcore::document::sequence(retry["value"]["run"]["record"]["id"].as_str().unwrap())
            .unwrap()
            .1,
        2
    );
    let task_id = task["record"]["id"].as_str().unwrap();
    state(&db, "task", task_id, "canceled", json!({}))
        .await
        .unwrap();
    let task = get(&db, "task", task_id).await;
    change(
        &db,
        "task.purge",
        json!({"id":task_id,"revision":task["revision"]}),
    )
    .await
    .unwrap();
    let third = change(&db,"task.create",json!({"parent":project["record"]["id"],"record":{"objective":"Third","criteria":["Checked"]}})).await.unwrap();
    assert_eq!(
        ttjcore::document::sequence(third["record"]["id"].as_str().unwrap())
            .unwrap()
            .1,
        3
    );
    assert!(change(&db,"task.create",json!({"parent":project["record"]["id"],"record":{"id":task_id,"objective":"Reuse","criteria":["Rejected"]}})).await.is_err());
}

#[tokio::test]
async fn concurrent_reverse_waits_cannot_both_commit_and_static_edges_also_count() {
    let path = std::env::temp_dir().join(format!("ttj-graph-{}.sqlite", uuid::Uuid::new_v4()));
    let url = format!(
        "sqlite://{}?mode=rwc",
        path.to_string_lossy().replace('\\', "/")
    );
    let db = connect(&url, true).await.unwrap();
    let other = connect(&url, false).await.unwrap();
    seed(&db).await;
    change(&db, "ticket.create", ticket("tkt-ABC123-2", "job-ABC123-2"))
        .await
        .unwrap();
    start(&db, JOB, RUN).await.unwrap();
    start(&db, "job-ABC123-2", "run-ABC123-2").await.unwrap();
    let (left, right) = tokio::join!(
        wait(&db, RUN, json!({"target":"tkt-ABC123-2"})),
        wait(&other, "run-ABC123-2", json!({"target":TICKET})),
    );
    assert_ne!(left.is_ok(), right.is_ok());
    db.db.close().await.unwrap();
    other.db.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
    let db = connect("sqlite::memory:", true).await.unwrap();
    seed(&db).await;
    let mut second = ticket("tkt-ABC123-2", "job-ABC123-2");
    second["record"]["depends"] = json!([TICKET]);
    change(&db, "ticket.create", second).await.unwrap();
    start(&db, JOB, RUN).await.unwrap();
    assert!(
        wait(&db, RUN, json!({"target":"tkt-ABC123-2"}))
            .await
            .is_err()
    );
    let mut value = db
        .call("document.export", json!({"project":PROJECT}))
        .await
        .unwrap();
    value["tickets"][0]["jobs"][0]["runs"][0]["wait"] = json!({"target":"tkt-ABC123-2"});
    assert!(Document::parse(&value).is_err());
}

async fn connect(url: &str, initialize: bool) -> ttjcore::Result<Store> {
    Store::connect(url, initialize)
        .await?
        .with_management_actors(["owner"])
}
