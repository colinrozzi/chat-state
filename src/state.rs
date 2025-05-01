use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main state structure for the chat-state actor
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatState {
    pub id: String,

    /// Basic information
    pub conversation_id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,

    /// Actor references
    pub anthropic_proxy_id: String,

    /// Conversation content
    pub system_prompt: Option<String>,
    pub messages: Vec<ChatMessage>,

    /// Conversation settings
    pub settings: ConversationSettings,
}

/// Chat message structure
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    /// Unique message identifier
    pub id: String,

    /// Message role: "user" or "assistant"
    pub role: String,

    /// Message content
    pub content: String,

    /// Timestamp when the message was created
    pub timestamp: u64,

    /// Optional metadata about the message
    pub metadata: Option<MessageMetadata>,
}

/// Message metadata (optional)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageMetadata {
    /// Token count of the message
    pub token_count: Option<u32>,

    /// Time taken to generate response (for assistant messages)
    pub response_time_ms: Option<u64>,

    /// Any additional metadata fields
    pub additional: Option<HashMap<String, String>>,
}

/// Conversation settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConversationSettings {
    /// Model to use (e.g., "claude-3-7-sonnet-20250219")
    pub model: String,

    /// Temperature setting (0.0 to 1.0)
    pub temperature: f32,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// Top-p setting (optional)
    pub top_p: Option<f32>,

    /// Any additional model parameters
    pub additional_params: Option<HashMap<String, serde_json::Value>>,
}

impl ChatState {
    /// Initialize a new state with default values
    pub fn new(
        id: String,
        conversation_id: String,
        anthropic_proxy_id: String,
        timestamp: u64,
    ) -> Self {
        ChatState {
            id,
            conversation_id: conversation_id.clone(),
            title: format!("Conversation {}", &conversation_id[0..8]),
            created_at: timestamp,
            updated_at: timestamp,
            anthropic_proxy_id,
            system_prompt: None,
            messages: Vec::new(),
            settings: ConversationSettings {
                model: "claude-3-7-sonnet-20250219".to_string(),
                temperature: 0.7,
                max_tokens: 4096,
                top_p: None,
                additional_params: None,
            },
        }
    }

    /// Add a user message to the conversation
    pub fn add_user_message(&mut self, content: String, timestamp: u64) -> String {
        let message_id = generate_message_id(&self.conversation_id, self.messages.len());

        let message = ChatMessage {
            id: message_id.clone(),
            role: "user".to_string(),
            content,
            timestamp,
            metadata: None,
        };

        self.messages.push(message);
        self.updated_at = timestamp;

        message_id
    }

    /// Add an assistant message to the conversation
    pub fn add_assistant_message(
        &mut self,
        content: String,
        response_time_ms: u64,
        timestamp: u64,
    ) -> String {
        let message_id = generate_message_id(&self.conversation_id, self.messages.len());

        let message = ChatMessage {
            id: message_id.clone(),
            role: "assistant".to_string(),
            content,
            timestamp,
            metadata: Some(MessageMetadata {
                token_count: None, // We could estimate this
                response_time_ms: Some(response_time_ms),
                additional: None,
            }),
        };

        self.messages.push(message);
        self.updated_at = timestamp;

        message_id
    }

    /// Update conversation settings
    pub fn update_settings(&mut self, settings: ConversationSettings, timestamp: u64) {
        self.settings = settings;
        self.updated_at = timestamp;
    }

    /// Update conversation title
    pub fn update_title(&mut self, title: String, timestamp: u64) {
        self.title = title;
        self.updated_at = timestamp;
    }

    /// Update system prompt
    pub fn update_system_prompt(&mut self, system_prompt: Option<String>, timestamp: u64) {
        self.system_prompt = system_prompt;
        self.updated_at = timestamp;
    }

    /// Get a subset of messages from the conversation
    pub fn get_messages(
        &self,
        limit: Option<u32>,
        before_timestamp: Option<u64>,
    ) -> Vec<ChatMessage> {
        let mut messages = self.messages.clone();

        // Apply before_timestamp filter if specified
        if let Some(timestamp) = before_timestamp {
            messages.retain(|msg| msg.timestamp < timestamp);
        }

        // Apply limit if specified
        if let Some(limit) = limit {
            if messages.len() > limit as usize {
                let start_idx = messages.len() - limit as usize;
                messages = messages[start_idx..].to_vec();
            }
        }

        messages
    }
}

/// Helper function to generate a unique message ID
fn generate_message_id(conversation_id: &str, message_index: usize) -> String {
    format!("{}-msg-{}", conversation_id, message_index)
}
