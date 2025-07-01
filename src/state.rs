use crate::bindings::theater::simple::message_server_host;
use crate::bindings::theater::simple::message_server_host::respond_to_request;
use crate::bindings::theater::simple::runtime::log;
use crate::bindings::theater::simple::store::{self, ContentRef};
use crate::bindings::theater::simple::supervisor::spawn;
use crate::protocol::{ChatStateRequest, ChatStateResponse, McpActorRequest, McpResponse};
use crate::proxy::Proxy;
use crate::state::message_server_host::send;
use crate::MCP_POC_MANIFEST;
use genai_types::messages::Role;
use genai_types::{
    messages::StopReason, CompletionRequest, CompletionResponse, Message, MessageContent,
    ModelInfo, ProxyRequest, ProxyResponse,
};
use mcp_protocol::tool::{Tool, ToolCallResult, ToolContent};
use serde::{Deserialize, Serialize};
use serde_json::{to_vec, Value};
use std::collections::HashMap;
use std::fmt::Display;
use thiserror::Error;

/// Main state structure for the chat-state actor
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatState {
    pub id: String,

    /// Basic information
    pub conversation_id: String,

    /// Proxy actor references
    pub proxies: HashMap<String, Proxy>,

    /// Conversation content
    pub messages: HashMap<String, ChatMessage>,

    /// Conversation settings
    pub settings: ConversationSettings,

    /// Channel-based subscription information
    pub subscription_channels: Vec<String>,

    /// Store ID for the conversation
    pub store_id: String,

    /// Head of the conversation
    pub head: Option<String>,

    /// Pending completion request id
    pub pending_completion: Option<String>,
}

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub struct ChatError {
    pub message: String,
    pub code: Option<String>,
}

impl Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ChatError: {} (code: {:?})", self.message, self.code)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub id: Option<String>,
    pub parent_id: Option<String>,
    pub entry: ChatEntry,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ChatEntry {
    Message(Message),
    Completion(CompletionResponse),
    Error(ChatError),
}

impl From<ChatEntry> for Message {
    fn from(entry: ChatEntry) -> Self {
        match entry {
            ChatEntry::Message(msg) => msg,
            ChatEntry::Completion(completion) => completion.into(),
            ChatEntry::Error(err) => Message {
                role: Role::User,
                content: vec![MessageContent::Text { text: err.message }],
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelConfig {
    pub model: String,
    pub provider: String,
}

/// Conversation settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InitConversationSettings {
    /// Model to use (e.g., "claude-3-7-sonnet-20250219")
    pub model_config: ModelConfig,

    /// Temperature setting (0.0 to 1.0)
    pub temperature: Option<f32>,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// System prompt to use
    pub system_prompt: Option<String>,

    /// Title of the conversation
    pub title: String,

    /// Mcp servers
    pub mcp_servers: Option<Vec<McpServer>>,
}

/// Into ConversationSettings trait to convert InitConversationSettings to ConversationSettings
impl From<InitConversationSettings> for ConversationSettings {
    fn from(init: InitConversationSettings) -> Self {
        ConversationSettings {
            model_config: init.model_config,
            temperature: init.temperature,
            max_tokens: init.max_tokens,
            system_prompt: init.system_prompt,
            title: init.title,
            mcp_servers: init.mcp_servers.unwrap_or_default(),
        }
    }
}

/// Conversation settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConversationSettings {
    /// Model to use (e.g., "claude-3-7-sonnet-20250219")
    pub model_config: ModelConfig,

    /// Temperature setting (0.0 to 1.0)
    pub temperature: Option<f32>,

    /// Maximum tokens to generate
    pub max_tokens: u32,

    /// System prompt to use
    pub system_prompt: Option<String>,

    /// Title of the conversation
    pub title: String,

    /// Mcp servers
    pub mcp_servers: Vec<McpServer>,
}

impl Default for ConversationSettings {
    fn default() -> Self {
        ConversationSettings {
            model_config: ModelConfig {
                model: "gemini-2.5-flash-preview-04-17".to_string(),
                provider: "google".to_string(),
            },
            temperature: None,
            max_tokens: 65535,
            system_prompt: None,
            title: "title".to_string(),
            mcp_servers: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StdPipeMcpConfig {
    command: String,
    args: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActorMcpConfig {
    manifest_path: String,
    init_state: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum McpConfig {
    #[serde(rename = "stdio")]
    StdPipe(StdPipeMcpConfig),
    #[serde(rename = "actor")]
    Actor(ActorMcpConfig),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpServer {
    pub actor_id: Option<String>,
    #[serde(flatten)]
    pub config: McpConfig,
    pub tools: Option<Vec<Tool>>,
}

impl McpServer {
    pub fn call_tool(&self, tool: String, args: Value) -> Result<McpResponse, String> {
        log(&format!("Calling tool: {} with args: {:?}", tool, args));
        // Check if the MCP server is started
        if self.actor_id.is_none() {
            return Err("MCP server not started".to_string());
        }

        // Check if the tool is available
        if self.tools.is_none() {
            return Err("No tools available".to_string());
        }

        // Check if the tool is valid
        let tools = self.tools.as_ref()
            .ok_or("No tools available")?;
        
        if !tools.iter().any(|t| t.name == tool) {
            return Err(format!("Tool {} not found", tool));
        }

        // Call the tool with the given arguments
        let actor_id = self.actor_id.as_ref()
            .ok_or("MCP server not started")?;
        
        let request_bytes = to_vec(&McpActorRequest::ToolsCall { name: tool, args })
            .map_err(|e| format!("Failed to serialize tool use request: {}", e))?;
        
        let result = message_server_host::request(actor_id, &request_bytes)
            .map_err(|e| format!("Failed to call tool: {}", e))?;

        serde_json::from_slice(&result)
            .map_err(|e| format!("Failed to parse tool response: {}", e))
    }

    pub fn has_tool(&self, tool: &str) -> bool {
        self.tools
            .as_ref()
            .map(|tools| tools.iter().any(|t| t.name == tool))
            .unwrap_or(false)
    }
}

impl ChatState {
    /// Initialize a new state with default values
    pub fn new(
        id: String,
        conversation_id: String,
        proxies: HashMap<String, Proxy>,
        store_id: String,
        conversation_settings: ConversationSettings,
    ) -> Self {
        log(&format!("Initializing chat state with ID: {}", id));

        // Check the store to see if there is anything stored
        let head = match store::get_by_label(&store_id, &conversation_id) {
            Ok(Some(stored_head)) => {
                log(&format!("Found stored head: {:?}", stored_head));
                match store::get(&store_id, &stored_head) {
                    Ok(head_bytes) => {
                        match serde_json::from_slice::<Option<String>>(&head_bytes) {
                            Ok(head) => {
                                log(&format!("Deserialized head: {:?}", head));
                                head
                            }
                            Err(e) => {
                                log(&format!("Failed to deserialize head: {}", e));
                                None
                            }
                        }
                    }
                    Err(e) => {
                        log(&format!("Failed to get stored head: {}", e));
                        None
                    }
                }
            }
            Ok(None) => {
                log("No stored heads found");
                None
            }
            Err(e) => {
                log(&format!("Failed to check for stored heads: {}", e));
                None
            }
        };

        log(&format!(
            "Conversation settings from initialization: {:?}",
            conversation_settings
        ));

        ChatState {
            id,
            conversation_id: conversation_id.clone(),
            proxies,
            messages: HashMap::new(),
            settings: conversation_settings,
            subscription_channels: Vec::new(),
            store_id,
            head,
            pending_completion: None,
        }
    }

    pub fn store_settings(&self) -> Result<(), String> {
        log("Storing conversation settings");

        let settings_bytes = to_vec(&self.settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        let settings_label = format!("settings_{}", self.conversation_id);
        store::store_at_label(&self.store_id, &settings_label, &settings_bytes)
            .map_err(|e| format!("Failed to store settings: {}", e))?;

        log("Stored conversation settings successfully");
        Ok(())
    }

    pub fn start_mcp_servers(&mut self) -> Result<(), String> {
        for mcp in &mut self.settings.mcp_servers {
            if let Some(ref actor_id) = mcp.actor_id {
                log(&format!(
                    "MCP server already started with actor ID: {}",
                    actor_id
                ));
                continue;
            }

            let actor_id = match &mcp.config {
                McpConfig::StdPipe(config) => {
                    log(&format!(
                        "Starting MCP server with stdio: {} {:?}",
                        config.command, config.args
                    ));
                    let config_bytes = serde_json::to_vec(&config)
                        .map_err(|e| format!("Failed to serialize stdio config: {}", e))?;
                    spawn(MCP_POC_MANIFEST, Some(&config_bytes))
                }
                McpConfig::Actor(config) => {
                    log(&format!(
                        "Starting MCP server with actor manifest: {}",
                        config.manifest_path
                    ));
                    let init_state_bytes = serde_json::to_vec(&config.init_state)
                        .map_err(|e| format!("Failed to serialize init state: {}", e))?;
                    spawn(&config.manifest_path, Some(&init_state_bytes))
                }
            };

            match actor_id {
                Ok(id) => {
                    log(&format!("MCP server started with actor ID: {}", id));
                    mcp.actor_id = Some(id.clone());

                    // Send the actor a list tools request
                    let tool_list_request = McpActorRequest::ToolsList {};
                    let request_bytes = to_vec(&tool_list_request)
                        .map_err(|e| format!("Failed to serialize tool list request: {}", e))?;
                    
                    log(&format!("Sending tool list request to MCP server: {}", id));
                    
                    let response_bytes = message_server_host::request(&id, &request_bytes)
                        .map_err(|e| format!("Failed to send tool list request to MCP server: {}", e))?;

                    let response: McpResponse = serde_json::from_slice(&response_bytes)
                        .map_err(|e| format!("Failed to parse tool list response: {}", e))?;

                    if let Some(result) = response.result {
                        let tools_value = result.get("tools")
                            .ok_or("No 'tools' field in MCP response")?;
                        
                        let tools = serde_json::from_value::<Vec<Tool>>(tools_value.clone())
                            .map_err(|e| format!("Failed to parse tool list from response: {}", e))?;

                        mcp.tools = Some(tools);
                    } else if let Some(error) = response.error {
                        log(&format!("Error in tool list response: {:?}", error));
                        return Err(format!("Error in tool list response: {}", error.message));
                    } else {
                        log("No result or error in tool list response");
                        return Err("No result or error in tool list response".to_string());
                    }
                }
                Err(e) => {
                    log(&format!("Failed to start MCP server: {}", e));
                    return Err(format!("Failed to start MCP server: {}", e));
                }
            }
        }

        Ok(())
    }

    pub fn continue_chain(&mut self) -> Result<(), String> {
        let head_id = self.head.as_ref()
            .ok_or("No head message found - conversation chain is empty")?;

        let last_message = self.messages.get(head_id)
            .ok_or("Head message not found in message store - data corruption possible")?
            .clone();

        match last_message.entry {
            ChatEntry::Message(msg) => {
                log(&format!(
                    "Last message is a message, nothing to continue: {:?}",
                    msg
                ));
                self.resolve_pending_completion()
                    .map_err(|e| format!("Failed to resolve pending completion: {}", e))?;
                Ok(())
            }
            ChatEntry::Completion(completion) => {
                log(&format!(
                    "Continuing chain with completion: {:?}",
                    completion
                ));

                match completion.stop_reason {
                    StopReason::EndTurn => {
                        log("Received end turn signal from proxy");
                        self.resolve_pending_completion()
                            .map_err(|e| format!("Failed to resolve pending completion after end turn: {}", e))?;
                        Ok(())
                    }
                    StopReason::MaxTokens => {
                        log("Received max tokens signal from proxy");
                        self.resolve_pending_completion()
                            .map_err(|e| format!("Failed to resolve pending completion after max tokens: {}", e))?;
                        Ok(())
                    }
                    StopReason::StopSequence => {
                        log("Received stop sequence signal from proxy");
                        self.resolve_pending_completion()
                            .map_err(|e| format!("Failed to resolve pending completion after stop sequence: {}", e))?;
                        Ok(())
                    }
                    StopReason::ToolUse => {
                        log("Received tool use signal from proxy");

                        let tool_responses = self.process_tools(completion)
                            .map_err(|e| format!("Failed to process tools: {}", e))?;

                        let tool_msg = ChatEntry::Message(Message {
                            role: Role::User,
                            content: tool_responses.clone(),
                        });

                        self.add_message(tool_msg.clone());

                        self.generate_completion()
                            .map_err(|e| format!("Failed to generate completion after tool use: {}", e))?;

                        Ok(())
                    }
                    StopReason::Other(signal) => {
                        log(&format!(
                            "Received unknown signal from proxy: {}",
                            signal
                        ));
                        self.resolve_pending_completion()
                            .map_err(|e| format!("Failed to resolve pending completion after unknown signal: {}", e))?;
                        Ok(())
                    }
                }
            }
            ChatEntry::Error(err) => {
                log(&format!("Last message is an error: {:?}", err));
                self.resolve_pending_completion()
                    .map_err(|e| format!("Failed to resolve pending completion after error: {}", e))?;
                Ok(())
            }
        }
    }

    pub fn resolve_pending_completion(&mut self) -> Result<(), String> {
        log("Resolving pending completion");

        match self.pending_completion {
            Some(ref id) => {
                log(&format!("Resolving pending completion with ID: {}", id));

                let msg = serde_json::to_vec(&ChatStateResponse::Head {
                    head: self.head.clone(),
                })
                .map_err(|e| format!("Failed to serialize head response: {}", e))?;
                if let Err(e) = respond_to_request(id, &msg) {
                    log(&format!("Failed to respond to request: {}", e));
                    // Don't return error here as the completion itself succeeded
                }

                log("Sent head response to pending completion");
            }
            None => {
                log("No pending completion to resolve");
            }
        }

        self.pending_completion = None;

        Ok(())
    }

    pub fn generate_completion(&mut self) -> Result<(), String> {
        if self.messages.is_empty() {
            return Err("Cannot generate completion: no messages in conversation".to_string());
        }

        // Generate a completion
        let model_response = self.generate_proxy_completion(&self.settings.model_config.provider.clone())
            .map_err(|e| format!("Failed to generate proxy completion: {}", e))?;

        log("Generated completion successfully");

        self.add_message(ChatEntry::Completion(model_response.clone()));

        let msg = serde_json::to_vec(&ChatStateRequest::ContinueProcessing)
            .map_err(|e| format!("Failed to serialize continue processing message: {}", e))?;
        
        send(&self.id, &msg)
            .map_err(|e| format!("Failed to send continue processing message: {}", e))?;
        log("Sent continue processing message");

        Ok(())
    }

    pub fn get_tools(&self) -> Result<Option<Vec<Tool>>, String> {
        log("Getting tools from MCP servers");

        let mut tools = Vec::new();

        for mcp in &self.settings.mcp_servers {
            if let Some(ref actor_id) = mcp.actor_id {
                if let Some(ref mcp_tools) = mcp.tools {
                    tools.extend(mcp_tools.clone());
                } else {
                    log(&format!("No tools found for MCP server: {}", actor_id));
                }
            } else {
                log("MCP server not started");
            }
        }

        if tools.is_empty() {
            log("No tools found");
            return Ok(None);
        } else {
            log(&format!("Found tools: {:?}", tools));
            return Ok(Some(tools));
        }
    }

    /// Call a tool with the given completion
    pub fn process_tools(
        &self,
        completion: CompletionResponse,
    ) -> Result<Vec<MessageContent>, String> {
        log("Processing tools");

        let mut tool_results = Vec::new();

        for message_content in completion.content {
            match message_content {
                MessageContent::ToolUse { id, name, input } => {
                    log(&format!("Calling tool: {} with args: {:?}", name, input));

                    // Call the tool with the given arguments
                    let result = self.call_tool(name, input)?;

                    log(&format!("Tool result: {:?}", result));
                    let tool_use_result = match result.error {
                        Some(err) => {
                            log(&format!("Error calling tool: {}", err.message));
                            MessageContent::ToolResult {
                                tool_use_id: id,
                                content: vec![ToolContent::Text {
                                    text: err.message.clone(),
                                }],
                                is_error: Some(true),
                            }
                        }
                        None => {
                            log(&format!("Tool call result: {:?}", result.result));

                            let tool_result_value = result.result
                                .ok_or("No result field in tool response")?;

                            let tool_result = serde_json::from_value::<ToolCallResult>(tool_result_value)
                                .map_err(|e| format!("Failed to parse tool call result: {}", e))?;

                            MessageContent::ToolResult {
                                tool_use_id: id,
                                content: tool_result.content,
                                is_error: None,
                            }
                        }
                    };

                    tool_results.push(tool_use_result);
                }
                _ => {
                    log("No tool use message found");
                }
            }
        }

        Ok(tool_results)
    }

    /// Get the list of tools from the MCP servers
    pub fn list_tools(&self) -> Result<Vec<Tool>, String> {
        log("Getting tool list from MCP servers");

        let mut tools = Vec::new();

        for mcp in &self.settings.mcp_servers {
            if let Some(ref actor_id) = mcp.actor_id {
                if let Some(ref mcp_tools) = mcp.tools {
                    tools.extend(mcp_tools.clone());
                } else {
                    log(&format!("No tools found for MCP server: {}", actor_id));
                }
            } else {
                log("MCP server not started");
            }
        }

        if tools.is_empty() {
            log("No tools found");
            return Err("No tools found".to_string());
        } else {
            log(&format!("Found tools: {:?}", tools));
            return Ok(tools);
        }
    }

    /// Get the list of models from the proxies
    pub fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        log("Getting model list from proxies");

        let mut models = Vec::new();

        for proxy in self.proxies.values() {
            let response = proxy.send_to_proxy(ProxyRequest::ListModels);

            match response {
                Ok(ProxyResponse::ListModels { models: m }) => {
                    log(&format!("Found models: {:?}", m));
                    models.extend(m);
                }
                Ok(_) => {
                    log("Unexpected response from proxy");
                    return Err("Unexpected response from proxy".to_string());
                }
                Err(e) => {
                    log(&format!("Error getting model list: {}", e));
                    return Err(format!("Error getting model list: {}", e));
                }
            }
        }

        if models.is_empty() {
            log("No models found");
            return Err("No models found".to_string());
        } else {
            log(&format!("Found models: {:?}", models));
            return Ok(models);
        }
    }

    /// Call a tool with the given name and arguments
    pub fn call_tool(&self, name: String, args: Value) -> Result<McpResponse, String> {
        log(&format!("Calling tool: {} with args: {:?}", name, args));

        // Check if the tool is available
        for mcp in &self.settings.mcp_servers {
            if mcp.has_tool(&name) {
                return mcp.call_tool(name, args);
            }
        }

        Err(format!("Tool {} not found", name))
    }

    /// Sends a request to the anthropic-proxy actor and returns the response
    pub fn generate_proxy_completion(
        &mut self,
        proxy_name: &String,
    ) -> Result<CompletionResponse, String> {
        log(&format!(
            "Generating completion from proxy actor: {}",
            proxy_name
        ));

        let messages = self
            .get_chain()
            .into_iter()
            .map(|m| m.entry.into())
            .collect::<Vec<_>>();

        // Create the Anthropic request
        let request = ProxyRequest::GenerateCompletion {
            request: CompletionRequest {
                model: self.settings.model_config.model.clone(),
                messages,
                temperature: self.settings.temperature,
                max_tokens: self.settings.max_tokens,
                disable_parallel_tool_use: None,
                system: self.settings.system_prompt.clone(),
                tools: self.get_tools()
                    .map_err(|e| format!("Failed to get tools for completion: {}", e))?,
                tool_choice: None,
            },
        };

        let response = self
            .proxies
            .get(proxy_name)
            .ok_or_else(|| format!("Proxy {} not found", proxy_name))?
            .send_to_proxy(request)
            .map_err(|e| format!("Failed to send request to proxy: {}", e))?;

        match response {
            ProxyResponse::Completion { completion } => {
                log("Received completion from proxy");
                Ok(completion)
            }
            ProxyResponse::Error { error } => {
                log(&format!("Error from proxy: {}", error));
                Err(format!("Error from proxy: {}", error))
            }
            _ => Err("Unexpected response from anthropic-proxy".to_string()),
        }
    }

    pub fn add_message(&mut self, chat_entry: ChatEntry) {
        log("Adding message to conversation");

        let mut chat_msg = ChatMessage {
            id: None,
            parent_id: self.head.clone(),
            entry: chat_entry,
        };

        // Serialize and store the message
        let msg_bytes = to_vec(&chat_msg)
            .map_err(|e| {
                log(&format!("Failed to serialize message: {}", e));
                return;
            })
            .unwrap();
        
        let msg_ref = match store::store(&self.store_id, &msg_bytes) {
            Ok(msg_ref) => msg_ref,
            Err(e) => {
                log(&format!("Failed to store message: {}", e));
                return;
            }
        };

        let id = msg_ref.hash.clone();

        chat_msg.id = Some(id.clone());

        self.messages.insert(id.clone(), chat_msg.clone());
        self.head = Some(id.clone());

        if let Err(e) = self.store_head() {
            log(&format!("Failed to store head: {}", e));
        }

        log(&format!("Updated head: {:?}", self.head));
        self.notify_subscribers(chat_msg.clone());
    }

    pub fn store_head(&self) -> Result<(), String> {
        log("Storing head of conversation");

        let head_bytes = to_vec(&self.head)
            .map_err(|e| format!("Failed to serialize head: {}", e))?;
        
        store::store_at_label(&self.store_id, &self.conversation_id, &head_bytes)
            .map_err(|e| format!("Failed to store head: {}", e))?;
        
        Ok(())
    }

    pub fn set_head(&mut self, head: Option<String>) -> Result<(), String> {
        log(&format!("Setting head of conversation to: {:?}", head));

        // look for the head in the messages
        if let Some(ref head_id) = head {
            if !self.messages.contains_key(head_id) {
                log(&format!("Head ID {} not found in messages", head_id));
                return Err(format!("Head ID {} not found in messages", head_id));
            }
        }

        self.head = head.clone();
        if let Err(e) = self.store_head() {
            log(&format!("Failed to store head: {}", e));
        }
        Ok(())
    }

    pub fn get_head(&self) -> Option<String> {
        log("Getting head of conversation");
        self.head.clone()
    }

    pub fn notify_subscribers(&self, chat_msg: ChatMessage) {
        log("Notifying subscription channels");

        let head_msg = match serde_json::to_vec(&ChatStateResponse::Head {
            head: self.head.clone(),
        }) {
            Ok(msg) => msg,
            Err(e) => {
                log(&format!("Failed to serialize head message: {}", e));
                return;
            }
        };

        let chat_msg = match serde_json::to_vec(&ChatStateResponse::ChatMessage { message: chat_msg }) {
            Ok(msg) => msg,
            Err(e) => {
                log(&format!("Failed to serialize chat message: {}", e));
                return;
            }
        };

        for channel_id in &self.subscription_channels {
            log(&format!("Notifying channel: {}", channel_id));

            match message_server_host::send_on_channel(channel_id, &head_msg) {
                Ok(_) => {
                    log(&format!("Notified channel {}: {:?}", channel_id, head_msg));
                }
                Err(e) => log(&format!("Failed to notify channel {}: {}", channel_id, e)),
            }

            match message_server_host::send_on_channel(channel_id, &chat_msg) {
                Ok(_) => {
                    log(&format!("Notified channel {}: {:?}", channel_id, chat_msg));
                }
                Err(e) => log(&format!("Failed to notify channel {}: {}", channel_id, e)),
            }
        }
    }

    pub fn get_chain(&mut self) -> Vec<ChatMessage> {
        let mut chain = Vec::new();

        let mut current_id = self.head.clone();
        while let Some(id) = current_id {
            if let Ok(Some(message)) = self.get_message(&id) {
                chain.push(message.clone());
                current_id = message.parent_id.clone();
            } else {
                break;
            }
        }

        chain.reverse();

        chain
    }

    pub fn get_message(&mut self, id: &str) -> Result<Option<ChatMessage>, String> {
        log(&format!("Getting message with ID: {}", id));

        // If the message is not found in our messages, check the store
        match self.messages.get(id) {
            Some(message) => Ok(Some(message.clone())),
            None => {
                let content_ref = ContentRef {
                    hash: id.to_string(),
                };
                match store::get(&self.store_id, &content_ref) {
                    Ok(msg_bytes) => {
                        log(&format!("Found message in store with ID: {}", id));
                        let message: ChatMessage = match serde_json::from_slice(&msg_bytes) {
                            Ok(msg) => msg,
                            Err(e) => {
                                log(&format!("Failed to deserialize stored message: {}", e));
                                return Ok(None);
                            }
                        };
                        self.messages.insert(id.to_string(), message.clone());
                        Ok(Some(message.clone()))
                    }
                    Err(_) => {
                        log(&format!("Message not found in store with ID: {}", id));
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// Get conversation settings
    pub fn get_settings(&self) -> &ConversationSettings {
        &self.settings
    }

    /// Update conversation settings
    pub fn update_settings(&mut self, settings: ConversationSettings) {
        self.settings = settings;

        log(&format!("Updated settings: {:?}", self.settings));

        // Start or restart MCP servers with new configuration
        if let Err(e) = self.start_mcp_servers() {
            log(&format!("Failed to start MCP servers: {}", e));
        }

        if let Err(e) = self.store_settings() {
            log(&format!("Failed to store conversation settings: {}", e));
        }
    }

    /// Add channel to subscriptions (called automatically)
    pub fn add_subscription_channel(&mut self, channel_id: String) {
        if !self.subscription_channels.contains(&channel_id) {
            self.subscription_channels.push(channel_id.clone());
            log(&format!("Auto-subscribed channel: {}", channel_id));
        }
    }

    /// Remove channel from subscriptions (called automatically on channel close)
    pub fn remove_subscription_channel(&mut self, channel_id: &str) {
        self.subscription_channels.retain(|id| id != channel_id);
        log(&format!("Unsubscribed closed channel: {}", channel_id));
    }
}
