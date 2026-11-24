//! TTJ 1.1.0 JSON document model.
//!
//! A TTJ document always uses the same root schema: `protocol`, `workers[]`,
//! `sessions[]`, the single `project` object (with `knowledge[]` and `tasks[]`)
//! and the root `tickets[]` (with `jobs[].runs[]`). Empty arrays and an absent
//! `project` are omitted on output and read back as empty, so one schema covers
//! both a single combined file and Task/Ticket split files.
use crate::{PROTOCOL, Result, invalid, rule};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// Record kinds whose IDs follow `<prefix>-<6 or 8 alphanumerics>-<n>`.
pub const ID_KINDS: [(&str, &str); 7] = [
    ("project", "pj"),
    ("task", "tsk"),
    ("ticket", "tkt"),
    ("job", "job"),
    ("run", "run"),
    ("worker", "wkr"),
    ("session", "ssn"),
];
pub const STATUSES: [&str; 7] = [
    "todo",
    "doing",
    "waiting",
    "paused",
    "blocked",
    "completed",
    "canceled",
];

pub fn prefix(kind: &str) -> Option<&'static str> {
    ID_KINDS.iter().find(|(k, _)| *k == kind).map(|(_, p)| *p)
}
/// Checks `^(pj|tsk|tkt|job|run|wkr|ssn)-[A-Za-z0-9]{6}([A-Za-z0-9]{2})?-[1-9][0-9]*$`.
pub fn is_id(id: &str) -> bool {
    ID_KINDS.iter().any(|(kind, _)| is_id_of(kind, id))
}
pub fn is_id_of(kind: &str, id: &str) -> bool {
    let Some(prefix) = prefix(kind) else {
        return false;
    };
    let Some(rest) = id.strip_prefix(prefix).and_then(|r| r.strip_prefix('-')) else {
        return false;
    };
    let Some((series, number)) = rest.split_once('-') else {
        return false;
    };
    matches!(series.len(), 6 | 8)
        && series.bytes().all(|c| c.is_ascii_alphanumeric())
        && number
            .bytes()
            .next()
            .is_some_and(|c| (b'1'..=b'9').contains(&c))
        && number.bytes().all(|c| c.is_ascii_digit())
}
/// Splits a TTJ ID into its series and increment for natural ordering.
pub fn sequence(id: &str) -> Option<(&str, u64)> {
    let (head, number) = id.rsplit_once('-')?;
    Some((head, number.parse().ok()?))
}

fn fail(message: impl Into<String>) -> crate::Error {
    invalid(message)
}
fn id_field(kind: &str, field: &str, id: &str) -> Result<()> {
    if is_id_of(kind, id) {
        Ok(())
    } else {
        Err(fail(format!(
            "{field} must be a {kind} ID like {}-a1b2c3d4-1",
            prefix(kind).unwrap_or_default()
        )))
    }
}
fn non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(fail(format!("{field} must be a non-empty string")))
    } else {
        Ok(())
    }
}
fn optional_text(field: &str, value: &Option<String>) -> Result<()> {
    value.as_deref().map_or(Ok(()), |v| non_empty(field, v))
}
fn texts(field: &str, values: &[String], required: bool) -> Result<()> {
    if required && values.is_empty() {
        return Err(fail(format!("{field} must not be empty")));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        non_empty(field, value)
            .map_err(|_| fail(format!("{field} must contain non-empty strings")))?;
        if !seen.insert(value) {
            return Err(fail(format!("{field} contains duplicates")));
        }
    }
    Ok(())
}
pub fn datetime(field: &str, value: &str) -> Result<()> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|_| ())
        .map_err(|_| fail(format!("{field} needs a valid timezone-qualified datetime")))
}
fn optional_datetime(field: &str, value: &Option<String>) -> Result<()> {
    value.as_deref().map_or(Ok(()), |v| datetime(field, v))
}
fn status(value: &str) -> Result<()> {
    if STATUSES.contains(&value) {
        Ok(())
    } else {
        Err(fail(
            "status must be todo, doing, waiting, paused, blocked, completed or canceled",
        ))
    }
}
fn todo() -> String {
    "todo".into()
}
fn active() -> String {
    "active".into()
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workers: Vec<Worker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<Session>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<Project>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<Ticket>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worker {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: String,
    pub worker_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge: Vec<Knowledge>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Task>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub id: String,
    #[serde(default = "active")]
    pub status: String,
    pub content: String,
    pub basis: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub objective: String,
    pub criteria: Vec<String>,
    #[serde(default = "todo")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<Handoff>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_ticket_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_position: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    pub id: String,
    pub task_id: String,
    pub worker_id: String,
    pub session_id: String,
    pub objective: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclusions: Vec<String>,
    #[serde(default = "todo")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<Ref>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<Job>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ref {
    pub ticket_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: String,
    pub objective: String,
    pub criteria: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default = "todo")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<Run>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub id: String,
    pub started: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<Wait>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wait {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_point: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
}

impl Worker {
    pub fn check(&self) -> Result<()> {
        id_field("worker", "Worker.id", &self.id)?;
        non_empty("Worker.name", &self.name)
    }
}
impl Session {
    pub fn check(&self) -> Result<()> {
        id_field("session", "Session.id", &self.id)?;
        id_field("worker", "Session.worker_id", &self.worker_id)?;
        optional_datetime("Session.started", &self.started)?;
        optional_datetime("Session.ended", &self.ended)
    }
}
impl Project {
    pub fn check(&self) -> Result<()> {
        id_field("project", "Project.id", &self.id)?;
        non_empty("Project.name", &self.name)?;
        for knowledge in &self.knowledge {
            knowledge.check()?;
        }
        for task in &self.tasks {
            task.check()?;
        }
        Ok(())
    }
}
impl Knowledge {
    pub fn check(&self) -> Result<()> {
        non_empty("Knowledge.id", &self.id)?;
        non_empty("Knowledge.content", &self.content)?;
        non_empty("Knowledge.basis", &self.basis)?;
        optional_text("Knowledge.supersedes", &self.supersedes)?;
        if !matches!(self.status.as_str(), "active" | "supersed") {
            return Err(fail("Knowledge status must be active or supersed"));
        }
        Ok(())
    }
}
impl Task {
    pub fn check(&self) -> Result<()> {
        id_field("task", "Task.id", &self.id)?;
        non_empty("objective", &self.objective)?;
        texts("criteria", &self.criteria, true)?;
        status(&self.status)?;
        optional_datetime("deadline", &self.deadline)?;
        optional_text("resume", &self.resume)?;
        if let Some(handoff) = &self.handoff {
            if self.status == "completed" {
                return Err(rule("A completed Task must not keep its volatile handoff"));
            }
            handoff.check()?;
        }
        Ok(())
    }
}
impl Handoff {
    pub fn check(&self) -> Result<()> {
        for (kind, field, value) in [
            (
                "ticket",
                "handoff.current_ticket_id",
                &self.current_ticket_id,
            ),
            ("worker", "handoff.worker_id", &self.worker_id),
            ("session", "handoff.session_id", &self.session_id),
            ("job", "handoff.current_job_id", &self.current_job_id),
        ] {
            if let Some(id) = value {
                id_field(kind, field, id)?;
            }
        }
        texts("handoff.blockers", &self.blockers, false)?;
        texts("handoff.notes", &self.notes, false)?;
        optional_datetime("handoff.updated", &self.updated)
    }
}
impl Ticket {
    pub fn check(&self) -> Result<()> {
        id_field("ticket", "Ticket.id", &self.id)?;
        id_field("task", "task_id", &self.task_id)?;
        id_field("worker", "worker_id", &self.worker_id)?;
        id_field("session", "session_id", &self.session_id)?;
        non_empty("objective", &self.objective)?;
        texts("exclusions", &self.exclusions, false)?;
        status(&self.status)?;
        texts("depends", &self.depends, false)?;
        for id in &self.depends {
            id_field("ticket", "depends", id)?;
            if *id == self.id {
                return Err(rule("Ticket dependencies must be acyclic"));
            }
        }
        for reference in &self.refs {
            id_field("ticket", "refs.ticket_id", &reference.ticket_id)?;
            optional_text("refs.kind", &reference.kind)?;
            optional_text("refs.note", &reference.note)?;
        }
        texts("artifacts", &self.artifacts, false)?;
        optional_text("cancel_reason", &self.cancel_reason)?;
        optional_text("resume", &self.resume)?;
        if self.status == "canceled" && self.cancel_reason.is_none() {
            return Err(rule("A canceled Ticket requires cancel_reason"));
        }
        let mut open = 0;
        for job in &self.jobs {
            if matches!(self.status.as_str(), "completed" | "canceled")
                && !matches!(job.status.as_str(), "completed" | "canceled")
            {
                return Err(rule("A terminal Ticket cannot have non-terminal Jobs"));
            }
            job.check()?;
            open += job.runs.iter().filter(|run| run.result.is_none()).count();
        }
        if open > 1 {
            return Err(rule("A Ticket may have at most one open Run"));
        }
        Ok(())
    }
}
impl Job {
    pub fn check(&self) -> Result<()> {
        id_field("job", "Job.id", &self.id)?;
        non_empty("objective", &self.objective)?;
        texts("criteria", &self.criteria, true)?;
        optional_text("instructions", &self.instructions)?;
        status(&self.status)?;
        optional_text("result", &self.result)?;
        texts("artifacts", &self.artifacts, false)?;
        optional_text("resume", &self.resume)?;
        for run in &self.runs {
            if matches!(self.status.as_str(), "completed" | "canceled") && run.result.is_none() {
                return Err(rule("A terminal Job cannot have an open Run"));
            }
            run.check()?;
        }
        Ok(())
    }
}
impl Run {
    pub fn check(&self) -> Result<()> {
        id_field("run", "Run.id", &self.id)?;
        datetime("Run.started", &self.started)?;
        optional_datetime("Run.ended", &self.ended)?;
        optional_text("Run.result", &self.result)?;
        if self.ended.is_some() && self.result.is_none() {
            return Err(rule("A finished Run requires result"));
        }
        if let Some(wait) = &self.wait {
            wait.check()?;
        }
        Ok(())
    }
}
impl Wait {
    pub fn check(&self) -> Result<()> {
        optional_text("wait.target", &self.target)?;
        optional_text("wait.condition", &self.condition)?;
        optional_text("wait.resume_point", &self.resume_point)?;
        optional_datetime("wait.since", &self.since)?;
        optional_datetime("wait.timeout", &self.timeout)?;
        optional_datetime("wait.resolved", &self.resolved)
    }
}

fn parse<T: for<'de> Deserialize<'de>>(what: &str, value: &Value) -> Result<T> {
    serde_json::from_value(value.clone()).map_err(|e| fail(format!("Invalid {what}: {e}")))
}
/// Validates one stored record body against the TTJ 1.1.0 schema.
pub fn check_record(kind: &str, body: &Value) -> Result<()> {
    match kind {
        "worker" => parse::<Worker>("Worker", body)?.check(),
        "session" => parse::<Session>("Session", body)?.check(),
        "project" => parse::<Project>("Project", body)?.check(),
        "knowledge" => parse::<Knowledge>("Knowledge", body)?.check(),
        "task" => parse::<Task>("Task", body)?.check(),
        "ticket" => parse::<Ticket>("Ticket", body)?.check(),
        "job" => parse::<Job>("Job", body)?.check(),
        "run" => parse::<Run>("Run", body)?.check(),
        _ => Err(fail("Unknown record kind")),
    }
}
pub fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("TTJ records serialize to JSON")
}

impl Document {
    pub fn read(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|_| fail("Cannot read TTJ file"))?;
        let value = serde_json::from_str(&text).map_err(|_| fail("Invalid TTJ JSON"))?;
        Self::parse(&value)
    }
    pub fn write(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        self.validate()?;
        let text =
            serde_json::to_string_pretty(self).map_err(|_| fail("Cannot serialize TTJ file"))?;
        std::fs::write(path, format!("{text}\n")).map_err(|_| fail("Cannot write TTJ file"))
    }
    pub fn write_split(
        &self,
        directory: impl AsRef<std::path::Path>,
    ) -> Result<Vec<std::path::PathBuf>> {
        self.validate()?;
        let directory = directory.as_ref();
        std::fs::create_dir_all(directory).map_err(|_| fail("Cannot create TTJ directory"))?;
        let lock_path = directory.join(".ttjcore-export.lock");
        let lock = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .map_err(|_| fail("Split output directory is already in use"))?;
        let result = (|| {
            for entry in
                std::fs::read_dir(directory).map_err(|_| fail("Cannot inspect TTJ directory"))?
            {
                if entry
                    .map_err(|_| fail("Cannot inspect TTJ directory"))?
                    .path()
                    != lock_path
                {
                    return Err(fail("Split output directory must be empty"));
                }
            }
            let mut paths = vec![];
            for (name, document) in self.split() {
                let path = directory.join(name);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|_| fail("Cannot create TTJ directory"))?;
                }
                document.write(&path)?;
                paths.push(path);
            }
            Ok(paths)
        })();
        drop(lock);
        let cleanup = std::fs::remove_file(lock_path)
            .map_err(|_| fail("Cannot release split output directory"));
        result.and_then(|paths| cleanup.map(|()| paths))
    }
    pub fn new() -> Self {
        Self {
            protocol: PROTOCOL.into(),
            ..Self::default()
        }
    }
    /// Reads a TTJ 1.1.0 document. Omitted arrays become empty arrays.
    pub fn parse(value: &Value) -> Result<Self> {
        if value.get("protocol").and_then(Value::as_str) != Some(PROTOCOL) {
            return Err(fail(format!("protocol must be {PROTOCOL}")));
        }
        let document: Self = parse("TTJ document", value)?;
        document.validate()?;
        Ok(document)
    }
    pub fn to_value(&self) -> Value {
        to_value(self)
    }
    /// Checks every record and the references that can be resolved inside this document.
    pub fn validate(&self) -> Result<()> {
        if self.protocol != PROTOCOL {
            return Err(fail(format!("protocol must be {PROTOCOL}")));
        }
        let mut ids = BTreeSet::new();
        let mut unique = |id: &str| {
            if ids.insert(id.to_owned()) {
                Ok(())
            } else {
                Err(fail(format!("Duplicate ID {id}")))
            }
        };
        let mut workers = BTreeMap::new();
        for worker in &self.workers {
            worker.check()?;
            unique(&worker.id)?;
            workers.insert(worker.id.as_str(), worker);
        }
        let mut sessions = BTreeMap::new();
        for session in &self.sessions {
            session.check()?;
            unique(&session.id)?;
            if !self.workers.is_empty() && !workers.contains_key(session.worker_id.as_str()) {
                return Err(rule(format!(
                    "Session {} references an unknown Worker",
                    session.id
                )));
            }
            sessions.insert(session.id.as_str(), session);
        }
        let mut tasks = BTreeSet::new();
        if let Some(project) = &self.project {
            project.check()?;
            unique(&project.id)?;
            for knowledge in &project.knowledge {
                unique(&knowledge.id)?;
            }
            for task in &project.tasks {
                unique(&task.id)?;
                tasks.insert(task.id.as_str());
            }
        }
        for ticket in &self.tickets {
            ticket.check()?;
            unique(&ticket.id)?;
            for job in &ticket.jobs {
                unique(&job.id)?;
                for run in &job.runs {
                    unique(&run.id)?;
                }
            }
            if self.project.is_some() && !tasks.contains(ticket.task_id.as_str()) {
                return Err(rule(format!(
                    "Ticket {} references a Task outside this Project",
                    ticket.id
                )));
            }
            if !workers.is_empty() && !workers.contains_key(ticket.worker_id.as_str()) {
                return Err(rule("Ticket references an unknown Worker"));
            }
            if !sessions.is_empty() && !sessions.contains_key(ticket.session_id.as_str()) {
                return Err(rule("Ticket references an unknown Session"));
            }
            if let Some(session) = sessions.get(ticket.session_id.as_str())
                && session.worker_id != ticket.worker_id
            {
                return Err(rule(format!(
                    "Ticket {} session belongs to another Worker",
                    ticket.id
                )));
            }
        }
        let depends: BTreeMap<_, _> = self
            .tickets
            .iter()
            .map(|ticket| {
                let mut edges = vec![];
                if !matches!(ticket.status.as_str(), "completed" | "canceled") {
                    edges.extend(ticket.depends.iter().map(String::as_str));
                    for job in ticket
                        .jobs
                        .iter()
                        .filter(|job| !matches!(job.status.as_str(), "completed" | "canceled"))
                    {
                        for run in &job.runs {
                            if let Some(wait) = &run.wait
                                && wait.resolved.is_none()
                                && let Some(target) =
                                    wait.target.as_deref().filter(|id| is_id_of("ticket", id))
                            {
                                edges.push(target);
                            }
                        }
                    }
                }
                (ticket.id.as_str(), edges)
            })
            .collect();
        for ticket in &self.tickets {
            let mut queue: Vec<&str> = depends[&ticket.id.as_str()].clone();
            let mut seen = BTreeSet::new();
            while let Some(id) = queue.pop() {
                if id == ticket.id {
                    return Err(rule("Ticket dependencies must be acyclic"));
                }
                if seen.insert(id)
                    && let Some(next) = depends.get(id)
                {
                    queue.extend(next.iter().copied());
                }
            }
        }
        Ok(())
    }
    /// Combines a Task file and Ticket files (or any documents sharing the root schema).
    pub fn merge(documents: impl IntoIterator<Item = Document>) -> Result<Self> {
        let mut merged = Self::new();
        for document in documents {
            if document.protocol != PROTOCOL {
                return Err(fail(format!("protocol must be {PROTOCOL}")));
            }
            for worker in document.workers {
                match merged.workers.iter().find(|w| w.id == worker.id) {
                    Some(existing) if *existing != worker => {
                        return Err(fail(format!("Worker {} differs between files", worker.id)));
                    }
                    Some(_) => {}
                    None => merged.workers.push(worker),
                }
            }
            for session in document.sessions {
                match merged.sessions.iter().find(|s| s.id == session.id) {
                    Some(existing) if *existing != session => {
                        return Err(fail(format!(
                            "Session {} differs between files",
                            session.id
                        )));
                    }
                    Some(_) => {}
                    None => merged.sessions.push(session),
                }
            }
            if let Some(project) = document.project {
                match &merged.project {
                    Some(existing) if *existing != project => {
                        return Err(fail("Only one Project may be merged"));
                    }
                    _ => merged.project = Some(project),
                }
            }
            merged.tickets.extend(document.tickets);
        }
        merged.validate()?;
        Ok(merged)
    }
    /// Splits into a Task file (`project.json`) and one file per Ticket (`tickets/<id>.json`).
    pub fn split(&self) -> Vec<(String, Document)> {
        let mut files = vec![];
        if self.project.is_some()
            || !self.workers.is_empty()
            || !self.sessions.is_empty()
            || self.tickets.is_empty()
        {
            files.push((
                "project.json".to_owned(),
                Document {
                    protocol: self.protocol.clone(),
                    workers: self.workers.clone(),
                    sessions: self.sessions.clone(),
                    project: self.project.clone(),
                    tickets: vec![],
                },
            ));
        }
        for ticket in &self.tickets {
            files.push((
                format!("tickets/{}.json", ticket.id),
                Document {
                    protocol: self.protocol.clone(),
                    tickets: vec![ticket.clone()],
                    ..Document::default()
                },
            ));
        }
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_follow_the_ttj_1_1_0_pattern() {
        for id in [
            "pj-a1b2c3d4-1",
            "tsk-e5f6a7-12",
            "tkt-C9d0E1f2-3",
            "job-1a2b3c4d-10",
            "run-5e6f7a8b-1",
            "wkr-7c8d9e0f-2",
            "ssn-2b3c4d-1",
        ] {
            assert!(is_id(id), "{id}");
        }
        for id in [
            "pj-a1b2c3d4",
            "pj-a1b2c3d4-0",
            "pj-a1b2c3d4-01",
            "pj-a1b2c3d-1",
            "pj-a1b2c3d4e-1",
            "pj-a1b2-c3d4-1",
            "prj-a1b2c3d4-1",
            "tsk-12abcd00",
            "550e8400-e29b-41d4-a716-446655440000",
        ] {
            assert!(!is_id(id), "{id}");
        }
        assert!(is_id_of("ticket", "tkt-abcdef-1"));
        assert!(!is_id_of("job", "tkt-abcdef-1"));
        assert_eq!(sequence("job-1a2b3c4d-12"), Some(("job-1a2b3c4d", 12)));
    }

    fn combined() -> Value {
        json!({
            "protocol": "TTJ/1.1.0",
            "workers": [{"id": "wkr-7c8d9e0f-1", "name": "Alpha"}],
            "sessions": [{"id": "ssn-2b3c4d5e-1", "worker_id": "wkr-7c8d9e0f-1",
                "started": "2026-01-01T01:00:00Z"}],
            "project": {"id": "pj-a1b2c3d4-1", "name": "勉強会の運営", "tasks": [{
                "id": "tsk-e5f6a7b8-1", "objective": "勉強会の案内を用意する",
                "criteria": ["日時・会場・申込方法が正しい案内文が確定している"],
                "handoff": {"current_ticket_id": "tkt-c9d0e1f2-1", "notes": ["draft"]}}]},
            "tickets": [{
                "id": "tkt-c9d0e1f2-1", "task_id": "tsk-e5f6a7b8-1",
                "worker_id": "wkr-7c8d9e0f-1", "session_id": "ssn-2b3c4d5e-1",
                "objective": "案内文の初稿を作成し確認準備を整える",
                "exclusions": ["参加者への案内メールの本送信は行わない"],
                "refs": [{"ticket_id": "tkt-c9d0e1f2-9", "kind": "related"}],
                "jobs": [{"id": "job-1a2b3c4d-1", "objective": "必要事項を揃えた案内文を作る",
                    "criteria": ["ドラフトが保存されている"], "status": "doing",
                    "runs": [{"id": "run-5e6f7a8b-1", "started": "2026-01-01T01:00:00Z",
                        "wait": {"target": "external", "since": "2026-01-01T01:04:30Z"}}]},
                    {"id": "job-1a2b3c4d-2", "objective": "点検する", "criteria": ["一致している"]}]
            }]
        })
    }

    #[test]
    fn documents_round_trip_and_omit_empty_collections() {
        let document = Document::parse(&combined()).unwrap();
        assert_eq!(document.tickets[0].status, "todo");
        assert_eq!(document.project.as_ref().unwrap().tasks[0].status, "todo");
        assert!(document.project.as_ref().unwrap().knowledge.is_empty());
        let value = document.to_value();
        assert!(value["project"].get("knowledge").is_none());
        assert!(value["tickets"][0]["jobs"][1].get("runs").is_none());
        assert!(value["tickets"][0].get("depends").is_none());
        assert_eq!(Document::parse(&value).unwrap(), document);
        let keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        for key in keys {
            assert!(
                ["protocol", "workers", "sessions", "project", "tickets"].contains(&key.as_str())
            );
        }
    }

    #[test]
    fn split_and_merge_preserve_the_single_root_schema() {
        let document = Document::parse(&combined()).unwrap();
        let files = document.split();
        assert_eq!(files[0].0, "project.json");
        assert!(files[0].1.tickets.is_empty());
        assert_eq!(files[1].0, "tickets/tkt-c9d0e1f2-1.json");
        let ticket_file = files[1].1.to_value();
        assert!(ticket_file.get("project").is_none());
        assert!(ticket_file.get("workers").is_none());
        assert_eq!(ticket_file["tickets"][0]["task_id"], "tsk-e5f6a7b8-1");
        let parsed = files
            .iter()
            .map(|(_, file)| Document::parse(&file.to_value()).unwrap());
        assert_eq!(Document::merge(parsed).unwrap(), document);
        let mut duplicate = files[1].1.clone();
        duplicate.tickets.extend(files[1].1.tickets.clone());
        assert!(Document::merge([duplicate]).is_err());
    }

    #[test]
    fn retired_and_misplaced_fields_are_rejected() {
        let mut cases = vec![];
        let mut projects = combined();
        projects["projects"] = json!([projects["project"].take()]);
        projects.as_object_mut().unwrap().remove("project");
        cases.push(projects);
        let mut old = combined();
        old["protocol"] = json!("TTJ/1.0.0");
        cases.push(old);
        for (path, key, value) in [
            ("/tickets/0", "assignee", json!(["a"])),
            ("/tickets/0", "handoffs", json!(["a"])),
            ("/tickets/0", "claims", json!([])),
            ("/tickets/0/jobs/0", "exclusions", json!(["a"])),
            ("/tickets/0/jobs/0/runs/0", "job", json!("job-1a2b3c4d-1")),
            ("/tickets/0/jobs/0/runs/0", "activity", json!({})),
            ("/tickets/0/jobs/0/runs/0", "generation", json!(1)),
            ("/project/tasks/0", "tickets", json!([])),
            ("/project/tasks/0", "project", json!("pj-a1b2c3d4-1")),
        ] {
            let mut document = combined();
            document.pointer_mut(path).unwrap()[key] = value;
            cases.push(document);
        }
        for value in cases {
            assert!(Document::parse(&value).is_err());
        }
    }
}
