use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "command", content = "params", rename_all = "snake_case")]
#[ts(export)]
pub enum Request {
    AppInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "command", content = "result", rename_all = "snake_case")]
#[ts(export)]
pub enum Response {
    AppInfo(AppInfo),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
}
