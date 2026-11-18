use serde_json::{Value, json};
use std::sync::Arc;
use ttjcore::{PROTOCOL, ipc, store::Store};
const HELP: &str = "TTJCore: TTJ 1.0.0 local CLI and shared IPC service
Usage:
  ttjcore init
  ttjcore serve
  ttjcore methods
  ttjcore call METHOD [--params JSON | --file FILE]
  ttjcore rpc
Environment:
  TTJCORE_DATABASE_URL selects SQLite (default) or PostgreSQL for init/serve.
  TTJCORE_HOME selects the local Core profile for every command.
serve owns the DB; call and rpc connect to the running Core without TCP.
rpc bridges JSON-RPC requests, responses and ttj.changed notifications over stdio.
Mutations need actor and reason. Use operation for retries and revision for updates.
";
#[tokio::main]
async fn main() {
    if let Err(message) = run().await {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
async fn run() -> std::result::Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("--help");
    if matches!(command, "--help" | "-h" | "help") {
        print!("{HELP}");
        return Ok(());
    }
    if command == "methods" {
        println!(
            "{}",
            json!({"protocol":PROTOCOL,"result":ttjcore::store::discover()})
        );
        return Ok(());
    }
    if matches!(command, "init" | "serve") {
        let url = std::env::var("TTJCORE_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://ttjcore.sqlite?mode=rwc".into());
        let store = Store::connect(&url, command == "init")
            .await
            .map_err(|e| e.message)?;
        if command == "init" {
            println!("{}", json!({"protocol":PROTOCOL,"initialized":true}));
            return Ok(());
        }
        return ipc::serve(
            Arc::new(store),
            ipc::home().map_err(|_| "Cannot locate Core profile")?,
        )
        .await
        .map_err(|_| "Cannot start local Core IPC; check the profile and existing server".into());
    }
    if command == "rpc" {
        return ipc::bridge(&ipc::home().map_err(|_| "Cannot locate Core profile")?)
            .await
            .map_err(|e| e.to_string());
    }
    if command != "call" {
        return Err("Unknown command. Use ttjcore --help".into());
    }
    let method = args.get(1).ok_or("call requires a method")?;
    let params = match args.get(2).map(String::as_str) {
        None => json!({}),
        Some("--params") if args.len() == 4 => {
            serde_json::from_str::<Value>(&args[3]).map_err(|_| "Invalid params JSON")?
        }
        Some("--file") if args.len() == 4 => {
            let text = std::fs::read_to_string(&args[3]).map_err(|_| "Cannot read params file")?;
            serde_json::from_str(&text).map_err(|_| "Invalid params JSON")?
        }
        _ => return Err("Use --params JSON or --file FILE".into()),
    };
    let request =
        json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":1,"method":method,"params":params});
    let response = ipc::call(
        &ipc::home().map_err(|_| "Cannot locate Core profile")?,
        request,
    )
    .await
    .map_err(|e| e.to_string())?;
    println!("{response}");
    if response.get("error").is_some() {
        std::process::exit(2);
    }
    Ok(())
}
