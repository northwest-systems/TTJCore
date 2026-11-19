pub mod engine;
pub mod entity;
pub mod ipc;
pub mod rpc;
pub mod store;

use serde::Serialize;
pub const PROTOCOL: &str = "TTJ/1.0.0";

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn invalid(message: impl Into<String>) -> Error {
    Error {
        code: -32602,
        message: message.into(),
    }
}
pub fn rule(message: impl Into<String>) -> Error {
    Error {
        code: -32010,
        message: message.into(),
    }
}
pub fn conflict() -> Error {
    Error {
        code: -32009,
        message: "Revision or operation conflicts with an existing change".into(),
    }
}
impl From<sea_orm::DbErr> for Error {
    fn from(_: sea_orm::DbErr) -> Self {
        Self {
            code: -32000,
            message:
                "Database operation failed; check connectivity and retry with the same operation id"
                    .into(),
        }
    }
}

pub mod startup;
