use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait, EntityTrait};
use serde_json::{Value, json};
use ttjcore::{
    entity::{history, operation, record},
    store::Store,
};

async fn change(db: &Store, method: &str, mut params: Value) -> ttjcore::Result<Value> {
    params["actor"] = json!(["operator"]);
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
        "project.create",
        json!({"operation":"project-create","record":{"id":"project","name":"Example"}}),
    )
    .await
    .unwrap();
    let create = json!({"operation":"task-create","record":{"id":"task","project":"project","objective":"Publish example","criteria":["Published"]}});
    change(db, "task.create", create.clone()).await.unwrap();
    change(db,"ticket.create",json!({"operation":"ticket-create","record":{"id":"ticket","task":"task","objective":"Prepare example","jobs":[
        {"id":"job","objective":"Check example","criteria":["Checked"],"instructions":detail}]}})).await.unwrap();
    change(db,"knowledge.create",json!({"record":{"id":"knowledge","project":"project","content":"Durable project knowledge","basis":"Verified finding"}})).await.unwrap();
    create
}
async fn finish_work(db: &Store, detail: &str) {
    let job = get(db, "job", "job").await;
    change(
        db,
        "run.start",
        json!({"id":"run","job":"job","revision":job["revision"]}),
    )
    .await
    .unwrap();
    let run = get(db, "run", "run").await;
    change(
        db,
        "run.finish",
        json!({"id":"run","revision":run["revision"],"result":detail}),
    )
    .await
    .unwrap();
    status(db, "job", "job", "completed").await;
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
    status(&db, "task", "task", "completed").await;
    let task = get(&db, "task", "task").await;
    change(
        &db,
        "task.delete",
        json!({"id":"task","revision":task["revision"]}),
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
        .call("task.get", json!({"id":"task","include_deleted":true}))
        .await
        .unwrap();
    let params = json!({"operation":"purge-once","id":"task","revision":task["revision"]});
    let result = change(&db, "task.purge", params.clone()).await.unwrap();
    assert_eq!(result["value"]["records"], 4);
    assert_eq!(result["value"]["storage"], "reclaimed");
    for (kind, id) in [
        ("task", "task"),
        ("ticket", "ticket"),
        ("job", "job"),
        ("run", "run"),
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
    assert_eq!(record::Entity::find().all(&db.db).await.unwrap().len(), 2);
    let timeline = db
        .call("timeline.list", json!({"project":"project"}))
        .await
        .unwrap();
    for (target, kind) in [
        ("task", "task"),
        ("ticket", "ticket"),
        ("job", "job"),
        ("run", "run"),
        ("project", "project"),
        ("knowledge", "knowledge"),
    ] {
        let events: Vec<_> = timeline
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["target"] == target)
            .collect();
        assert!(events.iter().all(|event| event["kind"] == kind));
        if ["task", "ticket", "job", "run"].contains(&target) {
            assert!(events.iter().all(|event| event["to"]["kind"] == kind));
        }
    }
    assert!(!timeline.to_string().contains("ARCHIVED_DETAIL_"));
    assert!(
        timeline
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["target"] == "task"
                && event["event"] == "record.deleted"
                && event["reason"] == "Taskを削除")
    );
    assert!(
        timeline
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["target"] == "ticket"
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
            .any(|event| event["target"] == "run"
                && event["to"]["record"]["objective"] == "Check example")
    );
    let operations = operation::Entity::find().all(&db.db).await.unwrap();
    assert!(!format!("{operations:?}").contains("ARCHIVED_DETAIL_"));
    assert_eq!(
        change(&db, "task.create", create).await.unwrap_err().code,
        -32009
    );
    assert_eq!(change(&db,"task.create",json!({"record":{"id":"task","project":"project","objective":"Reuse","criteria":["No"]}})).await.unwrap_err().code,-32009);
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
    let task = get(&db, "task", "task").await;
    assert_eq!(
        change(
            &db,
            "task.purge",
            json!({"id":"task","revision":task["revision"]})
        )
        .await
        .unwrap_err()
        .code,
        -32010
    );
    finish_work(&db, "Run result").await;
    let task = get(&db, "task", "task").await;
    assert_eq!(task["record"]["status"], "doing");
    assert_eq!(
        change(
            &db,
            "task.purge",
            json!({"id":"task","revision":task["revision"]})
        )
        .await
        .unwrap_err()
        .code,
        -32010
    );
    status(&db, "task", "task", "canceled").await;
    change(
        &db,
        "project.create",
        json!({"record":{"id":"other-project","name":"Other"}}),
    )
    .await
    .unwrap();
    change(&db,"task.create",json!({"record":{"id":"other-task","project":"other-project","objective":"Continue","criteria":["Ready"]}})).await.unwrap();
    change(&db,"ticket.create",json!({"record":{"id":"other-ticket","task":"other-task","objective":"Depend","depends":["ticket"],"jobs":[{"id":"other-job","objective":"Continue","criteria":["Ready"]}]}})).await.unwrap();
    let task = get(&db, "task", "task").await;
    let count = history::Entity::find().all(&db.db).await.unwrap().len();
    assert_eq!(
        change(
            &db,
            "task.purge",
            json!({"id":"task","revision":task["revision"]})
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
    assert_eq!(get(&db, "task", "task").await, task);
    let external = get(&db, "ticket", "other-ticket").await;
    change(
        &db,
        "ticket.update",
        json!({"id":"other-ticket","revision":external["revision"],"patch":{"depends":[]}}),
    )
    .await
    .unwrap();
    assert_eq!(
        change(&db, "task.purge", json!({"id":"task","revision":1}))
            .await
            .unwrap_err()
            .code,
        -32009
    );
    change(
        &db,
        "task.purge",
        json!({"id":"task","revision":task["revision"]}),
    )
    .await
    .unwrap();
    let snapshot = db
        .call("project.snapshot", json!({"id":"other-project"}))
        .await
        .unwrap();
    assert_eq!(
        snapshot["dependencies"][0]["record"]["objective"],
        "Prepare example"
    );
    assert_eq!(snapshot["dependencies"][0]["physical"], true);
    assert_eq!(snapshot["dependencies"][0]["deleted"], true);
    assert_eq!(
        get(&db, "ticket", "other-ticket").await["record"]["depends"],
        json!([])
    );
}
