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
