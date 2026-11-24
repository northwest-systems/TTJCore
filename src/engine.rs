use crate::document::{self, Document};
use crate::{Result, conflict, entity::record, invalid, rule};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn text(v: &Value, key: &str) -> Result<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("{key} must be a non-empty string")))
}
pub fn strings(v: &Value, key: &str, required: bool) -> Result<Vec<String>> {
    let Some(value) = v.get(key) else {
        return if required {
            Err(invalid(format!("{key} is required")))
        } else {
            Ok(vec![])
        };
    };
    let array = value
        .as_array()
        .ok_or_else(|| invalid(format!("{key} must be an array")))?;
    let mut out = vec![];
    for value in array {
        let s = value
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| invalid(format!("{key} must contain non-empty strings")))?;
        if out.iter().any(|a| a == s) {
            return Err(invalid(format!("{key} contains duplicates")));
        }
        out.push(s.to_owned());
    }
    if required && out.is_empty() {
        return Err(invalid(format!("{key} must not be empty")));
    }
    Ok(out)
}
pub fn terminal(body: &Value) -> bool {
    matches!(body["status"].as_str(), Some("completed" | "canceled"))
}
pub fn view(row: &record::Model) -> Value {
    json!({"kind":row.kind,"record":row.body,"revision":row.revision,"deleted":row.deleted,
        "started":row.started,"active":row.active,"constraints":row.flags,
        "display":row.flags.get("display").cloned().unwrap_or(json!({}))})
}
pub fn history_summary(value: Value) -> Value {
    let mut body = serde_json::Map::new();
    for key in ["id", "name", "objective", "status", "started", "ended"] {
        if let Some(field) = value["record"].get(key) {
            body.insert(key.into(), field.clone());
        }
    }
    json!({"kind":value["kind"],"record":body,"revision":value["revision"],"deleted":value["deleted"],
        "summary":true,"physical":value["constraints"]["physical"] == true})
}
pub struct Change {
    pub row: record::Model,
    pub before: Option<Value>,
    pub event: String,
    pub project: String,
}
pub struct Engine {
    pub rows: BTreeMap<String, record::Model>,
    pub reserved_ids: BTreeSet<String>,
    pub changes: Vec<Change>,
    pub actor: Vec<String>,
    pub management_actors: BTreeSet<String>,
    pub reason: String,
    pub now: String,
}
impl Engine {
    pub fn get(&self, id: &str) -> Result<record::Model> {
        self.rows
            .get(id)
            .filter(|r| !r.deleted)
            .cloned()
            .ok_or_else(|| crate::Error {
                code: -32004,
                message: "Record not found".into(),
            })
    }
    fn raw(&self, id: &str) -> Result<record::Model> {
        self.rows
            .get(id)
            .cloned()
            .ok_or_else(|| rule("Referenced record does not exist"))
    }
    fn expect(&self, p: &Value) -> Result<record::Model> {
        let row = self.get(&text(p, "id")?)?;
        if p["revision"].as_i64() != Some(row.revision) {
            return Err(conflict());
        }
        Ok(row)
    }
    pub fn project(&self, row: &record::Model) -> Result<String> {
        if matches!(row.kind.as_str(), "worker" | "session") {
            return Ok(String::new());
        }
        let mut current = row.clone();
        for _ in 0..6 {
            if current.kind == "project" {
                return Ok(current.id);
            }
            current = self.raw(
                current
                    .parent
                    .as_deref()
                    .ok_or_else(|| rule("Missing parent"))?,
            )?;
        }
        Err(rule("Invalid hierarchy"))
    }
    fn save(&mut self, mut row: record::Model, event: &str) -> Result<()> {
        let before = self.rows.get(&row.id).map(view);
        if let Some(previous) = self.rows.get(&row.id)
            && previous.kind == row.kind
            && previous.parent == row.parent
            && previous.body == row.body
            && previous.started == row.started
            && previous.deleted == row.deleted
            && previous.active == row.active
            && previous.flags == row.flags
        {
            return Ok(());
        }
        let event = if event == "status.changed"
            && before
                .as_ref()
                .is_some_and(|value| value["record"]["status"] == row.body["status"])
        {
            "record.updated"
        } else {
            event
        };
        row.revision = self.rows.get(&row.id).map_or(1, |r| r.revision + 1);
        self.rows.insert(row.id.clone(), row.clone());
        self.changes.push(Change {
            project: self.project(&row)?,
            row,
            before,
            event: event.into(),
        });
        Ok(())
    }
    fn insert(
        &mut self,
        kind: &str,
        parent: Option<String>,
        body: Value,
        display: Value,
    ) -> Result<String> {
        let id = text(&body, "id")?;
        if self.rows.contains_key(&id) {
            return Err(conflict());
        }
        self.save(
            record::Model {
                id: id.clone(),
                kind: kind.into(),
                parent,
                body,
                revision: 0,
                started: false,
                deleted: false,
                active: false,
                flags: json!({"display":display}),
            },
            "record.created",
        )?;
        Ok(id)
    }
    fn parent(&self, row: &record::Model) -> Result<record::Model> {
        self.get(
            row.parent
                .as_deref()
                .ok_or_else(|| rule("Missing parent"))?,
        )
    }
    fn allowed(body: &Value, allowed: &[&str]) -> Result<()> {
        let object = body
            .as_object()
            .ok_or_else(|| invalid("record/patch must be an object"))?;
        if let Some(key) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(invalid(format!(
                "Field {key} is not editable in this operation"
            )));
        }
        Ok(())
    }
    fn validate(kind: &str, body: &Value) -> Result<()> {
        document::check_record(kind, body)
    }
    pub fn record_body(&self, row: &record::Model) -> Value {
        let mut body = row.body.clone();
        let collections: &[(&str, &str)] = match row.kind.as_str() {
            "project" => &[("knowledge", "knowledge"), ("tasks", "task")],
            "ticket" => &[("jobs", "job")],
            "job" => &[("runs", "run")],
            _ => &[],
        };
        for (field, kind) in collections {
            let mut children = self.children(&row.id);
            children.retain(|r| r.kind == *kind && (!r.deleted || row.deleted));
            children.sort_by_key(|r| r.flags["order"].as_u64().unwrap_or(0));
            let items: Vec<_> = children.iter().map(|r| self.record_body(r)).collect();
            if !items.is_empty() {
                body[*field] = json!(items);
            }
        }
        omit_empty_arrays(&mut body);
        body
    }
    pub fn record_view(&self, row: &record::Model) -> Value {
        let mut value = view(row);
        value["record"] = self.record_body(row);
        value
    }
    fn children(&self, id: &str) -> Vec<record::Model> {
        self.rows
            .values()
            .filter(|r| r.parent.as_deref() == Some(id))
            .cloned()
            .collect()
    }
    fn descendants(&self, id: &str) -> Vec<record::Model> {
        let mut result = vec![];
        let mut queue = vec![id.to_owned()];
        while let Some(parent) = queue.pop() {
            for child in self.children(&parent) {
                queue.push(child.id.clone());
                result.push(child);
            }
        }
        result
    }
    fn jobs(&self, ticket: &record::Model) -> Result<Vec<record::Model>> {
        let mut jobs = self.children(&ticket.id);
        jobs.retain(|r| r.kind == "job");
        jobs.sort_by_key(|r| r.flags["order"].as_u64().unwrap_or(0));
        Ok(jobs)
    }
    fn current(&self, ticket: &record::Model) -> Result<Option<record::Model>> {
        Ok(self.jobs(ticket)?.into_iter().find(|r| !terminal(&r.body)))
    }
    fn dependencies(&self, ticket: &record::Model) -> Result<(bool, bool)> {
        let mut ready = true;
        let mut blocked = false;
        for id in self.edges(ticket)? {
            let dependency = self.raw(&id)?;
            if dependency.kind != "ticket" {
                return Err(rule("depends must reference Tickets"));
            }
            ready &= dependency.body["status"] == "completed";
            blocked |= dependency.body["status"] == "canceled";
        }
        Ok((ready, blocked))
    }
    fn edges(&self, ticket: &record::Model) -> Result<Vec<String>> {
        if terminal(&ticket.body) {
            return Ok(vec![]);
        }
        let mut edges = strings(&ticket.body, "depends", false)?;
        for job in self.jobs(ticket)?.iter().filter(|j| !terminal(&j.body)) {
            for run in self.children(&job.id) {
                let wait = &run.body["wait"];
                if wait["resolved"].is_null()
                    && let Some(target) = wait["target"]
                        .as_str()
                        .filter(|id| document::is_id_of("ticket", id))
                {
                    edges.push(target.to_owned());
                }
            }
        }
        Ok(edges)
    }
    fn check_cycles(&self) -> Result<()> {
        for ticket in self
            .rows
            .values()
            .filter(|r| r.kind == "ticket" && !r.deleted)
        {
            let mut seen = BTreeSet::new();
            let mut queue = self.edges(ticket)?;
            while let Some(id) = queue.pop() {
                if id == ticket.id {
                    return Err(rule("Ticket dependencies must be acyclic"));
                }
                if !seen.insert(id.clone()) {
                    continue;
                }
                let row = self.raw(&id)?;
                if row.kind != "ticket" {
                    return Err(rule("depends/wait.target must reference Tickets"));
                }
                queue.extend(self.edges(&row)?);
            }
        }
        Ok(())
    }
    fn assign_id(
        &self,
        kind: &str,
        body: &mut Value,
        reserved: &mut BTreeSet<String>,
    ) -> Result<()> {
        self.assign_id_with(kind, body, reserved, || uuid::Uuid::new_v4().as_u128())
    }
    fn assign_id_with(
        &self,
        kind: &str,
        body: &mut Value,
        reserved: &mut BTreeSet<String>,
        mut random: impl FnMut() -> u128,
    ) -> Result<()> {
        if !body.is_object() {
            return Err(invalid("record must be an object"));
        }
        if body.get("id").is_none() {
            let prefix = document::prefix(kind);
            if let Some(prefix) = prefix {
                let all: Vec<_> = self
                    .rows
                    .keys()
                    .chain(self.reserved_ids.iter())
                    .chain(reserved.iter())
                    .filter(|id| document::is_id_of(kind, id))
                    .collect();
                let head = all
                    .first()
                    .map(|id| id.rsplit_once('-').unwrap().0.to_owned())
                    .unwrap_or_else(|| {
                        let mut number = random();
                        let mut series = String::new();
                        for _ in 0..8 {
                            series.push(
                                b"0123456789abcdefghijklmnopqrstuvwxyz"[(number % 36) as usize]
                                    as char,
                            );
                            number /= 36;
                        }
                        format!("{prefix}-{series}")
                    });
                let previous = all
                    .iter()
                    .filter_map(|id| id.rsplit_once('-'))
                    .filter(|(series, _)| *series == head)
                    .map(|(_, n)| n)
                    .max_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)))
                    .unwrap_or("0");
                let number = increment(previous);
                body["id"] = json!(format!("{head}-{number}"));
            } else {
                loop {
                    let id = format!("{:032x}", random());
                    if !self.rows.contains_key(&id)
                        && !self.reserved_ids.contains(&id)
                        && !reserved.contains(&id)
                    {
                        body["id"] = json!(id);
                        break;
                    }
                }
            }
        }
        let id = text(body, "id")?;
        if document::prefix(kind).is_some() && !document::is_id_of(kind, &id) {
            return Err(invalid("Invalid TTJ 1.1.0 ID for record kind"));
        }
        if self.rows.contains_key(&id) || self.reserved_ids.contains(&id) || !reserved.insert(id) {
            return Err(conflict());
        }
        Ok(())
    }
    fn display(value: Option<&Value>) -> Result<Value> {
        let value = value.cloned().unwrap_or(json!({}));
        Self::allowed(&value, &["summary", "icon"])?;
        for (key, value) in value.as_object().unwrap() {
            if !value.is_string() || (key == "icon" && value.as_str().unwrap().trim().is_empty()) {
                return Err(invalid(
                    "display values must be strings; icon must be non-empty",
                ));
            }
        }
        Ok(value)
    }
    pub fn create(&mut self, kind: &str, p: &Value) -> Result<Value> {
        let mut body = p
            .get("record")
            .cloned()
            .ok_or_else(|| invalid("record is required"))?;
        let mut reserved = BTreeSet::new();
        if kind == "ticket"
            && let Some(jobs) = body.get("jobs").and_then(Value::as_array)
        {
            for job in jobs.iter().filter(|job| job.get("id").is_some()) {
                if !reserved.insert(text(job, "id")?) {
                    return Err(conflict());
                }
            }
        }
        self.assign_id(kind, &mut body, &mut reserved)?;
        let display = Self::display(p.get("display"))?;
        let mut jobs = vec![];
        let mut job_displays = vec![];
        let parent = match kind {
            "worker" => {
                Self::allowed(&body, &["id", "name"])?;
                None
            }
            "session" => {
                Self::allowed(&body, &["id", "worker_id", "started", "ended"])?;
                if self.get(&text(&body, "worker_id")?)?.kind != "worker" {
                    return Err(rule("Session needs a Worker"));
                }
                None
            }
            "project" => {
                Self::allowed(&body, &["id", "name"])?;
                None
            }
            "task" | "knowledge" => {
                let parent = self.get(&text(p, "parent")?)?;
                if parent.kind != "project" {
                    return Err(rule("Task/Knowledge needs a Project parent"));
                }
                if kind == "task" {
                    Self::allowed(
                        &body,
                        &[
                            "id",
                            "objective",
                            "criteria",
                            "deadline",
                            "resume",
                            "handoff",
                            "status",
                        ],
                    )?;
                    initial_status(&mut body)?;
                } else {
                    Self::allowed(&body, &["id", "content", "basis", "supersedes"])?;
                    if let Some(id) = body.get("supersedes") {
                        let mut old = self.get(
                            id.as_str()
                                .ok_or_else(|| invalid("supersedes must be an id"))?,
                        )?;
                        if old.kind != "knowledge"
                            || old.parent != Some(parent.id.clone())
                            || old.body["status"] != "active"
                        {
                            return Err(rule(
                                "supersedes must reference active Knowledge in the same Project",
                            ));
                        }
                        old.body["status"] = json!("supersed");
                        self.save(old, "knowledge.superseded")?;
                    }
                    body["status"] = json!("active");
                }
                Some(parent.id)
            }
            "ticket" => {
                Self::allowed(
                    &body,
                    &[
                        "id",
                        "task_id",
                        "worker_id",
                        "session_id",
                        "objective",
                        "exclusions",
                        "jobs",
                        "depends",
                        "refs",
                        "artifacts",
                        "resume",
                        "status",
                    ],
                )?;
                let parent = self.get(&text(&body, "task_id")?)?;
                if parent.kind != "task" || terminal(&parent.body) {
                    return Err(rule("Ticket needs a non-terminal Task"));
                }
                self.check_session(&body)?;
                jobs = match body.get("jobs") {
                    None => vec![],
                    Some(value) => value
                        .as_array()
                        .cloned()
                        .ok_or_else(|| invalid("jobs must be an array of Job objects"))?,
                };
                for job in &mut jobs {
                    if job.get("id").is_some() {
                        reserved.remove(&text(job, "id")?);
                    }
                    self.assign_id("job", job, &mut reserved)?;
                    let display = job.as_object_mut().unwrap().remove("display");
                    job_displays.push(Self::display(display.as_ref())?);
                    Self::allowed(
                        job,
                        &["id", "objective", "criteria", "instructions", "status"],
                    )?;
                    initial_status(job)?;
                    Self::validate("job", job)?;
                }
                body["jobs"] = json!(jobs);
                initial_status(&mut body)?;
                Some(parent.id)
            }
            _ => return Err(rule("Jobs use ticket.create; Runs use run.start")),
        };
        Self::validate(kind, &body)?;
        if kind == "ticket" {
            body.as_object_mut().unwrap().remove("jobs");
        }
        let id = self.insert(kind, parent, body, display)?;
        for (order, (job, display)) in jobs.into_iter().zip(job_displays).enumerate() {
            let job_id = self.insert("job", Some(id.clone()), job, display)?;
            let mut row = self.get(&job_id)?;
            row.flags["order"] = json!(order);
            self.save(row, "record.updated")?;
        }
        self.check_cycles()?;
        self.reconcile()?;
        Ok(self.record_view(&self.get(&id)?))
    }
    fn check_session(&self, body: &Value) -> Result<()> {
        let worker = self.get(&text(body, "worker_id")?)?;
        let session = self.get(&text(body, "session_id")?)?;
        if worker.kind != "worker"
            || session.kind != "session"
            || session.body["worker_id"] != worker.id
        {
            return Err(rule("Ticket requires a Session belonging to its Worker"));
        }
        Ok(())
    }
    fn authorize(&self, ticket: &record::Model) -> Result<()> {
        let expected = vec![
            text(&ticket.body, "worker_id")?,
            text(&ticket.body, "session_id")?,
        ];
        if self.actor != expected {
            return Err(rule(
                "Execution requires the Ticket's worker_id and session_id as actor",
            ));
        }
        self.check_session(&ticket.body)?;
        if self
            .get(&text(&ticket.body, "session_id")?)?
            .body
            .get("ended")
            .is_some_and(|value| !value.is_null())
        {
            return Err(rule("The Ticket Session has ended"));
        }
        Ok(())
    }
    fn authorize_management(&self, p: &Value) -> Result<()> {
        if p["mode"] != "management"
            || p.get("run").is_some()
            || self.actor.len() != 1
            || !self.management_actors.contains(&self.actor[0])
        {
            return Err(rule(
                "Management requires mode=management and a configured authorized principal; execution Run identity is not accepted",
            ));
        }
        Ok(())
    }
    fn authorize_change(&self, p: &Value, target: &record::Model) -> Result<()> {
        match p.get("mode").and_then(Value::as_str) {
            Some("management") => self.authorize_management(p),
            None | Some("execution") => self.authorize_run(
                p.get("run")
                    .ok_or_else(|| rule("Execution changes require a current Run ID"))?,
                target,
            ),
            _ => Err(invalid("mode must be execution or management")),
        }
    }
    pub fn update(&mut self, kind: &str, p: &Value) -> Result<Value> {
        let mut row = self.expect(p)?;
        if row.kind != kind {
            return Err(invalid("Record kind does not match method"));
        }
        let patch = p.get("patch").ok_or_else(|| invalid("patch is required"))?;
        let allowed: &[&str] = match kind {
            "project" => &["name"],
            "task" => &["objective", "criteria", "deadline", "resume", "handoff"],
            "ticket" => &["depends", "refs", "artifacts", "resume"],
            "job" => &["result", "artifacts", "resume"],
            "worker" => &["name"],
            "session" => &["ended"],
            "knowledge" => &["status"],
            _ => return Err(rule("Runs are updated through run.finish")),
        };
        Self::allowed(patch, allowed)?;
        if kind == "session" {
            self.authorize_management(p)?;
            if row.body["ended"].is_string()
                && patch
                    .get("ended")
                    .is_some_and(|ended| *ended != row.body["ended"])
            {
                return Err(rule(
                    "An ended Session cannot be reopened or have its end changed",
                ));
            }
        }
        if terminal(&row.body) {
            return Err(rule("Terminal records cannot be changed"));
        }
        if kind == "ticket" && patch.get("depends").is_some() && row.started {
            return Err(rule("Dependencies cannot change after execution starts"));
        }
        if kind == "knowledge"
            && patch.get("status").is_some()
            && !matches!(patch["status"].as_str(), Some("active" | "supersed"))
        {
            return Err(invalid("Knowledge status must be active or supersed"));
        }
        if p.get("display").is_some() {
            let display = Self::display(p.get("display"))?;
            if !row.flags["display"].is_object() {
                row.flags["display"] = json!({});
            }
            for (key, value) in display.as_object().unwrap() {
                row.flags["display"][key] = value.clone();
            }
        }
        if (kind == "task"
            && ["objective", "criteria", "deadline"]
                .iter()
                .any(|field| patch.get(field).is_some()))
            || (kind == "ticket" && patch.get("depends").is_some())
        {
            self.authorize_management(p)?;
        } else if matches!(kind, "task" | "ticket" | "job") {
            self.authorize_change(p, &row)?;
        }
        for (key, value) in patch.as_object().unwrap() {
            if value.is_null()
                && matches!(
                    key.as_str(),
                    "deadline" | "resume" | "handoff" | "ended" | "result"
                )
            {
                row.body.as_object_mut().unwrap().remove(key);
            } else {
                row.body[key] = value.clone();
            }
        }
        Self::validate(kind, &row.body)?;
        let id = row.id.clone();
        self.save(row, "record.updated")?;
        self.check_cycles()?;
        self.reconcile()?;
        Ok(self.record_view(&self.get(&id)?))
    }
    fn revoke(&mut self, mut run: record::Model) -> Result<()> {
        run.active = false;
        run.flags["stop_requested"] = json!(true);
        run.body["result"] = json!("canceled");
        self.save(run, "execution.stop_requested")
    }
    pub fn delete(&mut self, kind: &str, p: &Value) -> Result<Value> {
        let row = self.expect(p)?;
        if row.kind != kind {
            return Err(invalid("Record kind does not match method"));
        }
        if kind == "job" {
            return Err(rule(
                "Job membership is fixed; delete its terminal Ticket instead",
            ));
        }
        if kind == "run" {
            return Err(rule("Run history cannot be individually deleted"));
        }
        let mut targets = self.descendants(&row.id);
        targets.push(row.clone());
        if targets
            .iter()
            .any(|r| matches!(r.kind.as_str(), "task" | "ticket" | "job") && !terminal(&r.body))
        {
            return Err(rule("Cancel or complete all affected work before deleting"));
        }
        if kind == "knowledge" && row.body["status"] == "active" {
            return Err(rule("Supersede Knowledge before deleting it"));
        }
        if matches!(kind, "worker" | "session")
            && self.rows.values().any(|other| {
                other.body["worker_id"] == row.id || other.body["session_id"] == row.id
            })
        {
            return Err(rule("Worker/Session is still referenced"));
        }
        for mut target in targets {
            if target.deleted {
                continue;
            }
            target.deleted = true;
            self.save(target, "record.deleted")?;
        }
        Ok(view(&self.raw(&row.id)?))
    }
    pub fn purge_task(&mut self, p: &Value) -> Result<Value> {
        let row = self
            .rows
            .get(&text(p, "id")?)
            .cloned()
            .ok_or_else(|| crate::Error {
                code: -32004,
                message: "Record not found".into(),
            })?;
        if p["revision"].as_i64() != Some(row.revision) {
            return Err(conflict());
        }
        if row.kind != "task" {
            return Err(invalid("id must identify a Task"));
        }
        let mut targets = self.descendants(&row.id);
        targets.push(row.clone());
        if targets.iter().any(|target| {
            matches!(target.kind.as_str(), "task" | "ticket" | "job") && !terminal(&target.body)
                || target.active
        }) {
            return Err(rule(
                "Complete or cancel the Task and all its work before deleting",
            ));
        }
        let ids: BTreeSet<_> = targets.iter().map(|target| target.id.clone()).collect();
        for other in self
            .rows
            .values()
            .filter(|other| other.kind == "ticket" && !ids.contains(&other.id))
        {
            if strings(&other.body, "depends", false)?
                .iter()
                .any(|id| ids.contains(id))
                || other.body["refs"].as_array().is_some_and(|refs| {
                    refs.iter()
                        .any(|r| r["ticket_id"].as_str().is_some_and(|id| ids.contains(id)))
                })
                || self.descendants(&other.id).iter().any(|r| {
                    r.body["wait"]["target"]
                        .as_str()
                        .is_some_and(|id| ids.contains(id))
                })
            {
                return Err(rule(
                    "Another Task has a Ticket that references this Task's Tickets",
                ));
            }
        }
        let project = self.project(&row)?;
        let names: BTreeMap<_, _> = targets
            .iter()
            .filter_map(|target| {
                target
                    .body
                    .get("objective")
                    .cloned()
                    .map(|name| (target.id.clone(), name))
            })
            .collect();
        for mut target in targets {
            let mut before = history_summary(view(&target));
            if target.kind == "run"
                && let Some(name) = target.parent.as_ref().and_then(|id| names.get(id))
            {
                before["record"]["objective"] = name.clone();
            }
            target.revision += 1;
            target.deleted = true;
            target.active = false;
            target.flags = json!({"physical":true});
            target.body = before["record"].clone();
            self.rows.remove(&target.id);
            self.changes.push(Change {
                row: target,
                before: Some(before),
                event: "record.deleted".into(),
                project: project.clone(),
            });
        }
        Ok(json!({"id":row.id,"deleted":true,"physical":true,"records":ids.len()}))
    }
    pub fn set_status(&mut self, p: &Value) -> Result<Value> {
        let row = self.expect(p)?;
        self.authorize_change(p, &row)?;
        self.set_status_inner(p)
    }
    fn set_status_inner(&mut self, p: &Value) -> Result<Value> {
        let mut row = self.expect(p)?;
        if !matches!(row.kind.as_str(), "task" | "ticket" | "job") {
            return Err(rule("This record has no work status"));
        }
        let target = text(p, "status")?;
        let source = text(&row.body, "status")?;
        if source == target {
            return Ok(self.record_view(&row));
        }
        if terminal(&row.body) {
            return Err(rule("Terminal records cannot be reopened"));
        }
        let paths: &[&str] = match source.as_str() {
            "todo" => &["doing", "waiting", "paused", "blocked", "canceled"],
            "doing" => &["waiting", "paused", "blocked", "completed", "canceled"],
            "waiting" => &[
                "todo",
                "doing",
                "paused",
                "blocked",
                "completed",
                "canceled",
            ],
            "paused" => &["todo", "doing", "waiting", "blocked", "canceled"],
            "blocked" => &["todo", "doing", "waiting", "paused", "canceled"],
            _ => &[],
        };
        let ticket_aggregate = row.kind == "ticket"
            && target == "completed"
            && self.jobs(&row)?.iter().all(|j| terminal(&j.body));
        if !paths.contains(&target.as_str()) && !ticket_aggregate {
            return Err(rule("Transition is not permitted"));
        }
        if source == "paused" && target != "canceled" && p["release"] != true {
            return Err(rule("Leaving paused requires release=true"));
        }
        if p["resolved"] == true {
            row.flags["blocked"] = json!(false);
            if target != "waiting" {
                row.flags["waiting"] = json!(false);
            }
        }
        match target.as_str() {
            "todo" | "doing" => {
                if (target == "todo") == row.started {
                    return Err(rule("Use doing for started work, todo for unstarted work"));
                }
                if row.flags["blocked"] == true
                    || row.flags["waiting"] == true
                    || self.pending_wait(&row)
                {
                    return Err(rule("Unresolved constraints remain"));
                }
                if row.kind == "job" {
                    self.ordered(&row)?;
                }
                if row.kind == "ticket" {
                    let (ready, blocked) = self.dependencies(&row)?;
                    if !ready || blocked {
                        return Err(rule("Dependencies are not completed"));
                    }
                    if self.current(&row)?.is_some_and(|j| {
                        matches!(
                            j.body["status"].as_str(),
                            Some("waiting" | "paused" | "blocked")
                        )
                    }) {
                        return Err(rule("Current Job is constrained"));
                    }
                }
            }
            "waiting" => {
                if row.flags["blocked"] == true {
                    return Err(rule("Resolve blocking issues before waiting"));
                }
                let derived = row.kind == "ticket"
                    && (!self.dependencies(&row)?.0
                        || self.current(&row)?.is_some_and(|job| {
                            matches!(job.body["status"].as_str(), Some("waiting" | "paused"))
                                || self.pending_wait(&job)
                        }));
                if !derived {
                    row.flags["waiting"] = json!(true);
                }
            }
            "blocked" => {
                row.flags["blocked"] = json!(true);
                row.body["resume"] = json!(self.reason);
            }
            "completed" => {
                if row.kind == "ticket" {
                    if !self.jobs(&row)?.iter().all(|j| terminal(&j.body)) {
                        return Err(rule("Ticket still has non-terminal Jobs"));
                    }
                } else {
                    if p["criteria_met"] != true {
                        return Err(rule(
                            "Completion requires criteria_met=true and result evidence",
                        ));
                    }
                    let result = text(p, "result")?;
                    if row.kind == "job" {
                        row.body["result"] = json!(result);
                    } else {
                        row.flags["completion_evidence"] = json!(result);
                    }
                    if row.flags["waiting"] == true
                        || row.flags["blocked"] == true
                        || self.pending_wait(&row)
                    {
                        return Err(rule("Resolve constraints before completing"));
                    }
                    if row.kind == "job" {
                        self.ordered(&row)?;
                        if !row.started {
                            return Err(rule("Unstarted Job cannot complete"));
                        }
                        if self
                            .children(&row.id)
                            .iter()
                            .any(|r| r.kind == "run" && r.active)
                        {
                            return Err(rule("Finish active Runs before completing a Job"));
                        }
                    } else {
                        let tickets: Vec<_> = self
                            .children(&row.id)
                            .into_iter()
                            .filter(|r| r.kind == "ticket")
                            .collect();
                        row.body.as_object_mut().unwrap().remove("handoff");
                        if tickets.is_empty() || tickets.iter().any(|t| !terminal(&t.body)) {
                            return Err(rule("Task requires at least one Ticket, all terminal"));
                        }
                    }
                }
            }
            "paused" | "canceled" => {
                let mut targets = self.descendants(&row.id);
                if row.kind == "job" {
                    targets.extend(self.children(&row.id));
                }
                let mut seen = BTreeSet::new();
                for mut child in targets {
                    if !seen.insert(child.id.clone()) {
                        continue;
                    }
                    if child.kind == "run" && child.active {
                        if target == "canceled" {
                            self.revoke(child)?;
                        } else {
                            child.flags["stop_requested"] = json!(true);
                            self.save(child, "execution.stop_requested")?;
                        }
                    } else if target == "canceled"
                        && matches!(child.kind.as_str(), "task" | "ticket" | "job")
                        && !terminal(&child.body)
                    {
                        child.body["status"] = json!("canceled");
                        if child.kind == "ticket" {
                            child.body["cancel_reason"] = json!(self.reason);
                        }
                        self.save(child, "status.changed")?;
                    }
                }
            }
            _ => return Err(invalid("Unknown status")),
        }
        if target == "canceled" && row.kind == "job" && p.get("result").is_some() {
            row.body["result"] = json!(text(p, "result")?);
        }
        if target == "canceled" && row.kind == "ticket" {
            row.body["cancel_reason"] = json!(text(p, "cancel_reason")?);
        }
        row.body["status"] = json!(target);
        let id = row.id.clone();
        self.save(row, "status.changed")?;
        self.reconcile()?;
        Ok(self.record_view(&self.get(&id)?))
    }
    fn ordered(&self, job: &record::Model) -> Result<()> {
        let ticket = self.parent(job)?;
        let (ready, blocked) = self.dependencies(&ticket)?;
        if !ready || blocked {
            return Err(rule("Dependencies are not completed"));
        }
        if self.current(&ticket)?.is_none_or(|j| j.id != job.id) {
            return Err(rule("Only the first non-terminal Job may advance"));
        }
        Ok(())
    }
    fn startable(&self, job: &record::Model) -> Result<()> {
        if job.flags["waiting"] == true || job.flags["blocked"] == true || self.pending_wait(job) {
            return Err(rule("Job has unresolved resume constraints"));
        }
        let ticket = self.parent(job)?;
        let task = self.parent(&ticket)?;
        if matches!(
            task.body["status"].as_str(),
            Some("paused" | "completed" | "canceled")
        ) {
            return Err(rule("Task prevents execution"));
        }
        if matches!(
            ticket.body["status"].as_str(),
            Some("paused" | "completed" | "canceled")
        ) {
            return Err(rule("Ticket prevents execution"));
        }
        let (ready, blocked) = self.dependencies(&ticket)?;
        if !ready || blocked {
            return Err(rule("Dependencies are not completed"));
        }
        if ticket.flags["blocked"] == true || ticket.flags["waiting"] == true {
            return Err(rule("Ticket has unresolved constraints"));
        }
        if self.current(&ticket)?.is_none_or(|j| j.id != job.id) {
            return Err(rule("Only the first non-terminal Job may execute"));
        }
        Ok(())
    }
    pub fn start_run(&mut self, p: &Value) -> Result<Value> {
        let mut job = self.get(&text(p, "job")?)?;
        if job.kind != "job" || p["revision"].as_i64() != Some(job.revision) {
            return Err(conflict());
        }
        self.startable(&job)?;
        let mut ticket = self.parent(&job)?;
        if !matches!(job.body["status"].as_str(), Some("todo" | "doing"))
            || !matches!(ticket.body["status"].as_str(), Some("todo" | "doing"))
        {
            return Err(rule("Job and Ticket must be todo or doing"));
        }
        self.authorize(&ticket)?;
        if self
            .jobs(&ticket)?
            .iter()
            .flat_map(|j| self.children(&j.id))
            .any(|r| r.kind == "run" && r.active)
        {
            return Err(rule("A Ticket may have at most one open Run"));
        }
        if p.get("retry_of").is_some() {
            let previous = self.get(&text(p, "retry_of")?)?;
            if previous.kind != "run"
                || previous.parent.as_deref() != Some(job.id.as_str())
                || previous.active
            {
                return Err(rule("Retry requires a closed Run of this Job"));
            }
        }
        let previous: Vec<_> = self
            .jobs(&ticket)?
            .iter()
            .flat_map(|j| self.children(&j.id))
            .collect();
        if previous.iter().any(|r| r.kind == "run") {
            text(p, "safety")?;
        }
        job.started = true;
        job.body["status"] = json!("doing");
        self.save(job.clone(), "status.changed")?;
        ticket.started = true;
        ticket.body["status"] = json!("doing");
        self.save(ticket.clone(), "status.changed")?;
        let mut task = self.parent(&ticket)?;
        if !task.started || task.body["status"] == "todo" {
            task.started = true;
            if task.body["status"] == "todo" {
                task.body["status"] = json!("doing");
            }
            self.save(task, "status.changed")?;
        }
        let mut run_id = json!({});
        if let Some(id) = p.get("id") {
            run_id["id"] = id.clone();
        }
        self.assign_id("run", &mut run_id, &mut BTreeSet::new())?;
        let body = json!({"id":run_id["id"],"started":self.now});
        let id = self.insert("run", Some(job.id.clone()), body, json!({}))?;
        let mut run = self.get(&id)?;
        run.active = true;
        run.flags["order"] = json!(self.children(&job.id).len());
        self.save(run, "run.started")?;
        Ok(
            json!({"run":self.record_view(&self.get(&id)?),"job":self.record_view(&self.get(&job.id)?)}),
        )
    }
    pub fn finish_run(&mut self, p: &Value) -> Result<Value> {
        let mut run = self.expect(p)?;
        self.authorize_run(&json!(run.id), &run)?;
        run.body["result"] = json!(text(p, "result")?);
        run.body["ended"] = json!(self.now);
        run.active = false;
        let id = run.id.clone();
        self.save(run, "run.finished")?;
        Ok(self.record_view(&self.get(&id)?))
    }
    fn authorize_run(&self, value: &Value, target: &record::Model) -> Result<()> {
        let run = self.get(value.as_str().ok_or_else(|| invalid("run must be an ID"))?)?;
        if run.kind != "run" || !run.active || run.body.get("result").is_some() {
            return Err(rule("Run authority has expired"));
        }
        let job = self.parent(&run)?;
        let ticket = self.parent(&job)?;
        let task = self.parent(&ticket)?;
        if ![
            run.id.as_str(),
            job.id.as_str(),
            ticket.id.as_str(),
            task.id.as_str(),
        ]
        .contains(&target.id.as_str())
        {
            return Err(rule("Run does not belong to target"));
        }
        if terminal(&job.body)
            || terminal(&ticket.body)
            || terminal(&task.body)
            || self
                .current(&ticket)?
                .is_none_or(|current| current.id != job.id)
        {
            return Err(rule("Run authority has expired"));
        }
        self.authorize(&ticket)
    }
    pub fn wait_run(&mut self, p: &Value) -> Result<Value> {
        let mut run = self.expect(p)?;
        self.authorize_run(&json!(run.id), &run)?;
        let patch = p
            .get("wait")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("wait must be a single object"))?;
        if !run.body["wait"].is_object() {
            run.body["wait"] = json!({});
        }
        for (key, value) in patch {
            run.body["wait"][key] = value.clone();
        }
        Self::validate("run", &run.body)?;
        let id = run.id.clone();
        let job_id = run.parent.clone().unwrap();
        self.save(run, "run.wait_changed")?;
        let mut job = self.get(&job_id)?;
        let pending = self.pending_wait(&job);
        job.flags["waiting"] = json!(pending);
        if pending && matches!(job.body["status"].as_str(), Some("todo" | "doing")) {
            job.body["status"] = json!("waiting");
        }
        self.save(job, "status.changed")?;
        self.check_cycles()?;
        self.reconcile()?;
        Ok(self.record_view(&self.get(&id)?))
    }
    fn pending_wait(&self, row: &record::Model) -> bool {
        let job = if row.kind == "job" {
            Some(row.clone())
        } else if row.kind == "ticket" {
            self.current(row).ok().flatten()
        } else {
            None
        };
        job.is_some_and(|job| {
            self.children(&job.id).iter().any(|run| {
                run.kind == "run"
                    && run.body["wait"].is_object()
                    && run.body["wait"]["resolved"].is_null()
            })
        })
    }
    pub fn accept_evidence(&mut self, p: &Value) -> Result<Value> {
        if p.get("run").is_some() {
            return Err(rule(
                "Evidence must not originate from an expired execution Run",
            ));
        }
        let job = self.expect(p)?;
        if job.kind != "job" || !matches!(job.body["status"].as_str(), Some("waiting" | "doing")) {
            return Err(rule("Evidence requires the current waiting/doing Job"));
        }
        let ticket = self.parent(&job)?;
        let task = self.parent(&ticket)?;
        if terminal(&ticket.body)
            || matches!(ticket.body["status"].as_str(), Some("paused" | "blocked"))
            || terminal(&task.body)
            || task.body["status"] == "paused"
            || self
                .current(&ticket)?
                .is_none_or(|current| current.id != job.id)
        {
            return Err(rule(
                "Ticket/Task constraints prevent evidence-driven advancement",
            ));
        }
        match p.get("mode").and_then(Value::as_str) {
            Some("management") => self.authorize_management(p)?,
            None | Some("session") => self.authorize(&ticket)?,
            _ => return Err(invalid("Evidence mode must be session or management")),
        }
        let evidence = text(p, "evidence")?;
        let mut run = self.get(&text(p, "evidence_run")?)?;
        if run.kind != "run"
            || run.parent.as_deref() != Some(job.id.as_str())
            || run.active
            || run.body.get("result").is_none()
            || self
                .descendants(&ticket.id)
                .iter()
                .any(|row| row.kind == "run" && row.active)
        {
            return Err(rule(
                "Evidence requires a closed Run belonging to the current Job, with no active attempt",
            ));
        }
        if p["run_revision"].as_i64() != Some(run.revision) {
            return Err(conflict());
        }
        if run.body["wait"].is_object() && run.body["wait"]["resolved"].is_null() {
            if p["wait_met"] != true {
                return Err(rule(
                    "Wait resolution requires verified wait_met=true evidence",
                ));
            }
            run.body["wait"]["resolved"] = json!(self.now);
            run.flags["wait_evidence"] = json!(evidence);
            Self::validate("run", &run.body)?;
            self.save(run.clone(), "run.wait_resolved")?;
        }
        let status = text(p, "status")?;
        if !matches!(status.as_str(), "completed" | "doing" | "todo") {
            return Err(invalid("Evidence status must be completed, doing or todo"));
        }
        let mut transition = p.clone();
        transition["resolved"] = json!(true);
        let mut job = self.get(&job.id)?;
        job.flags["evidence"] = json!({"source_run":run.id,"verification":evidence});
        self.save(job.clone(), "evidence.accepted")?;
        transition["revision"] = json!(self.get(&job.id)?.revision);
        let result = self.set_status_inner(&transition)?;
        self.check_cycles()?;
        Ok(
            json!({"job":result,"run":self.record_view(&self.get(&run.id)?),"ticket":self.record_view(&self.get(&ticket.id)?)}),
        )
    }
    fn reconcile(&mut self) -> Result<()> {
        let ids: Vec<_> = self
            .rows
            .values()
            .filter(|r| r.kind == "ticket" && !r.deleted)
            .map(|r| r.id.clone())
            .collect();
        for _ in 0..=ids.len() {
            let mut changed = false;
            for id in &ids {
                let mut row = self.get(id)?;
                if terminal(&row.body) {
                    continue;
                }
                let (ready, blocked) = self.dependencies(&row)?;
                let current = self.current(&row)?;
                let status = if current.is_none() && !self.jobs(&row)?.is_empty() {
                    "completed"
                } else if row.body["status"] == "paused" {
                    "paused"
                } else if row.flags["blocked"] == true
                    || blocked
                    || current
                        .as_ref()
                        .is_some_and(|j| j.body["status"] == "blocked")
                {
                    "blocked"
                } else if row.flags["waiting"] == true
                    || current
                        .as_ref()
                        .is_some_and(|j| j.body["status"] == "paused")
                    || !ready
                    || current
                        .as_ref()
                        .is_some_and(|j| j.body["status"] == "waiting")
                {
                    "waiting"
                } else if row.started {
                    "doing"
                } else {
                    "todo"
                };
                if row.body["status"] != status {
                    if terminal(&row.body) {
                        return Err(rule("Terminal Ticket state cannot be reopened"));
                    }
                    row.body["status"] = json!(status);
                    self.save(row, "status.changed")?;
                    changed = true;
                }
            }
            if !changed {
                return Ok(());
            }
        }
        Err(rule("Unable to reconcile dependency states"))
    }
    fn restored_guards(
        &self,
        row: &record::Model,
        included: &BTreeSet<String>,
    ) -> Result<(bool, bool, bool)> {
        let started = matches!(row.body["status"].as_str(), Some("doing" | "completed"))
            || self
                .descendants(&row.id)
                .iter()
                .any(|r| r.kind == "run" && included.contains(&r.id));
        let (waiting, blocked) = if row.kind == "ticket" {
            let (ready, dependency_blocked) = self.dependencies(row)?;
            let current = self.current(row)?;
            (
                row.body["status"] == "waiting"
                    && ready
                    && !current.as_ref().is_some_and(|job| {
                        matches!(job.body["status"].as_str(), Some("waiting" | "paused"))
                            || self.pending_wait(job)
                    }),
                row.body["status"] == "blocked"
                    && !dependency_blocked
                    && !current
                        .as_ref()
                        .is_some_and(|job| job.body["status"] == "blocked"),
            )
        } else {
            (
                row.body["status"] == "waiting" || (row.kind == "job" && self.pending_wait(row)),
                row.body["status"] == "blocked",
            )
        };
        Ok((started, waiting, blocked))
    }
    fn validate_open_run(&self, run: &record::Model) -> Result<()> {
        let job = self.parent(run)?;
        let ticket = self.parent(&job)?;
        let task = self.parent(&ticket)?;
        if job.kind != "job"
            || ticket.kind != "ticket"
            || task.kind != "task"
            || terminal(&job.body)
            || terminal(&ticket.body)
            || terminal(&task.body)
            || job.body["status"] == "todo"
            || ticket.body["status"] == "todo"
            || task.body["status"] == "todo"
            || self
                .current(&ticket)?
                .is_none_or(|current| current.id != job.id)
        {
            return Err(rule(
                "An open Run must belong to the current non-terminal Job",
            ));
        }
        self.check_session(&ticket.body)?;
        if self.get(&text(&ticket.body, "session_id")?)?.body["ended"].is_string() {
            return Err(rule("An open Run cannot be restored into an ended Session"));
        }
        for id in strings(&ticket.body, "depends", false)? {
            let dependency = self.raw(&id)?;
            if dependency.kind != "ticket" || dependency.body["status"] != "completed" {
                return Err(rule(
                    "Open Run restoration requires completed prerequisite Tickets",
                ));
            }
        }
        Ok(())
    }
    pub fn export(&self, p: &Value) -> Result<Document> {
        let mut value = json!({"protocol": crate::PROTOCOL});
        let project = p.get("project").map(|_| text(p, "project")).transpose()?;
        if let Some(id) = &project {
            let row = self.get(id)?;
            if row.kind != "project" {
                return Err(invalid("project must identify a Project"));
            }
            value["project"] = self.record_body(&row);
        }
        let tickets = strings(p, "tickets", false)?;
        let mut bodies = vec![];
        let mut included = BTreeSet::new();
        let mut workers = BTreeSet::new();
        let mut sessions = BTreeSet::new();
        for row in self
            .rows
            .values()
            .filter(|r| r.kind == "ticket" && !r.deleted)
        {
            if project
                .as_ref()
                .is_some_and(|id| self.project(row).ok().as_ref() != Some(id))
            {
                continue;
            }
            if !tickets.is_empty() && !tickets.contains(&row.id) {
                continue;
            }
            included.insert(row.id.clone());
            included.extend(
                self.descendants(&row.id)
                    .iter()
                    .filter(|r| !r.deleted)
                    .map(|r| r.id.clone()),
            );
            workers.insert(text(&row.body, "worker_id")?);
            sessions.insert(text(&row.body, "session_id")?);
            bodies.push(self.record_body(row));
        }
        for id in &tickets {
            if self.get(id)?.kind != "ticket" {
                return Err(invalid("tickets must identify Tickets"));
            }
        }
        if let Some(project) = &project {
            included.insert(project.clone());
            included.extend(
                self.children(project)
                    .iter()
                    .filter(|r| !r.deleted && matches!(r.kind.as_str(), "task" | "knowledge"))
                    .map(|r| r.id.clone()),
            );
        }
        for id in &included {
            let row = self.get(id)?;
            if row.kind == "ticket" {
                let dependencies = strings(&row.body, "depends", false)?;
                for dependency in dependencies.into_iter().chain(self.edges(&row)?) {
                    if !included.contains(&dependency) || self.raw(&dependency)?.deleted {
                        return Err(rule(
                            "Export requires all completion dependencies, including deleted prerequisites, to be retained in the document",
                        ));
                    }
                }
            }
            if matches!(row.kind.as_str(), "task" | "ticket" | "job") && !terminal(&row.body) {
                let (started, waiting, blocked) = self.restored_guards(&row, &included)?;
                if row.started != started
                    || (row.flags["waiting"] == true) != waiting
                    || (row.flags["blocked"] == true) != blocked
                {
                    return Err(rule(format!(
                        "Cannot export {} without losing its resume guards; resolve the hidden constraints or restore a representable state first",
                        row.id
                    )));
                }
            }
            if row.kind == "run" && row.active {
                self.validate_open_run(&row)?;
            }
        }
        value["tickets"] = json!(bodies);
        if tickets.is_empty() {
            for row in self.rows.values().filter(|r| !r.deleted) {
                if row.kind == "worker" {
                    workers.insert(row.id.clone());
                }
                if row.kind == "session" {
                    sessions.insert(row.id.clone());
                }
            }
        }
        value["workers"] = json!(
            workers
                .iter()
                .map(|id| self.get(id).map(|r| r.body))
                .collect::<Result<Vec<_>>>()?
        );
        value["sessions"] = json!(
            sessions
                .iter()
                .map(|id| self.get(id).map(|r| r.body))
                .collect::<Result<Vec<_>>>()?
        );
        Document::parse(&value)
    }
    pub fn import(&mut self, p: &Value) -> Result<Value> {
        let values = p
            .get("documents")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("documents must be a TTJ document array"))?;
        let document = Document::merge(
            values
                .iter()
                .map(Document::parse)
                .collect::<Result<Vec<_>>>()?,
        )?;
        let mut pending = vec![];
        for worker in document.workers {
            pending.push(("worker", None, document::to_value(&worker), 0));
        }
        for session in document.sessions {
            pending.push(("session", None, document::to_value(&session), 0));
        }
        if let Some(mut project) = document.project {
            let tasks = std::mem::take(&mut project.tasks);
            let knowledge = std::mem::take(&mut project.knowledge);
            pending.push(("project", None, document::to_value(&project), 0));
            for knowledge in knowledge {
                pending.push((
                    "knowledge",
                    Some(project.id.clone()),
                    document::to_value(&knowledge),
                    0,
                ));
            }
            for task in tasks {
                pending.push((
                    "task",
                    Some(project.id.clone()),
                    document::to_value(&task),
                    0,
                ));
            }
        }
        for mut ticket in document.tickets {
            let jobs = std::mem::take(&mut ticket.jobs);
            pending.push((
                "ticket",
                Some(ticket.task_id.clone()),
                document::to_value(&ticket),
                0,
            ));
            for (order, mut job) in jobs.into_iter().enumerate() {
                let runs = std::mem::take(&mut job.runs);
                pending.push((
                    "job",
                    Some(ticket.id.clone()),
                    document::to_value(&job),
                    order,
                ));
                for (order, run) in runs.into_iter().enumerate() {
                    pending.push(("run", Some(job.id.clone()), document::to_value(&run), order));
                }
            }
        }
        let mut count = 0;
        for (kind, parent, body, order) in pending {
            let id = text(&body, "id")?;
            if let Some(old) = self.rows.get(&id) {
                if matches!(kind, "worker" | "session")
                    && !old.deleted
                    && old.kind == kind
                    && old.body == body
                {
                    continue;
                }
                return Err(conflict());
            }
            if self.reserved_ids.contains(&id) {
                return Err(conflict());
            }
            if let Some(parent) = &parent {
                self.get(parent)?;
            }
            if kind == "session" && self.get(&text(&body, "worker_id")?)?.kind != "worker" {
                return Err(rule("Session needs a Worker"));
            }
            if kind == "ticket" {
                let task = self.get(&text(&body, "task_id")?)?;
                if task.kind != "task" || (terminal(&task.body) && !terminal(&body)) {
                    return Err(rule("Ticket needs a Task"));
                }
                self.check_session(&body)?;
            }
            let active = kind == "run" && body.get("result").is_none();
            let started = active || matches!(body["status"].as_str(), Some("doing" | "completed"));
            self.insert(kind, parent, body, json!({}))?;
            let mut row = self.get(&id)?;
            row.active = active;
            row.started = started;
            row.flags["order"] = json!(order);
            if row.kind == "job" || row.kind == "task" {
                row.flags["blocked"] = json!(row.body["status"] == "blocked");
                row.flags["waiting"] = json!(row.body["status"] == "waiting");
            }
            self.save(row, "record.imported")?;
            count += 1;
        }
        let included: BTreeSet<_> = self.rows.keys().cloned().collect();
        let work: Vec<_> = self
            .changes
            .iter()
            .filter(|change| {
                change.event == "record.created"
                    && matches!(change.row.kind.as_str(), "task" | "ticket" | "job")
            })
            .map(|change| change.row.id.clone())
            .collect();
        for id in work {
            let mut row = self.get(&id)?;
            let (started, waiting, blocked) = self.restored_guards(&row, &included)?;
            row.started = started;
            row.flags["waiting"] = json!(waiting);
            row.flags["blocked"] = json!(blocked);
            self.save(row, "record.imported")?;
        }
        let ancestors: Vec<_> = self
            .rows
            .values()
            .filter(|row| {
                !row.deleted
                    && matches!(row.kind.as_str(), "task" | "ticket" | "job")
                    && !row.started
            })
            .cloned()
            .collect();
        for mut row in ancestors {
            if self
                .descendants(&row.id)
                .iter()
                .any(|child| child.kind == "run")
            {
                row.started = true;
                self.save(row, "record.imported")?;
            }
        }
        for run in self
            .rows
            .values()
            .filter(|row| row.kind == "run" && row.active && !row.deleted)
        {
            self.validate_open_run(run)?;
        }
        for ticket in self
            .rows
            .values()
            .filter(|r| r.kind == "ticket" && !r.deleted)
        {
            let body = self.record_body(ticket);
            Self::validate("ticket", &body)?;
            for id in strings(&body, "depends", false)? {
                if self.raw(&id)?.kind != "ticket" {
                    return Err(rule("depends must reference Tickets"));
                }
            }
            self.dependencies(ticket)?;
        }
        self.check_cycles()?;
        Ok(json!({"imported": count}))
    }
    pub fn apply(&mut self, method: &str, p: &Value) -> Result<Value> {
        match method {
            "document.import" => {
                self.authorize_management(p)?;
                self.import(p)
            }
            "job.accept_evidence" => self.accept_evidence(p),
            "task.purge" => self.purge_task(p),
            "status.set" => self.set_status(p),
            "run.start" => self.start_run(p),
            "run.finish" => self.finish_run(p),
            "run.wait" => self.wait_run(p),
            _ => {
                let (kind, action) = method
                    .split_once('.')
                    .ok_or_else(|| invalid("Unknown method"))?;
                match action {
                    "create" => self.create(kind, p),
                    "update" => self.update(kind, p),
                    "delete" => self.delete(kind, p),
                    _ => Err(invalid("Unknown mutation")),
                }
            }
        }
    }
}

fn omit_empty_arrays(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|_, v| !v.as_array().is_some_and(Vec::is_empty));
            for child in object.values_mut() {
                omit_empty_arrays(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                omit_empty_arrays(item);
            }
        }
        _ => {}
    }
}

fn initial_status(body: &mut Value) -> Result<()> {
    if body.get("status").is_some_and(|status| status != "todo") {
        return Err(invalid(
            "New work must start as todo; use status.set or document.import",
        ));
    }
    body["status"] = json!("todo");
    Ok(())
}
fn increment(number: &str) -> String {
    let mut bytes = number.as_bytes().to_vec();
    for byte in bytes.iter_mut().rev() {
        if *byte < b'9' {
            *byte += 1;
            return String::from_utf8(bytes).unwrap();
        }
        *byte = b'0';
    }
    bytes.insert(0, b'1');
    String::from_utf8(bytes).unwrap()
}
