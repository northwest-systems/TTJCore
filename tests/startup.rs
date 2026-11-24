use serde_json::Value;
use std::{
    path::PathBuf,
    process::{Command, Output},
    time::Duration,
};

struct Profile(PathBuf);
impl Profile {
    fn command(&self, args: &[&str]) -> Output {
        eprintln!("Starting CLI {}", args[0]);
        let output = Command::new(env!("CARGO_BIN_EXE_ttjcore"))
            .arg("--home")
            .arg(&self.0)
            .args(args)
            .env_remove("TTJCORE_DATABASE_URL")
            .output()
            .unwrap();
        eprintln!("CLI {} exited", args[0]);
        output
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = self.command(&["stop"]);
        for _ in 0..100 {
            if std::fs::remove_dir_all(&self.0).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn ensure_initializes_once_and_survives_the_launching_cli() {
    #[cfg(unix)]
    let temp_root = PathBuf::from("/tmp");
    #[cfg(windows)]
    let temp_root = std::env::temp_dir();
    let profile = Profile(temp_root.join(format!("ttjcore-startup-{}", uuid::Uuid::new_v4())));
    std::thread::scope(|scope| {
        let first = scope.spawn(|| profile.command(&["ensure"]));
        let second = scope.spawn(|| profile.command(&["ensure"]));
        for result in [first.join().unwrap(), second.join().unwrap()] {
            assert!(result.status.success(), "ensure failed");
            assert_eq!(
                serde_json::from_slice::<Value>(&result.stdout).unwrap()["ready"],
                true
            );
        }
    });
    let created = profile.command(&[
        "call",
        "project.create",
        "--params",
        r#"{"actor":["operator"],"reason":"Startup contract","record":{"name":"Startup"}}"#,
    ]);
    assert!(created.status.success());
    let list = profile.command(&["call", "project.list"]);
    assert!(list.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&list.stdout).unwrap()["result"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(profile.command(&["stop"]).status.success());
}

#[test]
fn cli_imports_and_exports_combined_and_split_ttj_files() {
    #[cfg(unix)]
    let temp_root = PathBuf::from("/tmp");
    #[cfg(windows)]
    let temp_root = std::env::temp_dir();
    let first = Profile(temp_root.join(format!("ttj-files-a-{}", uuid::Uuid::new_v4())));
    let second = Profile(temp_root.join(format!("ttj-files-b-{}", uuid::Uuid::new_v4())));
    assert!(first.command(&["ensure"]).status.success());
    assert!(second.command(&["ensure"]).status.success());
    let document = ttjcore::document::Document::parse(&serde_json::json!({
        "protocol":ttjcore::PROTOCOL,
        "workers":[{"id":"wkr-abcdef-1","name":"Operator"}],
        "sessions":[{"id":"ssn-abcdef-1","worker_id":"wkr-abcdef-1"}],
        "project":{"id":"pj-abcdef-1","name":"CLI files","tasks":[{"id":"tsk-abcdef-1","objective":"Deliver","criteria":["Verified"],"handoff":{"notes":["Check publication"]}}]},
        "tickets":[{"id":"tkt-abcdef-1","task_id":"tsk-abcdef-1","worker_id":"wkr-abcdef-1","session_id":"ssn-abcdef-1","objective":"Prepare","exclusions":["Publish"],"jobs":[{"id":"job-abcdef-1","objective":"Draft","criteria":["Checked"]}]}]
    })).unwrap();
    let source = first.0.join("source.json");
    document.write(&source).unwrap();
    let imported = first.command(&[
        "import",
        "--file",
        source.to_str().unwrap(),
        "--actor",
        r#"["owner"]"#,
        "--reason",
        "Verified import",
    ]);
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let combined = first.0.join("combined.json");
    assert!(
        first
            .command(&[
                "export",
                "pj-abcdef-1",
                "--file",
                combined.to_str().unwrap()
            ])
            .status
            .success()
    );
    assert_eq!(
        ttjcore::document::Document::read(&combined).unwrap(),
        document
    );
    let split = first.0.join("split");
    assert!(
        first
            .command(&[
                "export",
                "pj-abcdef-1",
                "--directory",
                split.to_str().unwrap()
            ])
            .status
            .success()
    );
    let project_file = split.join("project.json");
    let ticket_file = split.join("tickets/tkt-abcdef-1.json");
    assert!(
        second
            .command(&[
                "import",
                "--file",
                project_file.to_str().unwrap(),
                "--file",
                ticket_file.to_str().unwrap(),
                "--actor",
                r#"["owner"]"#,
                "--reason",
                "Verified split import"
            ])
            .status
            .success()
    );
    let exported = second.0.join("exported.json");
    assert!(
        second
            .command(&[
                "export",
                "pj-abcdef-1",
                "--file",
                exported.to_str().unwrap()
            ])
            .status
            .success()
    );
    assert_eq!(
        ttjcore::document::Document::read(exported).unwrap(),
        document
    );
}
