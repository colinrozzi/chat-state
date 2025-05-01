use crate::bindings::ntwk::theater::message_server_host;
use crate::bindings::ntwk::theater::runtime::log;
use anthropic_types::{
    AnthropicRequest, AnthropicResponse, CompletionRequest, Message, OperationType,
};
use serde::{Deserialize, Serialize};
use serde_json::to_vec;
use std::collections::HashMap;

/// Main state structure for the chat-state actor
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatState {
    pub id: String,

    /// Basic information
    pub conversation_id: String,
    pub title: String,

    /// Actor references
    pub anthropic_proxy_id: String,

    /// Conversation content
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,

    /// Conversation settings
    pub settings: ConversationSettings,

    /// Subscription information
    pub subscriptions: Vec<String>,
}

/// Conversation settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConversationSettings {
    /// Model to use (e.g., "claude-3-7-sonnet-20250219")
    pub model: String,

    /// Temperature setting (0.0 to 1.0)
    pub temperature: Option<f32>,

    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,

    /// Any additional model parameters
    pub additional_params: Option<HashMap<String, serde_json::Value>>,
}

impl ChatState {
    /// Initialize a new state with default values
    pub fn new(id: String, conversation_id: String, anthropic_proxy_id: String) -> Self {
        ChatState {
            id,
            conversation_id: conversation_id.clone(),
            title: format!("Conversation {}", &conversation_id[0..8]),
            anthropic_proxy_id,
            system_prompt: None,
            messages: Vec::new(),
            settings: ConversationSettings {
                model: "claude-3-7-sonnet-20250219".to_string(),
                temperature: None,
                max_tokens: None,
                additional_params: None,
            },
            subscriptions: Vec::new(),
        }
    }

    pub fn generate_completion(&mut self) -> Result<Message, String> {
        log("Getting completion from anthropic-proxy");

        let anthropic_response = self.send_to_anthropic_proxy();

        match anthropic_response {
            Ok(response) => {
                let msg = Message {
                    role: "assistant".to_string(),
                    content: response.completion.expect("No completion found").content,
                };

                log("Received response from anthropic-proxy");

                self.add_message(msg.clone());
                Ok(msg)
            }
            Err(e) => {
                log(&format!("Error getting completion: {}", e));
                Err(format!("Error getting completion: {}", e))
            }
        }
    }

    /// Sends a request to the anthropic-proxy actor and returns the response
    pub fn send_to_anthropic_proxy(&self) -> Result<AnthropicResponse, String> {
        log("Sending request to anthropic-proxy");

        // Create the Anthropic request
        let request = AnthropicRequest {
            completion_request: Some(CompletionRequest {
                model: self.settings.model.clone(),
                messages: self.messages.clone(),
                temperature: self.settings.temperature,
                max_tokens: self.settings.max_tokens,
                additional_params: self.settings.additional_params.clone(),
                anthropic_version: Some("v1".to_string()),
                disable_parallel_tool_use: None,
                system: self.system_prompt.clone(),
                tools: None,
                tool_choice: None,
            }),
            operation_type: OperationType::ChatCompletion,
            params: None,
            version: "v1".to_string(),
            request_id: "unimplemented".to_string(),
        };

        // Serialize the request
        let request_bytes =
            to_vec(&request).map_err(|e| format!("Error serializing Anthropic request: {}", e))?;

        // Send the request to the anthropic-proxy
        log(&format!(
            "Sending request to proxy actor: {}",
            self.anthropic_proxy_id
        ));
        let response_bytes = message_server_host::request(&self.anthropic_proxy_id, &request_bytes)
            .map_err(|e| format!("Error sending request to anthropic-proxy: {}", e))?;

        // Parse the response
        let response: AnthropicResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| format!("Error parsing Anthropic response: {}", e))?;

        Ok(response)
    }

    pub fn add_message(&mut self, message: Message) {
        let msg_bytes = to_vec(&message).expect("Error serializing message for logging");

        log(&format!("Adding message: {:?}", message));
        self.messages.push(message);

        // Notify subscribers about the new message
        for subscriber in &self.subscriptions {
            log(&format!("Notifying subscriber: {}", subscriber));
            message_server_host::send(subscriber, &msg_bytes)
                .expect("Error sending message to subscriber");
        }
    }

    /// Update conversation settings
    pub fn update_settings(&mut self, settings: ConversationSettings) {
        self.settings = settings;
    }

    /// Update conversation title
    pub fn update_title(&mut self, title: String) {
        self.title = title;
    }

    /// Update system prompt
    pub fn update_system_prompt(&mut self, system_prompt: Option<String>) {
        self.system_prompt = system_prompt;
    }

    /// Subscribe to updates
    pub fn subscribe(&mut self, channel_id: String) {
        if !self.subscriptions.contains(&channel_id) {
            self.subscriptions.push(channel_id);
        }
    }

    /// Unsubscribe from updates
    pub fn unsubscribe(&mut self, channel_id: String) {
        self.subscriptions.retain(|id| id != &channel_id);
    }
}
