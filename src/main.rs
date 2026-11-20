use serde_json::{Value, json};
use std::sync::Arc;
use ttjcore::{PROTOCOL, ipc, store::Store};
const HELP: &str = "TTJCore: TTJ 1.0.0 local CLI and shared IPC service
Usage:
  ttjcore [--home PROFILE] ensure
  ttjcore init
  ttjcore serve
  ttjcore stop
  ttjcore methods
  ttjcore call METHOD [--params JSON | --file FILE]
  ttjcore rpc
Environment:
  TTJCORE_DATABASE_URL selects SQLite (default) or PostgreSQL for ensure/init/serve.
  TTJCORE_HOME selects the local Core profile for every command.
serve owns the DB; call and rpc connect to the running Core without TCP.
rpc bridges JSON-RPC requests, responses and ttj.changed notifications over stdio.
Mutations need actor; methods lists required fields. Use operation for retries and revision for updates.
";
#[tokio::main]
async fn main() {
    if let Err(message) = run().await {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
async fn run() -> std::result::Result<(), String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let home = if args.first().is_some_and(|arg| arg == "--home") {
        if args.len() < 3 {
            return Err("--home requires a profile directory and command".into());
        }
        let home = std::path::PathBuf::from(args.remove(1));
        args.remove(0);
        home
    } else {
        ipc::home().map_err(|_| "Cannot locate Core profile")?
    };
    if args.first().is_some_and(|arg| arg == "ensure") {
        ttjcore::startup::ensure(&home)
            .await
            .map_err(|error| error.to_string())?;
        println!("{}", json!({"protocol":PROTOCOL,"ready":true}));
        return Ok(());
    }
    let command = args.first().map(String::as_str).unwrap_or("--help");
    if matches!(command, "--help" | "-h" | "help") {
        print!("{HELP}");
        return Ok(());
    }
    if matches!(command, "--version" | "-V") {
        println!("ttjcore {}", env!("CARGO_PKG_VERSION"));
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
        ipc::prepare_home(&home).map_err(|_| "Cannot prepare Core profile")?;
        let url = ttjcore::startup::database_url(&home);
        let store = Store::connect(&url, command == "init")
            .await
            .map_err(|e| e.message)?;
        if command == "init" {
            println!("{}", json!({"protocol":PROTOCOL,"initialized":true}));
            return Ok(());
        }
        return ipc::serve(Arc::new(store), home.clone())
            .await
            .map_err(|_| {
                "Cannot start local Core IPC; check the profile and existing server".into()
            });
    }
    if command == "stop" {
        let response = ipc::call(
            &home,
            json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":1,"method":"core.stop","params":{}}),
        )
        .await
        .map_err(|_| "Cannot stop Core; check the selected profile")?;
        if response.get("error").is_some() {
            return Err("Core refused the stop request".into());
        }
        println!("{response}");
        return Ok(());
    }
    if command == "rpc" {
        return ipc::bridge(&home).await.map_err(|e| e.to_string());
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
    let response = ipc::call(&home.clone(), request)
        .await
        .map_err(|e| e.to_string())?;
    println!("{response}");
    if response.get("error").is_some() {
        std::process::exit(2);
    }
    Ok(())
}
