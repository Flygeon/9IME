//! IPC protocol between the TSF client (in the app process) and the 9IME
//! server process that owns librime and the candidate window.
//!
//! Wire format: u32 LE length prefix + UTF-8 JSON body.

use serde::{Deserialize, Serialize};

pub const PIPE_NAME: &str = r"\\.\pipe\9ime";

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CandidateMsg {
    pub text: String,
    pub comment: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MenuMsg {
    pub page_size: i32,
    pub page_no: i32,
    pub is_last_page: bool,
    pub highlighted: i32,
    pub candidates: Vec<CandidateMsg>,
    pub select_keys: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ContextMsg {
    pub composing: bool,
    pub preedit: String,
    pub cursor: i32,
    pub menu: MenuMsg,
    pub commit_text_preview: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct StatusMsg {
    pub schema_id: String,
    pub schema_name: String,
    pub ascii_mode: bool,
    pub composing: bool,
    pub disabled: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    Hello { pid: u32 },
    Focus { focused: bool },
    ProcessKey {
        keycode: u32,
        mask: u32,
        anchor_x: i32,
        anchor_y: i32,
    },
    SelectCandidate { index: usize },
    SelectCandidateOnCurrentPage { index: usize },
    ChangePage { backward: bool },
    SetOption { name: String, value: bool },
    ToggleSimpTrad,
    SelectSchema { id: String },
    Deploy,
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
    Hello { ok: bool, version: String },
    KeyResult {
        handled: bool,
        commit: Option<String>,
        context: ContextMsg,
        status: StatusMsg,
    },
    Ok { ok: bool },
    OptionValue { value: bool },
    DeployResult { ok: bool },
    StatusMsg(StatusMsg),
}

pub fn encode(msg: &impl Serialize) -> Vec<u8> {
    let body = serde_json::to_vec(msg).expect("json serialize");
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub fn decode<T: serde::de::DeserializeOwned>(buf: &[u8]) -> serde_json::Result<T> {
    serde_json::from_slice(buf)
}
