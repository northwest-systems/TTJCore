use crate::{PROTOCOL, store::Store};
use serde_json::{Value, json};

fn failure(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":id,"error":{"code":code,"message":message}})
}
pub async fn dispatch(store: &Store, wire: &str) -> Option<Value> {
    let request: Value = match serde_json::from_str(wire) {
        Ok(v) => v,
        Err(_) => return Some(failure(Value::Null, -32700, "Parse error")),
    };
    let requests = if let Some(batch) = request.as_array() {
        if batch.is_empty() {
            return Some(failure(Value::Null, -32600, "Invalid Request"));
        }
        batch.clone()
    } else {
        vec![request.clone()]
    };
    let mut responses = vec![];
    for request in requests {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let valid_id = id.is_null() || id.is_string() || id.is_number();
        if !request.is_object()
            || request["jsonrpc"] != "2.0"
            || !request["method"].is_string()
            || !valid_id
        {
            responses.push(failure(Value::Null, -32600, "Invalid Request"));
            continue;
        }
        let notification = request.get("id").is_none();
        let response = if request["protocol"] != PROTOCOL {
            failure(id, -32602, "protocol must be TTJ/1.0.0")
        } else {
            match store
                .call(
                    request["method"].as_str().unwrap(),
                    request.get("params").cloned().unwrap_or(json!({})),
                )
                .await
            {
                Ok(result) => json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":id,"result":result}),
                Err(e) => failure(id, e.code, &e.message),
            }
        };
        if !notification {
            responses.push(response);
        }
    }
    if request.is_array() {
        if responses.is_empty() {
            None
        } else {
            Some(Value::Array(responses))
        }
    } else {
        responses.pop()
    }
}
