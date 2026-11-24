use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, EntityTrait};
use serde_json::{Value, json};
use ttjcore::{
    entity::{history, operation, record},
    store::Store,
};

async fn change(db: &Store, method: &str, mut params: Value) -> ttjcore::Result<Value> {
    params["actor"] = json!(["wkr-abcdef-1", "ssn-abcdef-1"]);
    if method != "task.purge" {
        params["reason"] = json!("Fixture operation");
    }
    db.call(method, params).await
}
async fn get(db: &Store, kind: &str, id: &str) -> Value {
    db.call(&format!("{kind}.get"), json!({"id":id}))
        .await
        .unwrap()
}
async fn status(db: &Store, kind: &str, id: &str, status: &str) {
    let row = get(db, kind, id).await;
    change(
        db,
        "status.set",
        json!({"id":id,"revision":row["revision"],"status":status,
        "criteria_met":true,"result":"Criteria verified"}),
    )
    .await
    .unwrap();
}
async fn seed(db: &Store, detail: &str) -> Value {
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
        json!({"operation":"project-create","record":{"id":"pj-abcdef-1","name":"Example"}}),
    )
    .await
    .unwrap();
    let create = json!({"operation":"task-create","parent":"pj-abcdef-1","record":{"id":"tsk-abcdef-1","objective":"Publish example","criteria":["Published"]}});
    change(db, "task.create", create.clone()).await.unwrap();
    change(db,"ticket.create",json!({"operation":"ticket-create","record":{"id":"tkt-abcdef-1","worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":"tsk-abcdef-1","objective":"Prepare example","jobs":[
        {"id":"job-abcdef-1","objective":"Check example","criteria":["Checked"],"instructions":detail}]}})).await.unwrap();
    change(db,"knowledge.create",json!({"parent":"pj-abcdef-1","record":{"id":"knowledge","content":"Durable project knowledge","basis":"Verified finding"}})).await.unwrap();
    create
}
async fn finish_work(db: &Store, detail: &str) {
    let job = get(db, "job", "job-abcdef-1").await;
    change(
        db,
        "run.start",
        json!({"id":"run-abcdef-1","job":"job-abcdef-1","revision":job["revision"]}),
    )
    .await
    .unwrap();
    let run = get(db, "run", "run-abcdef-1").await;
    change(
        db,
        "run.finish",
        json!({"id":"run-abcdef-1","revision":run["revision"],"result":detail}),
    )
    .await
    .unwrap();
    status(db, "job", "job-abcdef-1", "completed").await;
}

#[tokio::test]
async fn purge_removes_descendants_summarizes_history_preserves_knowledge_and_reclaims_sqlite() {
    let path = std::env::temp_dir().join(format!("ttj-purge-{}.sqlite", uuid::Uuid::new_v4()));
    let url = format!(
        "sqlite://{}?mode=rwc",
        path.to_string_lossy().replace('\\', "/")
    );
    let db = Store::connect(&url, true).await.unwrap();
    let detail = "ARCHIVED_DETAIL_".repeat(8192);
    let create = seed(&db, &detail).await;
    finish_work(&db, &detail).await;
    status(&db, "task", "tsk-abcdef-1", "completed").await;
    let task = get(&db, "task", "tsk-abcdef-1").await;
    change(
        &db,
        "task.delete",
        json!({"id":"tsk-abcdef-1","revision":task["revision"]}),
    )
    .await
    .unwrap();
    for event in history::Entity::find().all(&db.db).await.unwrap() {
        let without_kind = |value: Option<Value>| {
            value.map(|mut wrapper| {
                wrapper.as_object_mut().unwrap().remove("kind");
                wrapper
            })
        };
        let before = without_kind(event.before.clone());
        let after = without_kind(event.after.clone());
        let mut model: history::ActiveModel = event.into();
        model.before = Set(before);
        model.after = Set(after);
        model.update(&db.db).await.unwrap();
    }
    db.db
        .execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE)")
        .await
        .unwrap();
    let before = std::fs::metadata(&path).unwrap().len();
    let task = db
        .call(
            "task.get",
            json!({"id":"tsk-abcdef-1","include_deleted":true}),
        )
        .await
        .unwrap();
    let params = json!({"operation":"purge-once","id":"tsk-abcdef-1","revision":task["revision"]});
    let result = change(&db, "task.purge", params.clone()).await.unwrap();
    assert_eq!(result["value"]["records"], 4);
    assert_eq!(result["value"]["storage"], "reclaimed");
    for (kind, id) in [
        ("task", "tsk-abcdef-1"),
        ("ticket", "tkt-abcdef-1"),
        ("job", "job-abcdef-1"),
        ("run", "run-abcdef-1"),
    ] {
        assert_eq!(
            db.call(
                &format!("{kind}.get"),
                json!({"id":id,"include_deleted":true})
            )
            .await
            .unwrap_err()
            .code,
            -32004
        );
    }
    assert_eq!(record::Entity::find().all(&db.db).await.unwrap().len(), 4);
    let timeline = db
        .call("timeline.list", json!({"project":"pj-abcdef-1"}))
        .await
        .unwrap();
    for (target, kind) in [
        ("tsk-abcdef-1", "task"),
        ("tkt-abcdef-1", "ticket"),
        ("job-abcdef-1", "job"),
        ("run-abcdef-1", "run"),
        ("pj-abcdef-1", "project"),
        ("knowledge", "knowledge"),
    ] {
        let events: Vec<_> = timeline
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["target"] == target)
            .collect();
        assert!(!events.is_empty());
        assert!(events.iter().all(|event| event["kind"] == kind));
        if [
            "tsk-abcdef-1",
            "tkt-abcdef-1",
            "job-abcdef-1",
            "run-abcdef-1",
        ]
        .contains(&target)
        {
            assert!(events.iter().all(|event| event["to"]["kind"] == kind));
        }
    }
    assert!(!timeline.to_string().contains("ARCHIVED_DETAIL_"));
    assert!(
        timeline
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["target"] == "tsk-abcdef-1"
                && event["event"] == "record.deleted"
                && event["reason"] == "Taskを削除")
    );
    assert!(
        timeline
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["target"] == "tkt-abcdef-1"
                && event["to"]["record"]["objective"] == "Prepare example")
    );
    assert_eq!(
        get(&db, "knowledge", "knowledge").await["record"]["content"],
        "Durable project knowledge"
    );
    assert!(
        timeline
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["target"] == "run-abcdef-1"
                && event["to"]["record"]["objective"] == "Check example")
    );
    let operations = operation::Entity::find().all(&db.db).await.unwrap();
    assert!(!format!("{operations:?}").contains("ARCHIVED_DETAIL_"));
    assert_eq!(
        change(&db, "task.create", create).await.unwrap_err().code,
        -32009
    );
    assert_eq!(change(&db,"task.create",json!({"parent":"pj-abcdef-1","record":{"id":"tsk-abcdef-1","objective":"Reuse","criteria":["No"]}})).await.unwrap_err().code,-32009);
    let count = history::Entity::find().all(&db.db).await.unwrap().len();
    assert_eq!(change(&db, "task.purge", params).await.unwrap(), result);
    assert_eq!(
        history::Entity::find().all(&db.db).await.unwrap().len(),
        count
    );
    assert!(
        std::fs::metadata(&path).unwrap().len() < before / 2,
        "SQLite file should shrink after deleting bulky history"
    );
    db.db.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn purge_requires_a_terminal_task_rejects_references_and_keeps_historical_names() {
    let db = Store::connect("sqlite::memory:", true).await.unwrap();
    seed(&db, "Instructions").await;
    let task = get(&db, "task", "tsk-abcdef-1").await;
    assert_eq!(
        change(
            &db,
            "task.purge",
            json!({"id":"tsk-abcdef-1","revision":task["revision"]})
        )
        .await
        .unwrap_err()
        .code,
        -32010
    );
    finish_work(&db, "Run result").await;
    let task = get(&db, "task", "tsk-abcdef-1").await;
    assert_eq!(task["record"]["status"], "doing");
    assert_eq!(
        change(
            &db,
            "task.purge",
            json!({"id":"tsk-abcdef-1","revision":task["revision"]})
        )
        .await
        .unwrap_err()
        .code,
        -32010
    );
    status(&db, "task", "tsk-abcdef-1", "canceled").await;
    change(
        &db,
        "project.create",
        json!({"record":{"id":"pj-abcdef-2","name":"Other"}}),
    )
    .await
    .unwrap();
    change(&db,"task.create",json!({"parent":"pj-abcdef-2","record":{"id":"tsk-abcdef-2","objective":"Continue","criteria":["Ready"]}})).await.unwrap();
    change(&db,"ticket.create",json!({"record":{"id":"tkt-abcdef-2","worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","task_id":"tsk-abcdef-2","objective":"Depend","depends":["tkt-abcdef-1"],"jobs":[{"id":"job-abcdef-2","objective":"Continue","criteria":["Ready"]}]}})).await.unwrap();
    let task = get(&db, "task", "tsk-abcdef-1").await;
    let count = history::Entity::find().all(&db.db).await.unwrap().len();
    assert_eq!(
        change(
            &db,
            "task.purge",
            json!({"id":"tsk-abcdef-1","revision":task["revision"]})
        )
        .await
        .unwrap_err()
        .code,
        -32010
    );
    assert_eq!(
        history::Entity::find().all(&db.db).await.unwrap().len(),
        count
    );
    assert_eq!(get(&db, "task", "tsk-abcdef-1").await, task);
    let external = get(&db, "ticket", "tkt-abcdef-2").await;
    change(
        &db,
        "ticket.update",
        json!({"id":"tkt-abcdef-2","revision":external["revision"],"patch":{"depends":[]}}),
    )
    .await
    .unwrap();
    assert_eq!(
        change(&db, "task.purge", json!({"id":"tsk-abcdef-1","revision":1}))
            .await
            .unwrap_err()
            .code,
        -32009
    );
    change(
        &db,
        "task.purge",
        json!({"id":"tsk-abcdef-1","revision":task["revision"]}),
    )
    .await
    .unwrap();
    let snapshot = db
        .call("project.snapshot", json!({"id":"pj-abcdef-2"}))
        .await
        .unwrap();
    assert_eq!(
        snapshot["dependencies"][0]["record"]["objective"],
        "Prepare example"
    );
    assert_eq!(snapshot["dependencies"][0]["physical"], true);
    assert_eq!(snapshot["dependencies"][0]["deleted"], true);
    assert_eq!(
        get(&db, "ticket", "tkt-abcdef-2").await["record"]["depends"],
        Value::Null
    );
}
