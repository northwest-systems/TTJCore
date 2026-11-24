#[cfg(not(windows))]
use interprocess::local_socket::GenericFilePath;
#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::tokio::{Stream, prelude::*};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use ttjcore::{PROTOCOL, ipc, store::Store};

#[tokio::test]
async fn ipc_delivers_committed_changes_and_replays_without_duplicate_events() {
    // macOS temporary directories can exceed the Unix socket path limit.
    #[cfg(unix)]
    let temp_root = std::path::PathBuf::from("/tmp");
    #[cfg(windows)]
    let temp_root = std::env::temp_dir();
    let home = temp_root.join(format!("ttjcore-ipc-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(Store::connect("sqlite::memory:", true).await.unwrap());
    let server = tokio::spawn(ipc::serve(Arc::clone(&store), home.clone()));
    for _ in 0..100 {
        if home.join("connection.json").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let info: Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("connection.json")).unwrap())
            .unwrap();
    #[cfg(windows)]
    let name = info["endpoint"]
        .as_str()
        .unwrap()
        .to_ns_name::<GenericNamespaced>()
        .unwrap();
    #[cfg(not(windows))]
    let name = info["endpoint"]
        .as_str()
        .unwrap()
        .to_fs_name::<GenericFilePath>()
        .unwrap();
    let stream = Stream::connect(name).await.unwrap();
    let mut writer = &stream;
    let mut reader = BufReader::new(&stream);
    let subscription = json!({"jsonrpc":"2.0","protocol":PROTOCOL,"auth":info["token"],"id":"subscribe",
        "method":"events.subscribe","params":{"cursor":0,"project":"pj-abcdef-1"}});
    writer
        .write_all(format!("{subscription}\n").as_bytes())
        .await
        .unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap()["result"]["cursor"],
        0
    );
    let batch = ipc::call(
        &home,
        json!([null,{"jsonrpc":"2.0","protocol":PROTOCOL,
        "id":"batch-read","method":"project.list"}]),
    )
    .await
    .unwrap();
    assert_eq!(batch[0]["error"]["code"], -32600);
    assert_eq!(batch[1]["id"], "batch-read");
    let wait_home = home.clone();
    let waiting = tokio::spawn(async move {
        ipc::call(
            &wait_home,
            json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":2,
            "method":"events.wait","params":{"cursor":0,"project":"pj-abcdef-1","timeout_ms":1000}}),
        )
        .await
        .unwrap()
    });
    let create = json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":1,"method":"project.create",
        "params":{"actor":["operator"],"reason":"IPC contract","operation":"create-once",
            "record":{"id":"pj-abcdef-1","name":"Demo"}}});
    let first = ipc::call(&home, create.clone()).await.unwrap();
    assert!(first.get("error").is_none());
    line.clear();
    tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let notification: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(notification["method"], "ttj.changed");
    assert_eq!(
        notification["params"]["events"][0]["project"],
        "pj-abcdef-1"
    );
    let waited = waiting.await.unwrap();
    assert_eq!(waited["result"]["events"][0]["target"], "pj-abcdef-1");
    assert_eq!(ipc::call(&home, create).await.unwrap(), first);
    let cursor = notification["params"]["cursor"].clone();
    let unchanged = ipc::call(
        &home,
        json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":3,
        "method":"events.list","params":{"cursor":cursor}}),
    )
    .await
    .unwrap();
    assert!(unchanged["result"]["events"].as_array().unwrap().is_empty());
    drop(reader);
    drop(stream);
    let stopped = ipc::call(
        &home,
        json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":4,"method":"core.stop","params":{}}),
    )
    .await
    .unwrap();
    assert_eq!(stopped["result"]["stopping"], true);
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let store = Arc::try_unwrap(store).ok();
    if let Some(store) = store {
        store.db.close().await.unwrap();
    }
    assert_eq!(home.parent(), Some(temp_root.as_path()));
    std::fs::remove_dir_all(home).unwrap();
}
