#[cfg(windows)]
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{PROTOCOL, rpc, store::Store};
#[cfg(not(windows))]
use interprocess::local_socket::GenericFilePath;
#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{
    ListenerOptions, Name,
    tokio::{Stream, prelude::*},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Clone, Serialize, Deserialize)]
struct ConnectionInfo {
    endpoint: String,
    token: String,
}

pub fn home() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("TTJCORE_HOME") {
        return Ok(path.into());
    }
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA");
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME");
    let base = base.ok_or_else(|| io::Error::other("Set TTJCORE_HOME"))?;
    Ok(PathBuf::from(base).join(".ttjcore"))
}
fn address(endpoint: &str) -> io::Result<Name<'_>> {
    #[cfg(windows)]
    {
        endpoint.to_ns_name::<GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        endpoint.to_fs_name::<GenericFilePath>()
    }
}
fn endpoint(home: &Path) -> String {
    #[cfg(windows)]
    {
        let mut hash = DefaultHasher::new();
        home.hash(&mut hash);
        format!("ttjcore-{:016x}", hash.finish())
    }
    #[cfg(not(windows))]
    {
        home.join("core.sock").to_string_lossy().into_owned()
    }
}
pub fn prepare_home(home: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(home)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(home)?.permissions().mode() & 0o022 != 0 {
            return Err(io::Error::other(
                "Core profile must not be writable by other users",
            ));
        }
    }
    Ok(())
}
fn publish(home: &Path, info: &ConnectionInfo) -> io::Result<()> {
    use std::io::Write;
    let temporary = home.join(format!("connection-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(serde_json::to_string(info)?.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(temporary, home.join("connection.json"))?;
    Ok(())
}
fn read_info(home: &Path) -> io::Result<ConnectionInfo> {
    let text = std::fs::read_to_string(home.join("connection.json"))
        .map_err(|_| io::Error::other("Core is not running; start ttjcore serve"))?;
    serde_json::from_str(&text).map_err(|_| io::Error::other("Invalid Core connection profile"))
}
fn authenticate(request: &mut Value, token: &str) {
    if let Some(batch) = request.as_array_mut() {
        for item in batch {
            if item.is_object() {
                item["auth"] = json!(token);
            }
        }
    } else if request.is_object() {
        request["auth"] = json!(token);
    }
}
fn authorized(request: &Value, token: &str) -> bool {
    if let Some(batch) = request.as_array() {
        batch
            .iter()
            .all(|item| !item.is_object() || item["auth"] == token)
    } else {
        !request.is_object() || request["auth"] == token
    }
}
async fn send(writer: &mut &Stream, value: &Value) -> io::Result<()> {
    writer.write_all(format!("{value}\n").as_bytes()).await?;
    writer.flush().await
}
fn failure(id: Value, message: &str) -> Value {
    json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":id,"error":{"code":-32001,"message":message}})
}
pub async fn serve(store: Arc<Store>, home: PathBuf) -> io::Result<()> {
    prepare_home(&home)?;
    let info = ConnectionInfo {
        endpoint: endpoint(&home),
        token: uuid::Uuid::new_v4().to_string(),
    };
    #[cfg(unix)]
    if let Ok(metadata) = std::fs::symlink_metadata(&info.endpoint) {
        use std::os::unix::fs::FileTypeExt;
        if metadata.file_type().is_socket()
            && Stream::connect(address(&info.endpoint)?).await.is_err()
        {
            std::fs::remove_file(&info.endpoint)?;
        }
    }
    let listener = ListenerOptions::new()
        .name(address(&info.endpoint)?)
        .create_tokio()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&info.endpoint, std::fs::Permissions::from_mode(0o600))?;
    }
    publish(&home, &info)?;
    eprintln!("TTJCore local IPC is ready");
    let shutdown = Arc::new(tokio::sync::Notify::new());
    loop {
        tokio::select! {
            _=tokio::signal::ctrl_c()=>return Ok(()),
            _=shutdown.notified()=>return Ok(()),
            accepted=listener.accept()=>{
                let stream=accepted?;
                let store=Arc::clone(&store);let token=info.token.clone();let shutdown=Arc::clone(&shutdown);
                tokio::spawn(async move {
                    if handle(stream,store,&token,shutdown).await.is_err() {
                        // A disconnected client must not terminate the shared Core.
                    }
                });
            }
        }
    }
}
async fn handle(
    stream: Stream,
    store: Arc<Store>,
    token: &str,
    shutdown: Arc<tokio::sync::Notify>,
) -> io::Result<()> {
    let mut reader = BufReader::new(&stream).lines();
    let mut writer = &stream;
    let mut receiver = store.subscribe();
    let mut subscription: Option<(Option<String>, i64)> = None;
    loop {
        tokio::select! {
            line=reader.next_line()=>{
                let Some(line)=line? else {return Ok(());};
                let parsed=serde_json::from_str::<Value>(&line);
                if let Ok(request)=&parsed {
                    if !authorized(request,token) {
                        send(&mut writer,&failure(request["id"].clone(),"Local Core authentication failed")).await?;
                        continue;
                    }
                    if request["method"]=="events.subscribe" || request["method"]=="core.stop" {
                        if request["jsonrpc"]!="2.0" || request["protocol"]!=PROTOCOL
                            || request.get("id").is_none()
                            || !request["id"].is_string() && !request["id"].is_number() && !request["id"].is_null() {
                            send(&mut writer,&json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":null,
                                "error":{"code":-32600,"message":"A valid request id and protocol are required"}})).await?;
                            continue;
                        }
                        if request["method"]=="core.stop" {
                            send(&mut writer,&json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":request["id"],"result":{"stopping":true}})).await?;
                            shutdown.notify_one();return Ok(());
                        }
                        let mut params=request.get("params").cloned().unwrap_or(json!({}));
                        match store.call("events.list",params.clone()).await {
                            Ok(result)=>{
                                let cursor=result["cursor"].as_i64().unwrap_or(0);
                                subscription=Some((params["project"].as_str().map(str::to_owned),cursor));
                                params["cursor"]=json!(cursor);
                                send(&mut writer,&json!({"jsonrpc":"2.0","protocol":PROTOCOL,
                                    "id":request["id"],"result":result})).await?;
                            }
                            Err(error)=>send(&mut writer,&json!({"jsonrpc":"2.0","protocol":PROTOCOL,
                                "id":request["id"],"error":error})).await?,
                        }
                        continue;
                    }
                }
                if let Some(response)=rpc::dispatch(&store,&line).await {send(&mut writer,&response).await?;}
            }
            received=receiver.recv(), if subscription.is_some()=>{
                if matches!(received,Err(tokio::sync::broadcast::error::RecvError::Closed)) {return Ok(());}
                let (project,cursor)=subscription.as_ref().unwrap();
                let mut params=json!({"cursor":cursor});
                if let Some(project)=project {params["project"]=json!(project);}
                let result=store.call("events.list",params).await.map_err(|_|io::Error::other("Cannot read change history"))?;
                if let Some((_,cursor))=subscription.as_mut() {*cursor=result["cursor"].as_i64().unwrap_or(*cursor);}
                if !result["events"].as_array().is_none_or(Vec::is_empty) {
                    send(&mut writer,&json!({"jsonrpc":"2.0","protocol":PROTOCOL,
                        "method":"ttj.changed","params":result})).await?;
                }
            }
        }
    }
}
pub async fn call(home: &Path, mut request: Value) -> io::Result<Value> {
    let info = read_info(home)?;
    authenticate(&mut request, &info.token);
    let stream = Stream::connect(address(&info.endpoint)?)
        .await
        .map_err(|_| {
            io::Error::other("Cannot connect to Core; start ttjcore serve for this profile")
        })?;
    let mut writer = &stream;
    send(&mut writer, &request).await?;
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    let count = tokio::time::timeout(
        std::time::Duration::from_secs(35),
        reader.read_line(&mut line),
    )
    .await
    .map_err(|_| io::Error::other("Core response timed out"))??;
    if count == 0 {
        return Err(io::Error::other("Core disconnected"));
    }
    serde_json::from_str(&line).map_err(|_| io::Error::other("Invalid Core response"))
}
pub async fn bridge(home: &Path) -> io::Result<()> {
    let info = read_info(home)?;
    let stream = Stream::connect(address(&info.endpoint)?)
        .await
        .map_err(|_| io::Error::other("Cannot connect to Core; start ttjcore serve"))?;
    let mut input = BufReader::new(tokio::io::stdin()).lines();
    let mut reader = BufReader::new(&stream).lines();
    let mut writer = &stream;
    let mut output = tokio::io::stdout();
    let mut input_closed = false;
    let mut pending = 0usize;
    let mut subscribed = false;
    let mut subscription_id: Option<Value> = None;
    loop {
        tokio::select! {
            line=input.next_line(), if !input_closed=>{
                let Some(line)=line? else {
                    input_closed=true;
                    if pending==0 && !subscribed {return Ok(());}
                    continue;
                };
                if let Ok(mut request)=serde_json::from_str::<Value>(&line) {
                    let items=request.as_array().cloned().unwrap_or_else(||vec![request.clone()]);
                    let expects_response=items.is_empty() || items.iter().any(|r|
                        !r.is_object() || r["jsonrpc"]!="2.0" || !r["method"].is_string()
                        || r.get("id").is_some() || r["method"]=="events.subscribe");
                    pending+=usize::from(expects_response);
                    if !request.is_array() && request["method"]=="events.subscribe" {
                        subscription_id=request.get("id").cloned();
                    }
                    authenticate(&mut request,&info.token);send(&mut writer,&request).await?;
                } else {pending+=1;writer.write_all(format!("{line}\n").as_bytes()).await?;}
            }
            line=reader.next_line()=>{
                let Some(line)=line? else {return Ok(());};
                output.write_all(format!("{line}\n").as_bytes()).await?;output.flush().await?;
                if let Ok(response)=serde_json::from_str::<Value>(&line) {
                    if response.is_array() || response.get("id").is_some() {
                        pending=pending.saturating_sub(1);
                    }
                    if subscription_id.as_ref().is_some_and(|id|response.get("id")==Some(id))
                        && response.get("result").is_some() {
                        subscribed=true;
                    }
                }
                if input_closed && pending==0 && !subscribed {return Ok(());}
            }
        }
    }
}
