use anthropic_types::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::state::ConversationSettings;

/// Messages received by the chat-state actor
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ChatStateRequest {
    #[serde(rename = "add_message")]
    AddMessage(Message),
    #[serde(rename = "generate_completion")]
    GenerateCompletion,
    #[serde(rename = "update_settings")]
    UpdateSettings(ConversationSettings),
    #[serde(rename = "update_system_prompt")]
    UpdateSystemPrompt(Option<String>),
    #[serde(rename = "update_title")]
    UpdateTitle(String),
    #[serde(rename = "get_history")]
    GetHistory,
    #[serde(rename = "subscribe")]
    Subscribe(String),
    #[serde(rename = "unsubscribe")]
    Unsubscribe(String),
}

/// Data associated with the response
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ChatStateResponse {
    #[serde(rename = "success")]
    Success,

    #[serde(rename = "message")]
    Message(Message),

    #[serde(rename = "history")]
    History(Vec<Message>),

    #[serde(rename = "settings")]
    Settings(ConversationSettings),

    #[serde(rename = "error")]
    Error(ErrorInfo),
}

/// Error information
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ErrorInfo {
    /// Error code
    pub code: String,

    /// Human-readable error message
    pub message: String,

    /// Additional error details
    pub details: Option<HashMap<String, String>>,
}

/// Create an error response
pub fn create_error_response(code: &str, message: &str) -> ChatStateResponse {
    ChatStateResponse::Error(ErrorInfo {
        code: code.to_string(),
        message: message.to_string(),
        details: None,
    })
}
