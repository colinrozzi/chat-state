use genai_types::{Message, ModelInfo};
use mcp_protocol::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::state::ConversationSettings;

// Actor API request structures
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum McpActorRequest {
    ToolsList {},
    ToolsCall { name: String, args: Value },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Messages received by the chat-state actor
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ChatStateRequest {
    #[serde(rename = "add_message")]
    AddMessage { message: Message },
    #[serde(rename = "generate_completion")]
    GenerateCompletion,
    #[serde(rename = "get_settings")]
    GetSettings,
    #[serde(rename = "update_settings")]
    UpdateSettings { settings: ConversationSettings },
    #[serde(rename = "get_history")]
    GetHistory,
    #[serde(rename = "subscribe")]
    Subscribe { sub_id: String },
    #[serde(rename = "unsubscribe")]
    Unsubscribe { sub_id: String },
    #[serde(rename = "list_models")]
    ListModels,
    #[serde(rename = "list_tools")]
    ListTools,
}

/// Data associated with the response
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ChatStateResponse {
    #[serde(rename = "success")]
    Success,

    #[serde(rename = "message")]
    Message { message: Message },

    #[serde(rename = "completion")]
    Completion { messages: Vec<Message> },

    #[serde(rename = "history")]
    History { messages: Vec<Message> },

    #[serde(rename = "settings")]
    Settings { settings: ConversationSettings },

    #[serde(rename = "error")]
    Error { error: ErrorInfo },

    #[serde(rename = "tools_list")]
    ToolsList { tools: Vec<Tool> },

    #[serde(rename = "models_list")]
    ModelsList { models: Vec<ModelInfo> },
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
    ChatStateResponse::Error {
        error: ErrorInfo {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        },
    }
}

/// Convert internal settings to client-compatible settings
pub fn internal_to_client_settings(
    settings: &crate::state::ConversationSettings,
) -> crate::state::ConversationSettings {
    // Just return the settings directly
    settings.clone()
}
