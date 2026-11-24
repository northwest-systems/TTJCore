use crate::{PROTOCOL, ipc};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::{self, IsTerminal},
    path::Path,
};
use tokio::io::AsyncWriteExt;

fn single_line(text: &str, limit: usize) -> String {
    let mut output = String::new();
    for character in text.chars() {
        let character = if character <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&character) {
            ' '
        } else {
            character
        };
        if output.len() + character.len_utf8() > limit {
            break;
        }
        output.push(character);
    }
    output
}

fn encode(state: &str, title: &str, message: &str, percent: Option<u64>) -> String {
    let mut report = format!(
        "\x1b]7501;state={state}:app=ttjcore:title={}:msg={}",
        STANDARD.encode(single_line(title, 192)),
        STANDARD.encode(single_line(message, 2048))
    );
    if let Some(percent) = percent {
        report.push_str(&format!(":progress={percent}"));
    }
    report.push_str("\x1b\\");
    report
}

pub fn osc_report(snapshot: &Value) -> String {
    let ticket = &snapshot["ticket"]["record"];
    let task = &snapshot["task"]["record"];
    let job = &snapshot["job"]["record"];
    let run = &snapshot["run"];
    let status = ticket["status"].as_str().unwrap_or("todo");
    let label = if status == "completed" || status == "canceled" {
        status
    } else if task["status"] == "paused"
        || ticket["status"] == "paused"
        || job["status"] == "paused"
    {
        "paused"
    } else if run["constraints"]["stop_requested"] == true {
        "stopping"
    } else if ticket["status"] == "blocked" || job["status"] == "blocked" {
        "blocked"
    } else if ticket["status"] == "waiting" || job["status"] == "waiting" {
        "waiting"
    } else {
        status
    };
    let state = match label {
        "completed" => "done",
        "waiting" => "working",
        "doing" if run["active"] == true => "working",
        _ => "idle",
    };
    let title = job["objective"]
        .as_str()
        .or_else(|| ticket["objective"].as_str())
        .unwrap_or("TTJCore");
    let detail = run["progress"]["message"].as_str().unwrap_or(title);
    let percent = if state == "working" && run["active"] == true {
        run["progress"]["percent"]
            .as_u64()
            .filter(|percent| *percent <= 100)
    } else {
        None
    };
    encode(state, title, &format!("{label}: {detail}"), percent)
}

async fn call(home: &Path, method: &str, params: Value) -> io::Result<Value> {
    let response = ipc::call(
        home,
        json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":"watch","method":method,"params":params}),
    )
    .await?;
    if let Some(error) = response.get("error") {
        return Err(io::Error::other(
            error["message"]
                .as_str()
                .unwrap_or("Core rejected the watch request")
                .to_owned(),
        ));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| io::Error::other("Missing Core response"))
}

async fn output(text: &str) -> io::Result<()> {
    let mut stdout = tokio::io::stdout();
    stdout.write_all(text.as_bytes()).await?;
    stdout.flush().await
}

async fn watch_loop(home: &Path, ticket: &str, osc: bool) -> io::Result<()> {
    let mut snapshot = call(home, "ticket.snapshot", json!({"id":ticket})).await?;
    let mut previous = String::new();
    loop {
        let report = if osc {
            osc_report(&snapshot)
        } else {
            format!("{snapshot}\n")
        };
        if previous != report {
            output(&report).await?;
            previous = report;
        }
        if crate::engine::terminal(&snapshot["ticket"]["record"]) {
            return Ok(());
        }
        let mut cursor = snapshot["cursor"].clone();
        loop {
            let params = json!({"project":snapshot["project_id"],"cursor":cursor});
            let events = tokio::select! {
                result = call(home, "events.wait", params) => result?,
                result = tokio::signal::ctrl_c() => {
                    result?;
                    if osc { output(&encode("idle", "TTJCore", "Monitoring stopped", None)).await?; }
                    return Ok(());
                }
            };
            cursor = events["cursor"].clone();
            if events["events"].as_array().is_none_or(Vec::is_empty) {
                continue;
            }
            snapshot = call(home, "ticket.snapshot", json!({"id":ticket})).await?;
            break;
        }
    }
}

pub async fn watch(home: &Path, ticket: &str, osc: bool) -> io::Result<()> {
    if osc && !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "--osc7501 requires a terminal on stdout; omit it for JSON output",
        ));
    }
    let result = watch_loop(home, ticket, osc).await;
    if let Err(error) = &result
        && osc
    {
        let _ = output(&encode("error", "TTJCore", &error.to_string(), None)).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Value {
        json!({"task":{"record":{"status":"doing"}},"ticket":{"record":{"status":"doing","objective":"Ticket"}},
            "job":{"record":{"status":"doing","objective":"日本語のJob"}},
            "run":{"active":true,"progress":{"percent":37,"message":"検証中"}}})
    }

    #[test]
    fn percentages_do_not_complete_work_and_waits_do_not_invent_user_attention() {
        let mut value = snapshot();
        value["run"]["progress"]["percent"] = json!(100);
        let report = osc_report(&value);
        assert!(report.contains("state=working") && report.contains("progress=100"));
        assert!(report.contains(&format!("msg={}", STANDARD.encode("doing: 検証中"))));
        value["ticket"]["record"]["status"] = json!("waiting");
        assert!(osc_report(&value).contains("state=working"));
        assert!(!osc_report(&value).contains("kind="));
        value["ticket"]["record"]["status"] = json!("completed");
        assert!(osc_report(&value).contains("state=done"));
        assert!(!osc_report(&value).contains("progress="));
    }

    #[test]
    fn paused_stop_requested_and_closed_runs_do_not_claim_execution() {
        for change in ["paused", "stopping", "closed"] {
            let mut value = snapshot();
            match change {
                "paused" => value["task"]["record"]["status"] = json!("paused"),
                "stopping" => value["run"]["constraints"]["stop_requested"] = json!(true),
                _ => value["run"]["active"] = json!(false),
            }
            let report = osc_report(&value);
            assert!(report.contains("state=idle"));
            assert!(!report.contains("progress="));
        }
    }

    #[test]
    fn unicode_and_control_characters_are_encoded_within_protocol_limits() {
        let mut value = snapshot();
        value["job"]["record"]["objective"] = json!("\x1b\n日本語".repeat(100));
        value["run"]["progress"]["message"] = json!("\x07\t検証中".repeat(1000));
        let report = osc_report(&value);
        assert!(report.len() <= 4096);
        assert!(report.starts_with("\x1b]7501;") && report.ends_with("\x1b\\"));
        for (field, limit) in [("title", 192), ("msg", 2048)] {
            let encoded = report
                .split(':')
                .find_map(|pair| pair.strip_prefix(&format!("{field}=")))
                .unwrap();
            let decoded = STANDARD.decode(encoded).unwrap();
            assert!(decoded.len() <= limit);
            let text = String::from_utf8(decoded).unwrap();
            assert!(
                !text
                    .chars()
                    .any(|c| c <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&c))
            );
        }
    }
}
