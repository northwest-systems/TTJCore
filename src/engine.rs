use crate::{Result, conflict, entity::record, invalid, rule};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

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
    json!({"record":row.body,"revision":row.revision,"deleted":row.deleted,
        "started":row.started,"active":row.active,"constraints":row.flags,
        "display":row.flags.get("display").cloned().unwrap_or(json!({}))})
}
pub struct Change {
    pub row: record::Model,
    pub before: Option<Value>,
    pub event: String,
    pub project: String,
}
pub struct Engine {
    pub rows: BTreeMap<String, record::Model>,
    pub changes: Vec<Change>,
    pub actor: Vec<String>,
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
        if let Some(previous) = self.rows.get(&row.id) {
            if previous.kind == row.kind
                && previous.parent == row.parent
                && previous.body == row.body
                && previous.started == row.started
                && previous.deleted == row.deleted
                && previous.active == row.active
                && previous.flags == row.flags
            {
                return Ok(());
            }
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
        text(body, "id")?;
        match kind {
            "project" => {
                text(body, "name")?;
            }
            "task" | "job" => {
                text(body, "objective")?;
                strings(body, "criteria", true)?;
            }
            "ticket" => {
                text(body, "objective")?;
                strings(body, "jobs", true)?;
            }
            "knowledge" => {
                text(body, "content")?;
                text(body, "basis")?;
            }
            _ => {}
        }
        for field in ["assignee", "depends", "handoffs"] {
            if body.get(field).is_some() {
                strings(body, field, false)?;
            }
        }
        if let Some(deadline) = body.get("deadline") {
            let date = deadline
                .as_str()
                .ok_or_else(|| invalid("deadline must be a datetime"))?;
            OffsetDateTime::parse(date, &Rfc3339)
                .map_err(|_| invalid("deadline needs a valid timezone-qualified datetime"))?;
        }
        if let Some(resume) = body.get("resume") {
            if !resume.is_string() {
                return Err(invalid("resume must be a string"));
            }
        }
        if let Some(priority) = body.get("priority") {
            if !priority.is_i64() {
                return Err(invalid("priority must be an integer"));
            }
        }
        Ok(())
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
        strings(&ticket.body, "jobs", true)?
            .iter()
            .map(|id| self.raw(id))
            .collect()
    }
    fn current(&self, ticket: &record::Model) -> Result<Option<record::Model>> {
        Ok(self.jobs(ticket)?.into_iter().find(|r| !terminal(&r.body)))
    }
    fn dependencies(&self, ticket: &record::Model) -> Result<(bool, bool)> {
        let mut ready = true;
        let mut blocked = false;
        for id in strings(&ticket.body, "depends", false)? {
            let dependency = self.raw(&id)?;
            if dependency.kind != "ticket" {
                return Err(rule("depends must reference Tickets"));
            }
            ready &= dependency.body["status"] == "completed";
            blocked |= dependency.body["status"] == "canceled";
        }
        Ok((ready, blocked))
    }
    fn check_cycles(&self) -> Result<()> {
        for ticket in self.rows.values().filter(|r| r.kind == "ticket") {
            let mut seen = BTreeSet::new();
            let mut queue = strings(&ticket.body, "depends", false)?;
            while let Some(id) = queue.pop() {
                if id == ticket.id {
                    return Err(rule("Ticket dependencies must be acyclic"));
                }
                if !seen.insert(id.clone()) {
                    continue;
                }
                let row = self.raw(&id)?;
                if row.kind != "ticket" {
                    return Err(rule("depends must reference Tickets"));
                }
                queue.extend(strings(&row.body, "depends", false)?);
            }
        }
        Ok(())
    }
    fn assign_id(body: &mut Value) -> Result<()> {
        if !body.is_object() {
            return Err(invalid("record must be an object"));
        }
        if body.get("id").is_none() {
            body["id"] = json!(uuid::Uuid::new_v4().to_string());
        }
        text(body, "id")?;
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
        Self::assign_id(&mut body)?;
        let display = Self::display(p.get("display"))?;
        let mut jobs = vec![];
        let mut job_displays = vec![];
        let parent = match kind {
            "project" => {
                Self::allowed(&body, &["id", "name"])?;
                None
            }
            "task" => {
                Self::allowed(
                    &body,
                    &["id", "project", "objective", "criteria", "deadline"],
                )?;
                let parent = self.get(&text(&body, "project")?)?;
                if parent.kind != "project" {
                    return Err(rule("Task needs a Project"));
                }
                Some(parent.id)
            }
            "ticket" => {
                Self::allowed(
                    &body,
                    &[
                        "id",
                        "task",
                        "objective",
                        "jobs",
                        "assignee",
                        "depends",
                        "handoffs",
                        "resume",
                        "priority",
                    ],
                )?;
                let parent = self.get(&text(&body, "task")?)?;
                if parent.kind != "task" || terminal(&parent.body) {
                    return Err(rule("Ticket needs a non-terminal Task"));
                }
                jobs = body["jobs"]
                    .as_array()
                    .filter(|a| !a.is_empty())
                    .cloned()
                    .ok_or_else(|| {
                        invalid("jobs must contain one or more complete Job definitions")
                    })?;
                let mut ids = vec![];
                for job in &mut jobs {
                    Self::assign_id(job)?;
                    let display = job.as_object_mut().unwrap().remove("display");
                    job_displays.push(Self::display(display.as_ref())?);
                    Self::allowed(
                        job,
                        &["id", "objective", "criteria", "instructions", "assignee"],
                    )?;
                    Self::validate("job", job)?;
                    let id = text(job, "id")?;
                    if ids.contains(&id) || self.rows.contains_key(&id) {
                        return Err(conflict());
                    }
                    if let Some(instructions) = job.get("instructions") {
                        if !instructions.is_string() {
                            return Err(invalid("instructions must be a string"));
                        }
                    }
                    ids.push(id);
                }
                body["jobs"] = json!(ids);
                Some(parent.id)
            }
            "knowledge" => {
                Self::allowed(&body, &["id", "project", "content", "basis", "supersedes"])?;
                let parent = self.get(&text(&body, "project")?)?;
                if parent.kind != "project" {
                    return Err(rule("Knowledge needs a Project"));
                }
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
                body.as_object_mut().unwrap().remove("project");
                body["status"] = json!("active");
                Some(parent.id)
            }
            _ => {
                return Err(rule(
                    "Jobs are created atomically inside ticket.create; Runs use run.start",
                ));
            }
        };
        if matches!(kind, "task" | "ticket") {
            body["status"] = json!("todo");
        }
        Self::validate(kind, &body)?;
        let id = self.insert(kind, parent, body, display)?;
        for (mut job, display) in jobs.into_iter().zip(job_displays) {
            job["status"] = json!("todo");
            self.insert("job", Some(id.clone()), job, display)?;
        }
        self.check_cycles()?;
        self.reconcile()?;
        Ok(view(&self.get(&id)?))
    }
    pub fn update(&mut self, kind: &str, p: &Value) -> Result<Value> {
        let mut row = self.expect(p)?;
        if row.kind != kind {
            return Err(invalid("Record kind does not match method"));
        }
        let patch = p.get("patch").ok_or_else(|| invalid("patch is required"))?;
        let allowed: &[&str] = match kind {
            "project" => &["name"],
            "task" => &["objective", "criteria", "deadline"],
            "ticket" => &["assignee", "depends", "handoffs", "resume", "priority"],
            "job" => &["assignee", "resume"],
            "knowledge" => &["status"],
            _ => return Err(rule("Runs are updated through run.finish")),
        };
        Self::allowed(patch, allowed)?;
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
        let previous_assignee = row.body.get("assignee").cloned();
        for (key, value) in patch.as_object().unwrap() {
            if value.is_null() && matches!(key.as_str(), "deadline" | "resume" | "assignee") {
                row.body.as_object_mut().unwrap().remove(key);
            } else {
                row.body[key] = value.clone();
            }
        }
        Self::validate(kind, &row.body)?;
        let id = row.id.clone();
        let assignment_changed = previous_assignee != row.body.get("assignee").cloned();
        self.save(row, "record.updated")?;
        if assignment_changed {
            let runs: Vec<_> = self
                .rows
                .values()
                .filter(|run| run.kind == "run" && run.active)
                .cloned()
                .collect();
            for run in runs {
                let job = self.raw(run.parent.as_deref().unwrap_or(""))?;
                let inherited = job.body.get("assignee").is_none();
                let affected = job.id == id
                    || (kind == "ticket"
                        && inherited
                        && job.parent.as_deref() == Some(id.as_str()));
                if !affected {
                    continue;
                }
                let assigned = if inherited {
                    strings(&self.parent(&job)?.body, "assignee", false)?
                } else {
                    strings(&job.body, "assignee", false)?
                };
                if !assigned.is_empty()
                    && strings(&run.body, "actor", true)?
                        .iter()
                        .any(|a| !assigned.contains(a))
                {
                    self.revoke(run)?;
                }
            }
        }
        self.check_cycles()?;
        self.reconcile()?;
        Ok(view(&self.get(&id)?))
    }
    fn revoke(&mut self, mut run: record::Model) -> Result<()> {
        run.active = false;
        run.flags["revoked"] = json!(true);
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
        for mut target in targets {
            if target.deleted {
                continue;
            }
            target.deleted = true;
            self.save(target, "record.deleted")?;
        }
        Ok(view(&self.raw(&row.id)?))
    }
    pub fn set_status(&mut self, p: &Value) -> Result<Value> {
        let mut row = self.expect(p)?;
        if !matches!(row.kind.as_str(), "task" | "ticket" | "job") {
            return Err(rule("This record has no work status"));
        }
        let target = text(p, "status")?;
        let source = text(&row.body, "status")?;
        if source == target {
            return Ok(view(&row));
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
                if row.flags["blocked"] == true || row.flags["waiting"] == true {
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
                row.flags["waiting"] = json!(true);
            }
            "blocked" => {
                row.flags["blocked"] = json!(true);
                let mut ticket = if row.kind == "job" {
                    self.parent(&row)?
                } else {
                    row.clone()
                };
                if ticket.kind == "ticket" {
                    let mut notes = strings(&ticket.body, "handoffs", false)?;
                    notes.push(format!("課題：{}", self.reason));
                    ticket.body["handoffs"] = json!(notes);
                    if ticket.id == row.id {
                        row.body["handoffs"] = ticket.body["handoffs"].clone();
                    } else {
                        self.save(ticket, "record.updated")?;
                    }
                }
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
                    row.body["result"] = json!(text(p, "result")?);
                    if row.flags["waiting"] == true || row.flags["blocked"] == true {
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
                        self.save(child, "status.changed")?;
                    }
                }
            }
            _ => return Err(invalid("Unknown status")),
        }
        if let Some(run_id) = p.get("run") {
            let run = self.get(
                run_id
                    .as_str()
                    .ok_or_else(|| invalid("run must be an id"))?,
            )?;
            if run.kind != "run"
                || !run.active
                || run.parent.as_deref() != Some(row.id.as_str())
                || run.flags["revoked"] == true
                || run.body["generation"] != row.flags["generation"]
            {
                return Err(rule("Run authority has expired"));
            }
            let participants = strings(&run.body, "actor", true)?;
            if self.actor.iter().any(|actor| !participants.contains(actor)) {
                return Err(rule("Only Run participants may change Job status"));
            }
        }
        row.body["status"] = json!(target);
        let id = row.id.clone();
        self.save(row, "status.changed")?;
        self.reconcile()?;
        Ok(view(&self.get(&id)?))
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
        let assigned = if job.body.get("assignee").is_some() {
            strings(&job.body, "assignee", false)?
        } else {
            strings(&ticket.body, "assignee", false)?
        };
        if !assigned.is_empty() && self.actor.iter().any(|a| !assigned.contains(a)) {
            return Err(rule("Actor is not assigned to this Job"));
        }
        if let Some(previous) = p.get("retry_of") {
            let old = self.get(
                previous
                    .as_str()
                    .ok_or_else(|| invalid("retry_of must be an id"))?,
            )?;
            if old.kind != "run" || old.parent.as_deref() != Some(job.id.as_str()) {
                return Err(rule("Retry must reference this Job's Run"));
            }
            text(p, "safety")?;
            self.revoke(old)?;
        }
        let runs = self.children(&job.id);
        let active: Vec<_> = runs
            .iter()
            .filter(|r| r.kind == "run" && r.active)
            .collect();
        if active.iter().any(|r| {
            strings(&r.body, "actor", true)
                .unwrap_or_default()
                .iter()
                .any(|a| self.actor.contains(a))
        }) {
            return Err(rule(
                "Actor already has an active Run; use the same operation or an explicit safe retry",
            ));
        }
        let prior_jobs: Vec<_> = self
            .jobs(&ticket)?
            .into_iter()
            .take_while(|j| j.id != job.id)
            .collect();
        let unsafe_previous = runs
            .iter()
            .any(|r| !r.active && r.body.get("ended").is_none())
            || prior_jobs
                .iter()
                .flat_map(|j| self.children(&j.id))
                .any(|r| r.kind == "run" && r.body.get("ended").is_none());
        if unsafe_previous {
            text(p, "safety")?;
        }
        let generation = if active.is_empty() {
            job.flags["generation"].as_i64().unwrap_or(0) + 1
        } else {
            job.flags["generation"].as_i64().unwrap_or(1)
        };
        job.started = true;
        job.flags["generation"] = json!(generation);
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
        Self::assign_id(&mut run_id)?;
        let body = json!({"id":run_id["id"],"job":job.id,"actor":self.actor,"started":self.now,"generation":generation});
        let id = self.insert("run", Some(job.id.clone()), body, json!({}))?;
        let mut run = self.get(&id)?;
        run.active = true;
        self.save(run, "run.started")?;
        Ok(json!({"run":view(&self.get(&id)?),"job":view(&self.get(&job.id)?)}))
    }
    pub fn finish_run(&mut self, p: &Value) -> Result<Value> {
        let mut run = self.expect(p)?;
        if run.kind != "run" || !run.active || run.flags["revoked"] == true {
            return Err(rule("Run authority has expired"));
        }
        let job = self.parent(&run)?;
        if terminal(&job.body) || run.body["generation"] != job.flags["generation"] {
            return Err(rule("Run generation has expired"));
        }
        let participants = strings(&run.body, "actor", true)?;
        if self.actor.iter().any(|a| !participants.contains(a)) {
            return Err(rule("Only Run participants may finish it"));
        }
        run.body["result"] = json!(text(p, "result")?);
        run.body["ended"] = json!(self.now);
        run.active = false;
        let id = run.id.clone();
        self.save(run, "run.finished")?;
        Ok(view(&self.get(&id)?))
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
                if row.body["status"] == "canceled" {
                    continue;
                }
                let (ready, blocked) = self.dependencies(&row)?;
                let current = self.current(&row)?;
                let status = if current.is_none() {
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
                } else if current
                    .as_ref()
                    .is_some_and(|j| j.body["status"] == "paused")
                {
                    "waiting"
                } else if row.flags["waiting"] == true
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
    pub fn apply(&mut self, method: &str, p: &Value) -> Result<Value> {
        match method {
            "status.set" => self.set_status(p),
            "run.start" => self.start_run(p),
            "run.finish" => self.finish_run(p),
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
