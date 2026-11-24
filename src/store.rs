use crate::{
    PROTOCOL, Result, conflict,
    engine::{Engine, history_summary, strings, text, view},
    entity::{history, meta, operation, record},
    invalid, rule,
};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, ConnectOptions, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Schema, Statement, TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub struct Store {
    pub db: DatabaseConnection,
    changes: tokio::sync::broadcast::Sender<i64>,
}
impl Store {
    pub async fn connect(url: &str, initialize: bool) -> Result<Self> {
        if !url.starts_with("sqlite:")
            && !url.starts_with("postgres://")
            && !url.starts_with("postgresql://")
        {
            return Err(invalid("Use a SQLite or PostgreSQL database URL"));
        }
        let mut options = ConnectOptions::new(url);
        options.max_connections(1).sqlx_logging(false);
        let db = Database::connect(options).await?;
        if initialize {
            let schema = Schema::new(db.get_database_backend());
            let backend = db.get_database_backend();
            for mut statement in [
                schema.create_table_from_entity(meta::Entity),
                schema.create_table_from_entity(record::Entity),
                schema.create_table_from_entity(history::Entity),
                schema.create_table_from_entity(operation::Entity),
            ] {
                statement.if_not_exists();
                db.execute(backend.build(&statement)).await?;
            }
            meta::Entity::insert(meta::ActiveModel {
                id: Set(1),
                protocol: Set(PROTOCOL.into()),
            })
            .on_conflict(OnConflict::column(meta::Column::Id).do_nothing().to_owned())
            .exec_without_returning(&db)
            .await?;
        }
        let protocol = meta::Entity::find_by_id(1)
            .one(&db)
            .await?
            .ok_or_else(|| rule("Initialize the database with ttjcore init"))?;
        if protocol.protocol != PROTOCOL {
            return Err(rule("Unsupported database protocol"));
        }
        let (changes, _) = tokio::sync::broadcast::channel(64);
        Ok(Self { db, changes })
    }
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        if !params.is_object() {
            return Err(invalid("params must be an object"));
        }
        if method == "events.list" || method == "events.wait" {
            return self.events(method, &params).await;
        }
        if method == "rpc.discover" {
            return Ok(discover());
        }
        if method.ends_with(".get")
            || method.ends_with(".list")
            || method == "project.snapshot"
            || method == "document.export"
        {
            let isolation = if self.db.get_database_backend() == sea_orm::DbBackend::Postgres {
                Some(sea_orm::IsolationLevel::RepeatableRead)
            } else {
                None
            };
            let tx = self.db.begin_with_config(isolation, None).await?;
            let rows = record::Entity::find()
                .order_by_asc(record::Column::Id)
                .all(&tx)
                .await?;
            let events = history::Entity::find()
                .order_by_asc(history::Column::Id)
                .all(&tx)
                .await?;
            let cursor = events.last().map_or(0, |e| e.id);
            let mut result = read(method, &params, rows, events)?;
            if method == "project.snapshot" || method.ends_with(".list") && result.is_object() {
                result["cursor"] = json!(cursor);
            }
            tx.commit().await?;
            return Ok(result);
        }
        if !is_mutation(method) {
            return Err(crate::Error {
                code: -32601,
                message: "Method not found; call rpc.discover".into(),
            });
        }
        let actor = strings(&params, "actor", true)?;
        let reason = if method == "task.purge" && params.get("reason").is_none() {
            "Taskを削除".to_owned()
        } else {
            text(&params, "reason")?
        };
        let op = match params.get("operation") {
            Some(_) => text(&params, "operation")?,
            None => uuid::Uuid::new_v4().to_string(),
        };
        let tx = self.db.begin().await?;
        // Taking this write lock first serializes graph edits on both backends.
        meta::Entity::update_many()
            .col_expr(
                meta::Column::Protocol,
                Expr::col(meta::Column::Protocol).into(),
            )
            .filter(meta::Column::Id.eq(1))
            .exec(&tx)
            .await?;
        let request = json!({"method":method,"params":params});
        if let Some(existing) = operation::Entity::find_by_id(&op).one(&tx).await? {
            if existing.request != request {
                return Err(conflict());
            }
            tx.rollback().await?;
            return Ok(self.reclaim_task_space(existing.response).await);
        }
        let rows = record::Entity::find().all(&tx).await?;
        let reserved_ids = history::Entity::find()
            .select_only()
            .column(history::Column::Target)
            .distinct()
            .into_tuple::<String>()
            .all(&tx)
            .await?
            .into_iter()
            .collect();
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|_| rule("Clock formatting failed"))?;
        let mut engine = Engine {
            rows: rows.into_iter().map(|r| (r.id.clone(), r)).collect(),
            reserved_ids,
            changes: vec![],
            actor: actor.clone(),
            reason: reason.clone(),
            now: now.clone(),
        };
        let value = engine.apply(method, &params)?;
        let purged: BTreeSet<_> = engine
            .changes
            .iter()
            .filter(|change| change.row.flags["physical"] == true)
            .map(|change| change.row.id.clone())
            .collect();
        if !purged.is_empty() {
            let kinds: BTreeMap<_, _> = engine
                .changes
                .iter()
                .map(|change| (change.row.id.clone(), change.row.kind.clone()))
                .collect();
            let names: BTreeMap<_, _> = engine
                .changes
                .iter()
                .filter_map(|change| {
                    change
                        .row
                        .body
                        .get("objective")
                        .cloned()
                        .map(|name| (change.row.id.clone(), name))
                })
                .collect();
            for event in history::Entity::find()
                .filter(history::Column::Target.is_in(purged.iter().cloned()))
                .all(&tx)
                .await?
            {
                let summarize = |value| {
                    let mut summary = history_summary(value);
                    summary["kind"] = json!(kinds.get(&event.target));
                    if summary["record"].get("objective").is_none()
                        && let Some(name) = names.get(&event.target)
                    {
                        summary["record"]["objective"] = name.clone();
                    }
                    summary
                };
                let before = event.before.clone().map(summarize);
                let after = event.after.clone().map(summarize);
                let mut model: history::ActiveModel = event.into();
                model.before = Set(before);
                model.after = Set(after);
                model.update(&tx).await?;
            }
            for operation in operation::Entity::find().all(&tx).await? {
                let response = &operation.response["value"];
                let affected = [
                    &operation.request["params"]["id"],
                    &operation.request["params"]["job"],
                    &response["record"]["id"],
                    &response["run"]["record"]["id"],
                    &response["job"]["record"]["id"],
                ]
                .iter()
                .filter_map(|value| value.as_str())
                .any(|id| purged.contains(id));
                if affected {
                    let mut model: operation::ActiveModel = operation.into();
                    model.request = Set(json!({"purged":true}));
                    model.response = Set(json!({"purged":true}));
                    model.update(&tx).await?;
                }
            }
        }
        let mut changed_cursor = None;
        for change in engine.changes {
            let row = change.row;
            let model = record::ActiveModel {
                id: Set(row.id.clone()),
                kind: Set(row.kind.clone()),
                parent: Set(row.parent.clone()),
                body: Set(row.body.clone()),
                revision: Set(row.revision),
                started: Set(row.started),
                deleted: Set(row.deleted),
                active: Set(row.active),
                flags: Set(row.flags.clone()),
            };
            if row.flags["physical"] == true {
                record::Entity::delete_by_id(row.id.clone())
                    .exec(&tx)
                    .await?;
            } else {
                record::Entity::insert(model)
                    .on_conflict(
                        OnConflict::column(record::Column::Id)
                            .update_columns([
                                record::Column::Kind,
                                record::Column::Parent,
                                record::Column::Body,
                                record::Column::Revision,
                                record::Column::Started,
                                record::Column::Deleted,
                                record::Column::Active,
                                record::Column::Flags,
                            ])
                            .to_owned(),
                    )
                    .exec(&tx)
                    .await?;
            }
            let event = history::ActiveModel {
                operation: Set(op.clone()),
                project: Set(change.project),
                target: Set(row.id.clone()),
                event: Set(change.event),
                actor: Set(json!(actor)),
                timestamp: Set(now.clone()),
                reason: Set(reason.clone()),
                revision: Set(row.revision),
                before: Set(change.before),
                after: Set(Some(if row.flags["physical"] == true {
                    history_summary(view(&row))
                } else {
                    view(&row)
                })),
                ..Default::default()
            }
            .insert(&tx)
            .await?;
            changed_cursor = Some(event.id);
        }
        let response = json!({"operation":op,"value":value});
        operation::ActiveModel {
            id: Set(op),
            request: Set(request),
            response: Set(response.clone()),
        }
        .insert(&tx)
        .await?;
        tx.commit().await?;
        if let Some(cursor) = changed_cursor {
            let _ = self.changes.send(cursor);
        }
        Ok(self.reclaim_task_space(response).await)
    }
    async fn reclaim_task_space(&self, mut response: Value) -> Value {
        if response["value"]["physical"] == true {
            response["value"]["storage"] =
                if self.db.get_database_backend() == sea_orm::DbBackend::Sqlite {
                    let vacuum = self.db.execute_unprepared("VACUUM").await.is_ok();
                    let checkpoint = if vacuum {
                        self.db
                            .query_one(Statement::from_string(
                                sea_orm::DbBackend::Sqlite,
                                "PRAGMA wal_checkpoint(TRUNCATE)".to_owned(),
                            ))
                            .await
                            .ok()
                            .flatten()
                            .and_then(|row| row.try_get::<i64>("", "busy").ok())
                            == Some(0)
                    } else {
                        false
                    };
                    json!(if vacuum && checkpoint {
                        "reclaimed"
                    } else {
                        "pending"
                    })
                } else {
                    json!("reusable")
                };
        }
        response
    }
}
impl Store {
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<i64> {
        self.changes.subscribe()
    }
    async fn events(&self, method: &str, p: &Value) -> Result<Value> {
        let mut receiver = self.subscribe();
        let after = p
            .get("cursor")
            .map(|v| {
                v.as_i64()
                    .filter(|n| *n >= 0)
                    .ok_or_else(|| invalid("cursor must be a non-negative integer"))
            })
            .transpose()?;
        let project = p.get("project").map(|_| text(p, "project")).transpose()?;
        let timeout = p
            .get("timeout_ms")
            .map(|v| {
                v.as_u64()
                    .filter(|n| *n > 0 && *n <= 25000)
                    .ok_or_else(|| invalid("timeout_ms must be between 1 and 25000"))
            })
            .transpose()?
            .unwrap_or(20000);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout);
        let mut cursor = after;
        loop {
            let rows = history::Entity::find()
                .order_by_asc(history::Column::Id)
                .all(&self.db)
                .await?;
            let latest = rows.last().map_or(0, |r| r.id);
            let after = cursor.unwrap_or(latest);
            if after > latest {
                return Err(invalid("cursor is ahead of this Core history"));
            }
            let events: Vec<_> = rows
                .iter()
                .filter(|r| r.id > after && project.as_ref().is_none_or(|id| r.project == *id))
                .map(|r| {
                    json!({"id":r.id,"project":r.project,"target":r.target,"event":r.event,
                    "operation":r.operation,"revision":r.revision,"timestamp":r.timestamp,
                    "status":r.after.as_ref().map(|a|a["record"]["status"].clone())})
                })
                .collect();
            let result = json!({"cursor":latest,"events":events});
            if method == "events.list" || !result["events"].as_array().unwrap().is_empty() {
                return Ok(result);
            }
            cursor = Some(latest);
            match tokio::time::timeout_at(deadline, receiver.recv()).await {
                Err(_) => return Ok(result),
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => return Ok(result),
                _ => {}
            }
        }
    }
}

fn is_mutation(method: &str) -> bool {
    if matches!(method, "task.purge" | "document.import") {
        return true;
    }
    if matches!(
        method,
        "status.set" | "run.start" | "run.finish" | "run.wait"
    ) {
        return true;
    }
    let Some((kind, action)) = method.split_once('.') else {
        return false;
    };
    matches!(
        kind,
        "project" | "task" | "ticket" | "job" | "knowledge" | "worker" | "session"
    ) && matches!(action, "create" | "update" | "delete")
}
fn read(
    method: &str,
    p: &Value,
    rows: Vec<record::Model>,
    events: Vec<history::Model>,
) -> Result<Value> {
    let engine = Engine {
        rows: rows
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect::<BTreeMap<_, _>>(),
        reserved_ids: BTreeSet::new(),
        changes: vec![],
        actor: vec![],
        reason: String::new(),
        now: String::new(),
    };
    if method == "document.export" {
        return Ok(engine.export(p)?.to_value());
    }
    if method == "timeline.list" {
        let project = text(p, "project")?;
        let row = if p["include_deleted"] == true {
            engine
                .rows
                .get(&project)
                .cloned()
                .ok_or_else(|| crate::Error {
                    code: -32004,
                    message: "Record not found".into(),
                })?
        } else {
            engine.get(&project)?
        };
        if row.kind != "project" {
            return Err(invalid("project must identify a Project"));
        }
        return Ok(timeline(&project, &events, &engine.rows));
    }
    if method == "project.snapshot" {
        let project = engine.get(&text(p, "id")?)?;
        if project.kind != "project" {
            return Err(invalid("id must identify a Project"));
        }
        let mut tasks = vec![];
        let mut dependency_ids = BTreeSet::new();
        for task in engine.rows.values().filter(|r| {
            r.kind == "task" && !r.deleted && r.parent.as_deref() == Some(project.id.as_str())
        }) {
            let mut tickets = vec![];
            for ticket in engine.rows.values().filter(|r| {
                r.kind == "ticket" && !r.deleted && r.parent.as_deref() == Some(task.id.as_str())
            }) {
                dependency_ids.extend(strings(&ticket.body, "depends", false)?);
                let mut job_rows: Vec<_> = engine
                    .rows
                    .values()
                    .filter(|r| r.kind == "job" && r.parent.as_deref() == Some(ticket.id.as_str()))
                    .collect();
                job_rows.sort_by_key(|r| r.flags["order"].as_u64().unwrap_or(0));
                let jobs: Vec<_> = job_rows
                    .into_iter()
                    .map(|r| engine.record_view(r))
                    .collect();
                tickets.push(json!({"ticket":engine.record_view(ticket),"jobs":jobs}));
            }
            tasks.push(json!({"task":engine.record_view(task),"tickets":tickets}));
        }
        let knowledge: Vec<_> = engine
            .rows
            .values()
            .filter(|r| {
                r.kind == "knowledge"
                    && !r.deleted
                    && r.parent.as_deref() == Some(project.id.as_str())
                    && r.body["status"] == "active"
            })
            .map(|r| engine.record_view(r))
            .collect();
        let timeline = timeline(&project.id, &events, &engine.rows);
        for event in timeline.as_array().unwrap() {
            for side in ["from", "to"] {
                if let Some(depends) = event[side]["record"]["depends"].as_array() {
                    dependency_ids
                        .extend(depends.iter().filter_map(Value::as_str).map(str::to_owned));
                }
            }
        }
        let dependencies: Vec<_> = dependency_ids
            .iter()
            .filter_map(|id| {
                if let Some(row) = engine.rows.get(id) {
                    return (row.kind == "ticket").then(|| engine.record_view(row));
                }
                let event = events.iter().rev().find(|event| event.target == *id)?;
                let mut summary = event.after.clone().or_else(|| event.before.clone())?;
                summary["deleted"] = json!(true);
                summary["physical"] = json!(true);
                summary["active"] = json!(false);
                Some(summary)
            })
            .collect();
        return Ok(
            json!({"project":engine.record_view(&project),"tasks":tasks,"timeline":timeline,"knowledge":knowledge,"dependencies":dependencies}),
        );
    }
    let (kind, action) = method
        .split_once('.')
        .ok_or_else(|| invalid("Unknown method"))?;
    if !matches!(
        kind,
        "project" | "task" | "ticket" | "job" | "run" | "knowledge" | "worker" | "session"
    ) {
        return Err(crate::Error {
            code: -32601,
            message: "Method not found".into(),
        });
    }
    let include_deleted = p["include_deleted"] == true;
    if action == "get" {
        let id = text(p, "id")?;
        let row = engine
            .rows
            .get(&id)
            .filter(|r| r.kind == kind && (include_deleted || !r.deleted))
            .ok_or_else(|| crate::Error {
                code: -32004,
                message: "Record not found".into(),
            })?;
        return Ok(engine.record_view(row));
    }
    if action == "list" {
        let parent = p.get("parent").map(|_| text(p, "parent")).transpose()?;
        let project = p.get("project").map(|_| text(p, "project")).transpose()?;
        let mut list = vec![];
        for row in engine
            .rows
            .values()
            .filter(|r| r.kind == kind && (include_deleted || !r.deleted))
        {
            if parent
                .as_ref()
                .is_some_and(|parent| row.parent.as_ref() != Some(parent))
            {
                continue;
            }
            if let Some(project) = &project
                && engine.project(row)? != *project
            {
                continue;
            }
            list.push(engine.record_view(row));
        }
        return Ok(json!({"items":list}));
    }
    Err(invalid("Unknown read operation"))
}
fn timeline(
    project: &str,
    events: &[history::Model],
    rows: &BTreeMap<String, record::Model>,
) -> Value {
    json!(
        events
            .iter()
            .filter(|e| e.project == project)
            .map(|e| json!({
                "id":e.id,"operation":e.operation,"target":e.target,"event":e.event,
                "kind":rows.get(&e.target).map(|row|row.kind.as_str())
                    .or_else(||e.after.as_ref().and_then(|value|value["kind"].as_str()))
                    .or_else(||e.before.as_ref().and_then(|value|value["kind"].as_str())),
                "actor":e.actor,"timestamp":e.timestamp,"reason":e.reason,"revision":e.revision,
                "from":e.before,"to":e.after
            }))
            .collect::<Vec<_>>()
    )
}
pub fn discover() -> Value {
    let mut methods = vec![];
    for kind in [
        "project",
        "task",
        "ticket",
        "job",
        "run",
        "knowledge",
        "worker",
        "session",
    ] {
        for action in ["get", "list"] {
            methods.push(json!({"name":format!("{kind}.{action}"),"mutates":false,
                "required":if action=="get" {vec!["id"]} else {vec![]}}));
        }
        if kind != "run" {
            for action in ["create", "update", "delete"] {
                if kind == "job" && action != "update" {
                    continue;
                }
                methods.push(json!({"name":format!("{kind}.{action}"),"mutates":true,
                    "required":if action=="create" {if matches!(kind,"task"|"knowledge") {vec!["actor","reason","parent","record"]} else {vec!["actor","reason","record"]}}
                        else if action=="update" {vec!["actor","reason","id","revision","patch"]}
                        else {vec!["actor","reason","id","revision"]}}));
            }
        }
    }
    methods.extend([
        json!({"name":"document.export","mutates":false,"required":[],"optional":["project","tickets"]}),
        json!({"name":"document.import","mutates":true,"required":["actor","reason","documents"]}),
        json!({"name":"run.wait","mutates":true,"required":["actor","reason","id","revision","wait"]}),
        json!({"name":"task.purge","mutates":true,"required":["actor","id","revision"],
            "description":"Physically remove a terminal Task and all descendants; retain summary history and Knowledge; reject external Ticket references"}),
        json!({"name":"core.stop","mutates":false,"required":[],"transport":"IPC service control"}),
        json!({"name":"events.list","mutates":false,"required":[]}),
        json!({"name":"events.wait","mutates":false,"required":[]}),
        json!({"name":"events.subscribe","mutates":false,"required":[],"transport":"persistent IPC or rpc stdio bridge"}),
        json!({"name":"project.snapshot","mutates":false,"required":["id"]}),
        json!({"name":"timeline.list","mutates":false,"required":["project"]}),
        json!({"name":"status.set","mutates":true,"required":["actor","reason","id","revision","status"]}),
        json!({"name":"run.start","mutates":true,"required":["actor","reason","job","revision"]}),
        json!({"name":"run.finish","mutates":true,"required":["actor","reason","id","revision","result"]}),
    ]);
    json!({"methods":methods,
        "record_fields":{
            "project":{"required":["name"]},
            "task":{"required":["objective","criteria"],"parent":"Project ID in params.parent","optional":["deadline","resume","handoff","status"]},
            "worker":{"required":["name"]},
            "session":{"required":["worker_id"],"optional":["started","ended"]},
            "ticket":{"required":["task_id","worker_id","session_id","objective"],"optional":["jobs","exclusions","depends","refs","artifacts","resume","status"],
                "jobs":"Array of complete Job objects; IDs and order are fixed atomically"},
            "job":{"required":["objective","criteria"],"optional":["instructions","status"]},
            "knowledge":{"required":["content","basis"],"parent":"Project ID in params.parent","optional":["supersedes"]}
        },
        "field_types":{
            "criteria":"Non-empty array of non-empty strings",
            "actor":"Management actor identifiers, or [worker_id, session_id] for Run execution",
            "depends":"Array of Ticket IDs",
            "handoff":"Single volatile Task Handoff object",
            "deadline":"Timezone-qualified RFC 3339 datetime",
            "id":"<pj|tsk|tkt|job|run|wkr|ssn>-<6 or 8 alphanumerics>-<positive increment without leading zeros>; generated when omitted. Knowledge IDs are separate.",
            "revision":"Integer returned by the latest read"
        },
        "editable_fields":{
            "project":["name"],"task":["objective","criteria","deadline","resume","handoff"],
            "ticket":["depends","refs","artifacts","resume"],
            "job":["result","artifacts","resume"],"knowledge":["status"],"worker":["name"],"session":["ended"]
        },
        "display_metadata":{"storage":"Outside TTJ record; returned as wrapper.display",
            "fields":{"summary":"Short string for cards","icon":"User-defined icon key"},
            "write":"Optional display object on create/update; inline Job definitions may include display"},
        "mutation_options":{
        "operation":"Caller-generated unique id for safe retries; generated when omitted",
        "revision":"Revision returned by the latest read; required for updates/deletes",
        "criteria_met":"Required true, with result evidence, for Task/Job completion",
        "release":"Required true to release an explicit pause",
        "resolved":"Required true to attest that waiting/blocking constraints are resolved",
        "safety":"Required explanation before retrying or following an unconfirmed stopped Run"
    }})
}
