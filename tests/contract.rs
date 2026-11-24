use serde_json::{Value, json};
use ttjcore::{PROTOCOL, rpc, store::Store};
async fn db() -> Store {
    Store::connect("sqlite::memory:", true).await.unwrap()
}
async fn change(db: &Store, method: &str, mut p: Value) -> ttjcore::Result<Value> {
    p["actor"] = json!(["wkr-abcdef-1", "ssn-abcdef-1"]);
    p["reason"] = json!("Contract test");
    db.call(method, p).await
}
async fn get(db: &Store, kind: &str, id: &str) -> Value {
    db.call(&format!("{kind}.get"), json!({"id":id}))
        .await
        .unwrap()
}
async fn seed(db: &Store) {
    change(
        db,
        "worker.create",
        json!({"record":{"id":"wkr-abcdef-1","name":"Operator"}}),
    )
    .await
    .unwrap();
    change(
        db,
        "session.create",
        json!({"record":{"id":"ssn-abcdef-1","worker_id":"wkr-abcdef-1"}}),
    )
    .await
    .unwrap();

    change(
        db,
        "project.create",
        json!({"record":{"id":"pj-abcdef-1","name":"Demo"}}),
    )
    .await
    .unwrap();
    change(
        db,
        "task.create",
        json!({"parent":"pj-abcdef-1","record":{"id":"tsk-abcdef-1","objective":"Deliver","criteria":["Verified"]}}),
    )
    .await
    .unwrap();
}
async fn ticket(db: &Store, id: &str, jobs: &[&str], depends: &[&str]) -> Value {
    change(db,"ticket.create",json!({"record":{"id":id,"worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":"tsk-abcdef-1","objective":id,"depends":depends,
 "jobs":jobs.iter().map(|id|json!({"id":id,"objective":id,"criteria":["Verified"]})).collect::<Vec<_>>()}})).await.unwrap()
}
async fn state(
    db: &Store,
    id: &str,
    kind: &str,
    status: &str,
    extra: Value,
) -> ttjcore::Result<Value> {
    let r = get(db, kind, id).await;
    let mut p = json!({"id":id,"revision":r["revision"],"status":status});
    if kind == "ticket" && status == "canceled" {
        p["cancel_reason"] = json!("Scope canceled");
    }
    for (k, v) in extra.as_object().unwrap() {
        p[k] = v.clone();
    }
    change(db, "status.set", p).await
}
async fn start(db: &Store, job: &str, id: &str) -> ttjcore::Result<Value> {
    let j = get(db, "job", job).await;
    change(
        db,
        "run.start",
        json!({"id":id,"job":job,"revision":j["revision"],"safety":"Previous execution stopped"}),
    )
    .await
}
async fn finish(db: &Store, id: &str) -> ttjcore::Result<Value> {
    let r = get(db, "run", id).await;
    change(
        db,
        "run.finish",
        json!({"id":id,"revision":r["revision"],"result":"Finished"}),
    )
    .await
}
async fn complete(db: &Store, id: &str) {
    state(
        db,
        id,
        "job",
        "completed",
        json!({"criteria_met":true,"result":"All criteria checked"}),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn atomic_create_replay_and_conflicts() {
    let db = db().await;
    seed(&db).await;
    assert!(change(&db,"ticket.create",json!({"record":{"id":"tkt-abcdef-7","worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":"tsk-abcdef-1","objective":"Bad","jobs":[
 {"id":"job-abcdef-7","objective":"First","criteria":["OK"]},{"id":"job-abcdef-8","objective":"Second","criteria":[]}]}})).await.is_err());
    assert!(
        db.call("job.get", json!({"id":"job-abcdef-7"}))
            .await
            .is_err()
    );
    let p = json!({"operation":"once","record":{"id":"pj-abcdef-2","name":"Second"}});
    let first = change(&db, "project.create", p.clone()).await.unwrap();
    assert_eq!(first, change(&db, "project.create", p).await.unwrap());
    assert_eq!(
        change(
            &db,
            "project.create",
            json!({"operation":"once","record":{"id":"pj-abcdef-3","name":"Other"}})
        )
        .await
        .unwrap_err()
        .code,
        -32009
    );
    let r = get(&db, "project", "pj-abcdef-2").await;
    change(
        &db,
        "project.update",
        json!({"id":"pj-abcdef-2","revision":r["revision"],"patch":{"name":"Updated"}}),
    )
    .await
    .unwrap();
    assert_eq!(
        change(
            &db,
            "project.update",
            json!({"id":"pj-abcdef-2","revision":r["revision"],"patch":{"name":"Stale"}})
        )
        .await
        .unwrap_err()
        .code,
        -32009
    );
    assert_eq!(
        get(&db, "project", "pj-abcdef-2").await["record"]["name"],
        "Updated"
    );
}
#[tokio::test]
async fn fixed_plan_dependencies_and_job_order() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "tkt-abcdef-1", &["job-abcdef-1", "job-abcdef-2"], &[]).await;
    ticket(&db, "tkt-abcdef-2", &["job-abcdef-3"], &["tkt-abcdef-1"]).await;
    let a = get(&db, "ticket", "tkt-abcdef-1").await;
    for patch in [
        json!({"objective":"Changed"}),
        json!({"depends":["tkt-abcdef-2"]}),
    ] {
        assert!(
            change(
                &db,
                "ticket.update",
                json!({"id":"tkt-abcdef-1","revision":a["revision"],"patch":patch})
            )
            .await
            .is_err()
        );
    }
    assert!(start(&db, "job-abcdef-2", "run-abcdef-8").await.is_err());
    assert!(start(&db, "job-abcdef-3", "run-abcdef-9").await.is_err());
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-2").await["record"]["status"],
        "waiting"
    );
}
#[tokio::test]
async fn run_results_and_completion_aggregation() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "tkt-abcdef-1", &["job-abcdef-1", "job-abcdef-2"], &[]).await;
    ticket(&db, "tkt-abcdef-2", &["job-abcdef-3"], &["tkt-abcdef-1"]).await;
    start(&db, "job-abcdef-1", "run-abcdef-1").await.unwrap();
    assert_eq!(
        get(&db, "task", "tsk-abcdef-1").await["record"]["status"],
        "doing"
    );
    finish(&db, "run-abcdef-1").await.unwrap();
    assert_eq!(
        state(
            &db,
            "job-abcdef-1",
            "job",
            "waiting",
            json!({"run":"run-abcdef-1"})
        )
        .await
        .unwrap_err()
        .code,
        -32010
    );
    assert_eq!(
        get(&db, "job", "job-abcdef-1").await["record"]["status"],
        "doing"
    );
    complete(&db, "job-abcdef-1").await;
    state(&db, "job-abcdef-2", "job", "canceled", json!({}))
        .await
        .unwrap();
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-1").await["record"]["status"],
        "completed"
    );
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-2").await["record"]["status"],
        "todo"
    );
    assert!(
        state(
            &db,
            "tsk-abcdef-1",
            "task",
            "completed",
            json!({"criteria_met":true,"result":"Claim"})
        )
        .await
        .is_err()
    );
    state(&db, "tkt-abcdef-2", "ticket", "canceled", json!({}))
        .await
        .unwrap();
    state(
        &db,
        "tsk-abcdef-1",
        "task",
        "completed",
        json!({"criteria_met":true,"result":"Scope checked"}),
    )
    .await
    .unwrap();
    assert!(
        change(
            &db,
            "ticket.create",
            json!({"record":{"id":"tkt-abcdef-6","worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":"tsk-abcdef-1","objective":"Late",
 "jobs":[{"id":"job-abcdef-6","objective":"Late","criteria":["OK"]}]}})
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn cancellation_is_terminal_and_revokes_runs() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "tkt-abcdef-1", &["job-abcdef-1"], &[]).await;
    start(&db, "job-abcdef-1", "run-abcdef-1").await.unwrap();
    state(&db, "tkt-abcdef-1", "ticket", "canceled", json!({}))
        .await
        .unwrap();
    assert_eq!(
        get(&db, "job", "job-abcdef-1").await["record"]["status"],
        "canceled"
    );
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-1").await["record"]["status"],
        "canceled"
    );
    assert!(finish(&db, "run-abcdef-1").await.is_err());
    assert!(
        state(&db, "tkt-abcdef-1", "ticket", "completed", json!({}))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn pause_retains_issues_and_aggregation_overrides_pause() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "tkt-abcdef-1", &["job-abcdef-1"], &[]).await;
    start(&db, "job-abcdef-1", "run-abcdef-1").await.unwrap();
    state(&db, "tkt-abcdef-1", "ticket", "paused", json!({}))
        .await
        .unwrap();
    state(&db, "job-abcdef-1", "job", "blocked", json!({}))
        .await
        .unwrap();
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-1").await["record"]["status"],
        "paused"
    );
    assert!(
        state(
            &db,
            "tkt-abcdef-1",
            "ticket",
            "doing",
            json!({"resolved":true})
        )
        .await
        .is_err()
    );
    state(
        &db,
        "job-abcdef-1",
        "job",
        "doing",
        json!({"resolved":true}),
    )
    .await
    .unwrap();
    finish(&db, "run-abcdef-1").await.unwrap();
    complete(&db, "job-abcdef-1").await;
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-1").await["record"]["status"],
        "completed"
    );
}
#[tokio::test]
async fn blocked_waiting_and_unstarted_recovery() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "tkt-abcdef-1", &["job-abcdef-1"], &[]).await;
    state(&db, "job-abcdef-1", "job", "blocked", json!({}))
        .await
        .unwrap();
    state(
        &db,
        "job-abcdef-1",
        "job",
        "waiting",
        json!({"resolved":true}),
    )
    .await
    .unwrap();
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-1").await["record"]["status"],
        "waiting"
    );
    state(&db, "job-abcdef-1", "job", "todo", json!({"resolved":true}))
        .await
        .unwrap();
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-1").await["record"]["status"],
        "todo"
    );
}
#[tokio::test]
async fn deletion_retains_dependencies_history_and_knowledge() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "tkt-abcdef-1", &["job-abcdef-1"], &[]).await;
    ticket(&db, "tkt-abcdef-2", &["job-abcdef-3"], &["tkt-abcdef-1"]).await;
    change(
        &db,
        "knowledge.create",
        json!({"parent":"pj-abcdef-1","record":{"id":"k","content":"Decision","basis":"Verified"}}),
    )
    .await
    .unwrap();
    state(&db, "job-abcdef-1", "job", "canceled", json!({}))
        .await
        .unwrap();
    let a = get(&db, "ticket", "tkt-abcdef-1").await;
    change(
        &db,
        "ticket.delete",
        json!({"id":"tkt-abcdef-1","revision":a["revision"]}),
    )
    .await
    .unwrap();
    assert!(
        db.call("ticket.get", json!({"id":"tkt-abcdef-1"}))
            .await
            .is_err()
    );
    assert_eq!(
        db.call(
            "ticket.get",
            json!({"id":"tkt-abcdef-1","include_deleted":true})
        )
        .await
        .unwrap()["record"]["status"],
        "completed"
    );
    start(&db, "job-abcdef-3", "run-abcdef-2").await.unwrap();
    finish(&db, "run-abcdef-2").await.unwrap();
    complete(&db, "job-abcdef-3").await;
    state(
        &db,
        "tsk-abcdef-1",
        "task",
        "completed",
        json!({"criteria_met":true,"result":"Checked"}),
    )
    .await
    .unwrap();
    let t = get(&db, "task", "tsk-abcdef-1").await;
    change(
        &db,
        "task.delete",
        json!({"id":"tsk-abcdef-1","revision":t["revision"]}),
    )
    .await
    .unwrap();
    let snapshot = db
        .call("project.snapshot", json!({"id":"pj-abcdef-1"}))
        .await
        .unwrap();
    assert!(snapshot["tasks"].as_array().unwrap().is_empty());
    assert_eq!(snapshot["knowledge"][0]["record"]["id"], "k");
    assert!(
        snapshot["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"] == "record.deleted")
    );
}
#[tokio::test]
async fn knowledge_replacement_preserves_old_content() {
    let db = db().await;
    seed(&db).await;
    change(
        &db,
        "knowledge.create",
        json!({"parent":"pj-abcdef-1","record":{"id":"old","content":"Old","basis":"Initial"}}),
    )
    .await
    .unwrap();
    change(&db,"knowledge.create",json!({"parent":"pj-abcdef-1","record":{"id":"new","content":"New","basis":"Correction","supersedes":"old"}})).await.unwrap();
    assert_eq!(
        get(&db, "knowledge", "old").await["record"]["status"],
        "supersed"
    );
    let r = get(&db, "knowledge", "new").await;
    assert!(
        change(
            &db,
            "knowledge.update",
            json!({"id":"new","revision":r["revision"],"patch":{"content":"Overwrite"}})
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn rpc_protocol_errors_batches_and_notifications() {
    let db = db().await;
    for params in [json!({"parent":false}), json!({"project":1})] {
        assert_eq!(
            db.call("project.list", params).await.unwrap_err().code,
            -32602
        );
    }
    assert_eq!(
        rpc::dispatch(&db, "{").await.unwrap()["error"]["code"],
        -32700
    );
    let r = json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":"llm-1","method":"project.list"});
    assert_eq!(
        rpc::dispatch(&db, &r.to_string()).await.unwrap()["id"],
        "llm-1"
    );
    assert_eq!(
        rpc::dispatch(&db, r#"{"jsonrpc":"2.0","id":1,"method":"project.list"}"#)
            .await
            .unwrap()["error"]["code"],
        -32602
    );
    let n = json!({"jsonrpc":"2.0","protocol":PROTOCOL,"method":"project.list"});
    assert!(rpc::dispatch(&db, &n.to_string()).await.is_none());
    assert_eq!(
        rpc::dispatch(&db, &json!([n, r]).to_string())
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn postgres_contract() {
    let db = Store::connect(
        &std::env::var("TTJCORE_TEST_POSTGRES_URL").expect("Set test database URL"),
        true,
    )
    .await
    .unwrap();
    let series = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let id = format!("pj-{series}-1");
    change(
        &db,
        "worker.create",
        json!({"record":{"id":"wkr-abcdef-1","name":"Operator"}}),
    )
    .await
    .unwrap();
    change(
        &db,
        "session.create",
        json!({"record":{"id":"ssn-abcdef-1","worker_id":"wkr-abcdef-1"}}),
    )
    .await
    .unwrap();
    let task = format!("tsk-{series}-1");
    let ticket_id = format!("tkt-{series}-1");
    let job = format!("job-{series}-1");
    let run = format!("run-{series}-1");
    let knowledge = format!("knowledge-{id}");
    change(
        &db,
        "project.create",
        json!({"record":{"id":id,"name":"PostgreSQL contract"}}),
    )
    .await
    .unwrap();
    let r = get(&db, "project", &id).await;
    change(
        &db,
        "project.update",
        json!({"id":id,"revision":r["revision"],"patch":{"name":"Updated"}}),
    )
    .await
    .unwrap();
    let create = json!({"operation":format!("create-{task}"),"parent":id,"record":{
        "id":task,"objective":"Verify PostgreSQL","criteria":["Contract verified"]}});
    let created = change(&db, "task.create", create.clone()).await.unwrap();
    assert_eq!(created, change(&db, "task.create", create).await.unwrap());
    let invalid_job = format!("invalid-{job}");
    assert!(
        change(
            &db,
            "ticket.create",
            json!({"record":{
        "id":format!("invalid-{ticket_id}"),"worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":task,"objective":"Invalid plan",
        "jobs":[{"id":invalid_job,"objective":"Invalid","criteria":[]}]}})
        )
        .await
        .is_err()
    );
    assert!(db.call("job.get", json!({"id":invalid_job})).await.is_err());
    change(&db,"ticket.create",json!({"record":{
        "id":ticket_id,"worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":task,"objective":"Work","jobs":[{"id":job,"objective":"Verify","criteria":["Checked"]}]}})).await.unwrap();
    change(
        &db,
        "knowledge.create",
        json!({"parent":id,"record":{
        "id":knowledge,"content":"Keep after Task deletion","basis":"Contract test"}}),
    )
    .await
    .unwrap();
    start(&db, &job, &run).await.unwrap();
    finish(&db, &run).await.unwrap();
    complete(&db, &job).await;
    assert_eq!(
        get(&db, "ticket", &ticket_id).await["record"]["status"],
        "completed"
    );
    state(
        &db,
        &task,
        "task",
        "completed",
        json!({"criteria_met":true,"result":"All checks complete"}),
    )
    .await
    .unwrap();
    let task_row = get(&db, "task", &task).await;
    change(
        &db,
        "task.purge",
        json!({"id":task,"revision":task_row["revision"]}),
    )
    .await
    .unwrap();
    let snapshot = db.call("project.snapshot", json!({"id":id})).await.unwrap();
    assert!(snapshot["tasks"].as_array().unwrap().is_empty());
    assert_eq!(snapshot["knowledge"][0]["record"]["id"], knowledge);
    assert!(
        snapshot["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"] == "record.deleted")
    );
    let r = get(&db, "project", &id).await;
    change(
        &db,
        "project.delete",
        json!({"id":id,"revision":r["revision"]}),
    )
    .await
    .unwrap();
    assert!(db.call("project.get", json!({"id":id})).await.is_err());
    assert!(
        !db.call(
            "timeline.list",
            json!({"project":id,"include_deleted":true})
        )
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty()
    );
}

#[tokio::test]
async fn ticket_identity_is_fixed_and_only_one_run_is_open() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "tkt-abcdef-1", &["job-abcdef-1"], &[]).await;
    start(&db, "job-abcdef-1", "run-abcdef-1").await.unwrap();
    assert!(start(&db, "job-abcdef-1", "run-abcdef-2").await.is_err());
    let row = get(&db, "ticket", "tkt-abcdef-1").await;
    for patch in [
        json!({"worker_id":"wkr-abcdef-2"}),
        json!({"session_id":"ssn-abcdef-2"}),
        json!({"assignee":["operator"]}),
    ] {
        assert!(
            change(
                &db,
                "ticket.update",
                json!({"id":row["record"]["id"],"revision":row["revision"],"patch":patch})
            )
            .await
            .is_err()
        );
    }
    let run = get(&db, "run", "run-abcdef-1").await;
    assert!(db.call("run.finish", json!({"id":run["record"]["id"],"revision":run["revision"],"actor":["wkr-abcdef-1","ssn-abcdef-2"],"reason":"Wrong session","result":"succeeded"})).await.is_err());
    finish(&db, "run-abcdef-1").await.unwrap();
    start(&db, "job-abcdef-1", "run-abcdef-2").await.unwrap();
    assert!(finish(&db, "run-abcdef-1").await.is_err());
}
#[tokio::test]
async fn concurrent_changes_with_the_same_revision_commit_only_once() {
    let path = std::env::temp_dir().join(format!("ttjcore-{}.sqlite", uuid::Uuid::new_v4()));
    let url = format!(
        "sqlite://{}?mode=rwc",
        path.to_string_lossy().replace('\\', "/")
    );
    let first = Store::connect(&url, true).await.unwrap();
    let second = Store::connect(&url, false).await.unwrap();
    seed(&first).await;
    let project = get(&first, "project", "pj-abcdef-1").await;
    let left = json!({"id":"pj-abcdef-1","revision":project["revision"],"patch":{"name":"Left"}});
    let right = json!({"id":"pj-abcdef-1","revision":project["revision"],"patch":{"name":"Right"}});
    let (left, right) = tokio::join!(
        change(&first, "project.update", left),
        change(&second, "project.update", right)
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let failed = if let Err(e) = left {
        e
    } else {
        right.unwrap_err()
    };
    assert_eq!(failed.code, -32009);
    first.db.close().await.unwrap();
    second.db.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn generated_ids_replay_and_display_metadata_stay_outside_ttj_records() {
    fn short_id(id: &Value, prefix: &str) {
        assert!(id.as_str().unwrap().starts_with(prefix));
        assert!(ttjcore::document::is_id(id.as_str().unwrap()));
    }
    async fn auto(db: &Store, method: &str, p: Value) -> ttjcore::Result<Value> {
        change(db, method, p)
            .await
            .map(|value| value["value"].clone())
    }

    let db = db().await;
    change(
        &db,
        "worker.create",
        json!({"record":{"id":"wkr-abcdef-1","name":"Operator"}}),
    )
    .await
    .unwrap();
    change(
        &db,
        "session.create",
        json!({"record":{"id":"ssn-abcdef-1","worker_id":"wkr-abcdef-1"}}),
    )
    .await
    .unwrap();
    let request = json!({"operation":"auto-project","record":{"name":"Automatic IDs"}});
    let project = auto(&db, "project.create", request.clone()).await.unwrap();
    assert_eq!(project, auto(&db, "project.create", request).await.unwrap());
    short_id(&project["record"]["id"], "pj-");
    let task = auto(
        &db,
        "task.create",
        json!({"parent":project["record"]["id"],"record":{
        "objective":"Deliver","criteria":["Verified"]}}),
    )
    .await
    .unwrap();
    short_id(&task["record"]["id"], "tsk-");
    let request = json!({"operation":"auto-ticket","display":{"summary":"Compact card"},
        "record":{"worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":task["record"]["id"],"objective":"Review","jobs":[
        {"objective":"Prepare","criteria":["Ready"],"display":{"icon":"job"}},
        {"objective":"GitHub review","criteria":["Approved"],"display":{"icon":"github"}}]}});
    let ticket = auto(&db, "ticket.create", request.clone()).await.unwrap();
    assert_eq!(ticket, auto(&db, "ticket.create", request).await.unwrap());
    short_id(&ticket["record"]["id"], "tkt-");
    assert_eq!(ticket["display"]["summary"], "Compact card");
    assert!(ticket["record"].get("summary").is_none());
    let ids = ticket["record"]["jobs"].as_array().unwrap();
    for id in ids {
        short_id(&id["id"], "job-");
    }
    assert_ne!(ids[0], ids[1]);
    let job = get(&db, "job", ids[1]["id"].as_str().unwrap()).await;
    assert_eq!(job["display"]["icon"], "github");
    assert!(job["record"].get("display").is_none());
    let updated = auto(
        &db,
        "job.update",
        json!({"id":job["record"]["id"],"revision":job["revision"],
        "patch":{},"display":{"icon":"custom.star"}}),
    )
    .await
    .unwrap();
    assert_eq!(updated["display"]["icon"], "custom.star");
    let first = get(&db, "job", ids[0]["id"].as_str().unwrap()).await;
    let start = json!({"operation":"auto-run","job":ids[0]["id"],"revision":first["revision"]});
    let run = auto(&db, "run.start", start.clone()).await.unwrap();
    short_id(&run["run"]["record"]["id"], "run-");
    assert_eq!(run, auto(&db, "run.start", start).await.unwrap());
    let knowledge = auto(
        &db,
        "knowledge.create",
        json!({"parent":project["record"]["id"],"record":{"content":"Short IDs","basis":"Contract test"}}),
    )
    .await
    .unwrap();
    assert!(!knowledge["record"]["id"].as_str().unwrap().is_empty());
    let all_ids = [
        &project["record"]["id"],
        &task["record"]["id"],
        &ticket["record"]["id"],
        &ids[0]["id"],
        &ids[1]["id"],
        &run["run"]["record"]["id"],
        &knowledge["record"]["id"],
    ];
    let unique: std::collections::BTreeSet<_> =
        all_ids.iter().map(|id| id.as_str().unwrap()).collect();
    assert_eq!(unique.len(), all_ids.len());
    assert!(
        auto(
            &db,
            "project.create",
            json!({"record":{"id":project["record"]["id"],"name":"Duplicate"}})
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn legacy_and_wrong_kind_ids_are_rejected() {
    let db = db().await;
    for id in [
        "550e8400-e29b-41d4-a716-446655440000",
        "pj-abcdef-01",
        "tsk-abcdef-1",
        "pj-abcdef-0",
    ] {
        assert!(
            change(
                &db,
                "project.create",
                json!({"record":{"id":id,"name":"Invalid"}})
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn snapshot_resolves_current_and_historical_dependencies_across_projects_and_deletion() {
    let db = db().await;
    seed(&db).await;
    change(
        &db,
        "project.create",
        json!({"record":{"id":"pj-abcdef-4","name":"Other"}}),
    )
    .await
    .unwrap();
    change(&db,"task.create",json!({"parent":"pj-abcdef-4","record":{"id":"tsk-abcdef-2","objective":"Approval","criteria":["Approved"]}})).await.unwrap();
    change(&db,"ticket.create",json!({"record":{"id":"tkt-abcdef-4","worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":"tsk-abcdef-2","objective":"External approval","jobs":[
        {"id":"job-abcdef-4","objective":"Approve","criteria":["Reviewed"]}]}})).await.unwrap();
    ticket(&db, "tkt-abcdef-8", &["job-abcdef-9"], &[]).await;
    ticket(&db, "tkt-abcdef-5", &["job-abcdef-5"], &["tkt-abcdef-4"]).await;
    let snapshot = db
        .call("project.snapshot", json!({"id":"pj-abcdef-1"}))
        .await
        .unwrap();
    let dependencies = snapshot["dependencies"].as_array().unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0]["record"]["objective"], "External approval");
    assert_eq!(dependencies[0]["deleted"], false);
    start(&db, "job-abcdef-4", "run-abcdef-4").await.unwrap();
    finish(&db, "run-abcdef-4").await.unwrap();
    complete(&db, "job-abcdef-4").await;
    let row = get(&db, "ticket", "tkt-abcdef-4").await;
    change(
        &db,
        "ticket.delete",
        json!({"id":"tkt-abcdef-4","revision":row["revision"]}),
    )
    .await
    .unwrap();
    let snapshot = db
        .call("project.snapshot", json!({"id":"pj-abcdef-1"}))
        .await
        .unwrap();
    assert_eq!(snapshot["dependencies"][0]["deleted"], true);
    assert_eq!(
        snapshot["dependencies"][0]["record"]["objective"],
        "External approval"
    );
    let row = get(&db, "ticket", "tkt-abcdef-5").await;
    change(
        &db,
        "ticket.update",
        json!({"id":"tkt-abcdef-5","revision":row["revision"],"patch":{"depends":[]}}),
    )
    .await
    .unwrap();
    let snapshot = db
        .call("project.snapshot", json!({"id":"pj-abcdef-1"}))
        .await
        .unwrap();
    assert_eq!(snapshot["dependencies"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["dependencies"][0]["record"]["id"], "tkt-abcdef-4");
    assert!(
        snapshot["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["from"]["record"]["depends"] == json!(["tkt-abcdef-4"]))
    );
}
