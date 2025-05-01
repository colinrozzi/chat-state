use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::state::{ChatMessage, ConversationSettings};

/// Messages received by the chat-state actor
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatStateRequest {
    /// Action to perform: "send_message", "update_settings", "get_history", etc.
    pub action: String,
    
    /// Message content (for send_message action)
    pub message: Option<String>,
    
    /// Settings update (for update_settings action)
    pub settings: Option<ConversationSettings>,
    
    /// System prompt update (for update_system_prompt action)
    pub system_prompt: Option<String>,
    
    /// Title update (for update_title action)
    pub title: Option<String>,
    
    /// Parameters for history retrieval (for get_history action)
    pub history_params: Option<HistoryParams>,
}

/// Parameters for retrieving conversation history
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HistoryParams {
    /// Maximum number of messages to return
    pub limit: Option<u32>,
    
    /// Return messages before this timestamp
    pub before_timestamp: Option<u64>,
}

/// Responses sent from the chat-state actor
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatStateResponse {
    /// Response type: "message", "history", "settings_updated", "error", etc.
    pub response_type: String,
    
    /// Message content if this is a message response
    pub message: Option<ChatMessage>,
    
    /// Message history if this is a history response
    pub history: Option<Vec<ChatMessage>>,
    
    /// Settings if this is a settings response
    pub settings: Option<ConversationSettings>,
    
    /// Error details if this is an error response
    pub error: Option<ErrorInfo>,
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

/// Request format for the Anthropic API via anthropic-proxy
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnthropicRequest {
    /// API version
    pub version: String,
    
    /// Operation type ("chat_completion")
    pub operation_type: String,
    
    /// Request ID for tracking
    pub request_id: String,
    
    /// Completion request details
    pub completion_request: Option<CompletionRequest>,
    
    /// Additional parameters
    pub params: Option<HashMap<String, serde_json::Value>>,
}

/// Completion request details
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompletionRequest {
    /// Model to use
    pub model: String,
    
    /// Message history
    pub messages: Vec<AnthropicMessage>,
    
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    
    /// Temperature setting
    pub temperature: Option<f32>,
    
    /// System prompt
    pub system: Option<String>,
    
    /// Top-p setting
    pub top_p: Option<f32>,
    
    /// Anthropic API version
    pub anthropic_version: Option<String>,
    
    /// Additional parameters
    pub additional_params: Option<HashMap<String, serde_json::Value>>,
}

/// Message format for Anthropic API
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnthropicMessage {
    /// Message role ("user" or "assistant")
    pub role: String,
    
    /// Message content
    pub content: String,
}

/// Response from the Anthropic API
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnthropicResponse {
    /// API version
    pub version: String,
    
    /// Request ID
    pub request_id: String,
    
    /// Response status ("success" or "error")
    pub status: String,
    
    /// Error message if status is "error"
    pub error: Option<String>,
    
    /// Completion results if status is "success"
    pub completion: Option<CompletionResult>,
}

/// Completion result
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompletionResult {
    /// Generated content
    pub content: String,
    
    /// Completion ID
    pub id: String,
    
    /// Model used
    pub model: String,
    
    /// Reason the generation stopped
    pub stop_reason: String,
    
    /// Stop sequence if applicable
    pub stop_sequence: Option<String>,
    
    /// Message type
    pub message_type: String,
    
    /// Token usage statistics
    pub usage: Usage,
}

/// Token usage statistics
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Usage {
    /// Input tokens used
    pub input_tokens: u32,
    
    /// Output tokens generated
    pub output_tokens: u32,
}

// Helper functions to create common messages

/// Create an error response
pub fn create_error_response(code: &str, message: &str) -> ChatStateResponse {
    ChatStateResponse {
        response_type: "error".to_string(),
        message: None,
        history: None,
        settings: None,
        error: Some(ErrorInfo {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }),
    }
}

/// Create a message response
pub fn create_message_response(message: ChatMessage) -> ChatStateResponse {
    ChatStateResponse {
        response_type: "message".to_string(),
        message: Some(message),
        history: None,
        settings: None,
        error: None,
    }
}

/// Create a history response
pub fn create_history_response(messages: Vec<ChatMessage>) -> ChatStateResponse {
    ChatStateResponse {
        response_type: "history".to_string(),
        message: None,
        history: Some(messages),
        settings: None,
        error: None,
    }
}

/// Create a settings response
pub fn create_settings_response(settings: ConversationSettings) -> ChatStateResponse {
    ChatStateResponse {
        response_type: "settings_updated".to_string(),
        message: None,
        history: None,
        settings: Some(settings),
        error: None,
    }
}

/// Convert a ChatMessage to an AnthropicMessage
pub fn to_anthropic_message(message: &ChatMessage) -> AnthropicMessage {
    AnthropicMessage {
        role: message.role.clone(),
        content: message.content.clone(),
    }
}

/// Create an AnthropicRequest from state and a user message
pub fn create_anthropic_request(
    conversation_id: &str,
    messages: &[ChatMessage],
    system_prompt: Option<String>,
    settings: &ConversationSettings,
) -> AnthropicRequest {
    AnthropicRequest {
        version: "1.0".to_string(),
        operation_type: "chat_completion".to_string(),
        request_id: format!("req-{}", conversation_id),
        completion_request: Some(CompletionRequest {
            model: settings.model.clone(),
            messages: messages.iter().map(to_anthropic_message).collect(),
            max_tokens: Some(settings.max_tokens),
            temperature: Some(settings.temperature),
            system: system_prompt,
            top_p: settings.top_p,
            anthropic_version: None,
            additional_params: settings.additional_params.clone(),
        }),
        params: None,
    }
}
