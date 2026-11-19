use serde_json::{Value, json};
use ttjcore::{PROTOCOL, rpc, store::Store};
async fn db() -> Store {
    Store::connect("sqlite::memory:", true).await.unwrap()
}
async fn change(db: &Store, method: &str, mut p: Value) -> ttjcore::Result<Value> {
    p["actor"] = json!(["operator"]);
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
        "project.create",
        json!({"record":{"id":"p","name":"Demo"}}),
    )
    .await
    .unwrap();
    change(
        db,
        "task.create",
        json!({"record":{"id":"t","project":"p","objective":"Deliver","criteria":["Verified"]}}),
    )
    .await
    .unwrap();
}
async fn ticket(db: &Store, id: &str, jobs: &[&str], depends: &[&str]) -> Value {
    change(db,"ticket.create",json!({"record":{"id":id,"task":"t","objective":id,"depends":depends,
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
        json!({"id":id,"job":job,"revision":j["revision"]}),
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
    assert!(change(&db,"ticket.create",json!({"record":{"id":"bad","task":"t","objective":"Bad","jobs":[
 {"id":"orphan","objective":"First","criteria":["OK"]},{"id":"invalid","objective":"Second","criteria":[]}]}})).await.is_err());
    assert!(db.call("job.get", json!({"id":"orphan"})).await.is_err());
    let p = json!({"operation":"once","record":{"id":"p2","name":"Second"}});
    let first = change(&db, "project.create", p.clone()).await.unwrap();
    assert_eq!(first, change(&db, "project.create", p).await.unwrap());
    assert_eq!(
        change(
            &db,
            "project.create",
            json!({"operation":"once","record":{"id":"p3","name":"Other"}})
        )
        .await
        .unwrap_err()
        .code,
        -32009
    );
    let r = get(&db, "project", "p2").await;
    change(
        &db,
        "project.update",
        json!({"id":"p2","revision":r["revision"],"patch":{"name":"Updated"}}),
    )
    .await
    .unwrap();
    assert_eq!(
        change(
            &db,
            "project.update",
            json!({"id":"p2","revision":r["revision"],"patch":{"name":"Stale"}})
        )
        .await
        .unwrap_err()
        .code,
        -32009
    );
    assert_eq!(get(&db, "project", "p2").await["record"]["name"], "Updated");
}
#[tokio::test]
async fn fixed_plan_dependencies_and_job_order() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "a", &["a1", "a2"], &[]).await;
    ticket(&db, "b", &["b1"], &["a"]).await;
    let a = get(&db, "ticket", "a").await;
    for patch in [json!({"objective":"Changed"}), json!({"depends":["b"]})] {
        assert!(
            change(
                &db,
                "ticket.update",
                json!({"id":"a","revision":a["revision"],"patch":patch})
            )
            .await
            .is_err()
        );
    }
    assert!(start(&db, "a2", "wrong-order").await.is_err());
    assert!(start(&db, "b1", "wrong-dependency").await.is_err());
    assert_eq!(get(&db, "ticket", "b").await["record"]["status"], "waiting");
}
#[tokio::test]
async fn run_results_and_completion_aggregation() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "a", &["a1", "a2"], &[]).await;
    ticket(&db, "b", &["b1"], &["a"]).await;
    start(&db, "a1", "r").await.unwrap();
    assert_eq!(get(&db, "task", "t").await["record"]["status"], "doing");
    finish(&db, "r").await.unwrap();
    assert_eq!(
        state(&db, "a1", "job", "waiting", json!({"run":"r"}))
            .await
            .unwrap_err()
            .code,
        -32010
    );
    assert_eq!(get(&db, "job", "a1").await["record"]["status"], "doing");
    complete(&db, "a1").await;
    state(&db, "a2", "job", "canceled", json!({}))
        .await
        .unwrap();
    assert_eq!(
        get(&db, "ticket", "a").await["record"]["status"],
        "completed"
    );
    assert_eq!(get(&db, "ticket", "b").await["record"]["status"], "todo");
    assert!(
        state(
            &db,
            "t",
            "task",
            "completed",
            json!({"criteria_met":true,"result":"Claim"})
        )
        .await
        .is_err()
    );
    state(&db, "b", "ticket", "canceled", json!({}))
        .await
        .unwrap();
    state(
        &db,
        "t",
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
            json!({"record":{"id":"late","task":"t","objective":"Late",
 "jobs":[{"id":"late-job","objective":"Late","criteria":["OK"]}]}})
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn cancellation_is_terminal_and_revokes_runs() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "a", &["a1"], &[]).await;
    start(&db, "a1", "r").await.unwrap();
    state(&db, "a", "ticket", "canceled", json!({}))
        .await
        .unwrap();
    assert_eq!(get(&db, "job", "a1").await["record"]["status"], "canceled");
    assert_eq!(
        get(&db, "ticket", "a").await["record"]["status"],
        "canceled"
    );
    assert!(finish(&db, "r").await.is_err());
    assert!(
        state(&db, "a", "ticket", "completed", json!({}))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn pause_retains_issues_and_aggregation_overrides_pause() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "a", &["a1"], &[]).await;
    start(&db, "a1", "r").await.unwrap();
    state(&db, "a", "ticket", "paused", json!({}))
        .await
        .unwrap();
    state(&db, "a1", "job", "blocked", json!({})).await.unwrap();
    assert_eq!(get(&db, "ticket", "a").await["record"]["status"], "paused");
    assert!(
        state(&db, "a", "ticket", "doing", json!({"resolved":true}))
            .await
            .is_err()
    );
    state(&db, "a1", "job", "doing", json!({"resolved":true}))
        .await
        .unwrap();
    finish(&db, "r").await.unwrap();
    complete(&db, "a1").await;
    assert_eq!(
        get(&db, "ticket", "a").await["record"]["status"],
        "completed"
    );
}
#[tokio::test]
async fn blocked_waiting_and_unstarted_recovery() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "a", &["a1"], &[]).await;
    state(&db, "a1", "job", "blocked", json!({})).await.unwrap();
    state(&db, "a1", "job", "waiting", json!({"resolved":true}))
        .await
        .unwrap();
    assert_eq!(get(&db, "ticket", "a").await["record"]["status"], "waiting");
    state(&db, "a1", "job", "todo", json!({"resolved":true}))
        .await
        .unwrap();
    assert_eq!(get(&db, "ticket", "a").await["record"]["status"], "todo");
}
#[tokio::test]
async fn deletion_retains_dependencies_history_and_knowledge() {
    let db = db().await;
    seed(&db).await;
    ticket(&db, "a", &["a1"], &[]).await;
    ticket(&db, "b", &["b1"], &["a"]).await;
    change(
        &db,
        "knowledge.create",
        json!({"record":{"id":"k","project":"p","content":"Decision","basis":"Verified"}}),
    )
    .await
    .unwrap();
    state(&db, "a1", "job", "canceled", json!({}))
        .await
        .unwrap();
    let a = get(&db, "ticket", "a").await;
    change(
        &db,
        "ticket.delete",
        json!({"id":"a","revision":a["revision"]}),
    )
    .await
    .unwrap();
    assert!(db.call("ticket.get", json!({"id":"a"})).await.is_err());
    assert_eq!(
        db.call("ticket.get", json!({"id":"a","include_deleted":true}))
            .await
            .unwrap()["record"]["status"],
        "completed"
    );
    start(&db, "b1", "rb").await.unwrap();
    finish(&db, "rb").await.unwrap();
    complete(&db, "b1").await;
    state(
        &db,
        "t",
        "task",
        "completed",
        json!({"criteria_met":true,"result":"Checked"}),
    )
    .await
    .unwrap();
    let t = get(&db, "task", "t").await;
    change(
        &db,
        "task.delete",
        json!({"id":"t","revision":t["revision"]}),
    )
    .await
    .unwrap();
    let snapshot = db
        .call("project.snapshot", json!({"id":"p"}))
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
        json!({"record":{"id":"old","project":"p","content":"Old","basis":"Initial"}}),
    )
    .await
    .unwrap();
    change(&db,"knowledge.create",json!({"record":{"id":"new","project":"p","content":"New","basis":"Correction","supersedes":"old"}})).await.unwrap();
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
    let id = uuid::Uuid::new_v4().to_string();
    let task = format!("task-{id}");
    let ticket_id = format!("ticket-{id}");
    let job = format!("job-{id}");
    let run = format!("run-{id}");
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
    let create = json!({"operation":format!("create-{task}"),"record":{
        "id":task,"project":id,"objective":"Verify PostgreSQL","criteria":["Contract verified"]}});
    let created = change(&db, "task.create", create.clone()).await.unwrap();
    assert_eq!(created, change(&db, "task.create", create).await.unwrap());
    let invalid_job = format!("invalid-{job}");
    assert!(
        change(
            &db,
            "ticket.create",
            json!({"record":{
        "id":format!("invalid-{ticket_id}"),"task":task,"objective":"Invalid plan",
        "jobs":[{"id":invalid_job,"objective":"Invalid","criteria":[]}]}})
        )
        .await
        .is_err()
    );
    assert!(db.call("job.get", json!({"id":invalid_job})).await.is_err());
    change(&db,"ticket.create",json!({"record":{
        "id":ticket_id,"task":task,"objective":"Work","jobs":[{"id":job,"objective":"Verify","criteria":["Checked"]}]}})).await.unwrap();
    change(
        &db,
        "knowledge.create",
        json!({"record":{
        "id":knowledge,"project":id,"content":"Keep after Task deletion","basis":"Contract test"}}),
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
        "task.delete",
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
async fn ticket_assignment_change_preserves_job_specific_run_authority() {
    let db = db().await;
    seed(&db).await;
    change(&db,"ticket.create",json!({"record":{"id":"a","task":"t","objective":"Work",
        "assignee":["default-one"],"jobs":[{"id":"a1","objective":"Work","criteria":["Verified"],"assignee":["operator"]}]}})).await.unwrap();
    start(&db, "a1", "r").await.unwrap();
    let a = get(&db, "ticket", "a").await;
    change(
        &db,
        "ticket.update",
        json!({"id":"a","revision":a["revision"],"patch":{"assignee":["default-two"]}}),
    )
    .await
    .unwrap();
    finish(&db, "r").await.unwrap();
}
#[tokio::test]
async fn joint_work_in_one_job_keeps_independent_run_authorities() {
    let db = db().await;
    seed(&db).await;
    change(
        &db,
        "ticket.create",
        json!({"record":{"id":"a","task":"t","objective":"Review",
        "assignee":["reviewer-a","reviewer-b"],
        "jobs":[{"id":"a1","objective":"Joint review","criteria":["Both reviewed"]}]}}),
    )
    .await
    .unwrap();
    let job = get(&db, "job", "a1").await;
    db.call("run.start",json!({"id":"ra","job":"a1","revision":job["revision"],"actor":["reviewer-a"],"reason":"First review"})).await.unwrap();
    let job = get(&db, "job", "a1").await;
    db.call("run.start",json!({"id":"rb","job":"a1","revision":job["revision"],"actor":["reviewer-b"],"reason":"Joint review"})).await.unwrap();
    let job = get(&db, "job", "a1").await;
    assert_eq!(
        db.call(
            "status.set",
            json!({"id":"a1","revision":job["revision"],
            "status":"waiting","run":"ra","actor":["reviewer-b"],"reason":"Wrong Run"})
        )
        .await
        .unwrap_err()
        .code,
        -32010
    );
    for (id, actor) in [("ra", "reviewer-a"), ("rb", "reviewer-b")] {
        let run = get(&db, "run", id).await;
        db.call("run.finish",json!({"id":id,"revision":run["revision"],"actor":[actor],"reason":"Review finished","result":"Checked"})).await.unwrap();
    }
    complete(&db, "a1").await;
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
    let project = get(&first, "project", "p").await;
    let left = json!({"id":"p","revision":project["revision"],"patch":{"name":"Left"}});
    let right = json!({"id":"p","revision":project["revision"],"patch":{"name":"Right"}});
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
    async fn auto(db: &Store, method: &str, p: Value) -> ttjcore::Result<Value> {
        change(db, method, p)
            .await
            .map(|value| value["value"].clone())
    }

    let db = db().await;
    let request = json!({"operation":"auto-project","record":{"name":"Automatic IDs"}});
    let project = auto(&db, "project.create", request.clone()).await.unwrap();
    assert_eq!(project, auto(&db, "project.create", request).await.unwrap());
    uuid::Uuid::parse_str(project["record"]["id"].as_str().unwrap()).unwrap();
    let task = auto(
        &db,
        "task.create",
        json!({"record":{"project":project["record"]["id"],
        "objective":"Deliver","criteria":["Verified"]}}),
    )
    .await
    .unwrap();
    let request = json!({"operation":"auto-ticket","display":{"summary":"Compact card"},
        "record":{"task":task["record"]["id"],"objective":"Review","jobs":[
        {"objective":"Prepare","criteria":["Ready"],"display":{"icon":"job"}},
        {"objective":"GitHub review","criteria":["Approved"],"display":{"icon":"github"}}]}});
    let ticket = auto(&db, "ticket.create", request.clone()).await.unwrap();
    assert_eq!(ticket, auto(&db, "ticket.create", request).await.unwrap());
    assert_eq!(ticket["display"]["summary"], "Compact card");
    assert!(ticket["record"].get("summary").is_none());
    let ids = ticket["record"]["jobs"].as_array().unwrap();
    assert_ne!(ids[0], ids[1]);
    let job = get(&db, "job", ids[1].as_str().unwrap()).await;
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
    let first = get(&db, "job", ids[0].as_str().unwrap()).await;
    let start = json!({"operation":"auto-run","job":ids[0],"revision":first["revision"]});
    let run = auto(&db, "run.start", start.clone()).await.unwrap();
    uuid::Uuid::parse_str(run["run"]["record"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(run, auto(&db, "run.start", start).await.unwrap());
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
