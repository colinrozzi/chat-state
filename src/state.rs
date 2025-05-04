use crate::bindings::ntwk::theater::message_server_host;
use crate::bindings::ntwk::theater::runtime::log;
use crate::bindings::ntwk::theater::supervisor::spawn;
use crate::protocol::{McpActorRequest, McpResponse};
use genai_types::{
    messages::StopReason, CompletionRequest, CompletionResponse, Message, MessageContent,
    ProxyRequest, ProxyResponse,
};
use mcp_protocol::tool::{Tool, ToolCallResult};
use serde::{Deserialize, Serialize};
use serde_json::{to_vec, Value};
use std::collections::HashMap;

/// Main state structure for the chat-state actor
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatState {
    pub id: String,

    /// Basic information
    pub conversation_id: String,

    /// Actor references
    pub anthropic_proxy_id: String,

    /// Conversation content
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
    pub max_tokens: u32,

    /// Any additional model parameters
    pub additional_params: Option<HashMap<String, serde_json::Value>>,

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
            model: "claude-3-7-sonnet-20250219".to_string(),
            temperature: None,
            max_tokens: 8192,
            additional_params: None,
            system_prompt: None,
            title: "title".to_string(),
            mcp_servers: vec![McpServer {
                config: McpConfig {
                    command: "/Users/colinrozzi/work/mcp-servers/bin/fs-mcp-server".to_string(),
                    args: vec![
                        "--allowed-dirs".to_string(),
                        "/Users/colinrozzi/work/tmp".to_string(),
                    ],
                },
                actor_id: None,
                tools: None,
            }],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpConfig {
    command: String,
    args: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpServer {
    pub actor_id: Option<String>,
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
        if !self.tools.as_ref().unwrap().iter().any(|t| t.name == tool) {
            return Err(format!("Tool {} not found", tool));
        }

        // Call the tool with the given arguments
        let result = message_server_host::request(
            self.actor_id.as_ref().unwrap(),
            &to_vec(&McpActorRequest::ToolsCall { name: tool, args })
                .expect("Error serializing tool use request"),
        )
        .map_err(|e| format!("Error calling tool: {}", e))?;

        serde_json::from_slice(&result).map_err(|e| format!("Error parsing tool response: {}", e))
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
    pub fn new(id: String, conversation_id: String, anthropic_proxy_id: String) -> Self {
        ChatState {
            id,
            conversation_id: conversation_id.clone(),
            anthropic_proxy_id,
            messages: Vec::new(),
            settings: ConversationSettings::default(),
            subscriptions: Vec::new(),
        }
    }

    pub fn start_mcp_servers(&mut self) -> Result<(), String> {
        for mcp in &mut self.settings.mcp_servers {
            if mcp.actor_id.is_some() {
                log(&format!(
                    "MCP server already started with actor ID: {}",
                    mcp.actor_id.as_ref().unwrap()
                ));
                continue;
            }

            log(&format!(
                "Starting MCP server: {} with args: {:?}",
                mcp.config.command, mcp.config.args
            ));

            let actor_id = spawn(
                "/Users/colinrozzi/work/actors/mcp-poc/manifest.toml",
                Some(&serde_json::to_vec(&mcp.config).unwrap()),
            );

            match actor_id {
                Ok(id) => {
                    log(&format!("MCP server started with actor ID: {}", id));
                    mcp.actor_id = Some(id.clone());

                    // Send the actor a list tools request
                    let tool_list_request = McpActorRequest::ToolsList {};
                    let request_bytes =
                        to_vec(&tool_list_request).expect("Error serializing tool list request");
                    log(&format!("Sending tool list request to MCP server: {}", id));
                    let response_bytes = message_server_host::request(&id, &request_bytes)
                        .expect("Error sending tool list request to MCP server");

                    let response: McpResponse = serde_json::from_slice(&response_bytes)
                        .expect("Error parsing tool list response");

                    if let Some(result) = response.result {
                        log(&format!("Tool list response: {:?}", result));
                        let tools = serde_json::from_value::<Vec<Tool>>(
                            result.get("tools").unwrap().clone(),
                        )
                        .expect("Error parsing tool list response");

                        log(&format!("Parsed tools: {:?}", tools));

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
                    log(&format!("Error starting MCP server: {}", e));
                    return Err(format!("Error starting MCP server: {}", e));
                }
            }
        }

        Ok(())
    }

    pub fn generate_completion(&mut self) -> Result<Vec<Message>, String> {
        log("Getting completion from anthropic-proxy");

        if self.messages.is_empty() {
            return Err("No messages in conversation".to_string());
        }

        let mut new_messages = Vec::new();

        loop {
            // Generate a completion
            let anthropic_response = self
                .send_to_anthropic_proxy()
                .expect("Error getting completion");

            let msg = Message {
                role: "assistant".to_string(),
                content: anthropic_response.content.clone(),
            };

            self.add_message(msg.clone());
            new_messages.push(msg);

            match anthropic_response.stop_reason {
                StopReason::EndTurn => {
                    log("Received end turn signal from anthropic-proxy");
                    break;
                }
                StopReason::MaxTokens => {
                    log("Received max tokens signal from anthropic-proxy");
                    break;
                }
                StopReason::StopSequence => {
                    log("Received stop sequence signal from anthropic-proxy");
                    break;
                }
                StopReason::ToolUse => {
                    log("Received tool use signal from anthropic-proxy");

                    let tool_responses = self
                        .process_tools(anthropic_response)
                        .expect("Error calling tool");

                    let tool_msg = Message {
                        role: "user".to_string(),
                        content: tool_responses.clone(),
                    };

                    self.add_message(tool_msg.clone());
                }
            }
        }

        log("Generated completion successfully");
        Ok(new_messages)
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

                    let err = if result.error.is_some() {
                        Some(true)
                    } else {
                        None
                    };

                    let tool_result =
                        serde_json::from_value::<ToolCallResult>(result.result.clone().unwrap())
                            .expect("Error parsing tool call result");

                    let tool_use_result = MessageContent::ToolResult {
                        tool_use_id: id,
                        content: tool_result.content,
                        is_error: err,
                    };

                    log(&format!("Tool result: {:?}", result));

                    tool_results.push(tool_use_result);
                }
                _ => {
                    log("No tool use message found");
                }
            }
        }

        Ok(tool_results)
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
    pub fn send_to_anthropic_proxy(&self) -> Result<CompletionResponse, String> {
        log("Sending request to anthropic-proxy");

        // Create the Anthropic request
        let request = ProxyRequest::GenerateCompletion {
            request: CompletionRequest {
                model: self.settings.model.clone(),
                messages: self.messages.clone(),
                temperature: self.settings.temperature,
                max_tokens: self.settings.max_tokens,
                disable_parallel_tool_use: None,
                system: self.settings.system_prompt.clone(),
                tools: self.get_tools().expect("Error getting tools"),
                tool_choice: None,
            },
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
        let response: ProxyResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| format!("Error parsing Anthropic response: {}", e))?;

        match response {
            ProxyResponse::Completion { completion } => {
                log("Received completion from anthropic-proxy");
                Ok(completion)
            }
            _ => Err("Unexpected response from anthropic-proxy".to_string()),
        }
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

    /// Get conversation settings
    pub fn get_settings(&self) -> &ConversationSettings {
        &self.settings
    }

    /// Update conversation settings
    pub fn update_settings(&mut self, settings: ConversationSettings) {
        self.settings = settings;

        log(&format!("Updated settings: {:?}", self.settings));

        self.start_mcp_servers()
            .expect("Error starting MCP servers");
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
