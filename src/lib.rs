mod bindings;
mod protocol;
mod state;
mod utils;
mod anthropic_client;

use crate::bindings::exports::ntwk::theater::actor::Guest;
use crate::bindings::exports::ntwk::theater::message_server_client::Guest as MessageServerClient;
use crate::bindings::ntwk::theater::runtime::log;
use crate::bindings::ntwk::theater::supervisor::spawn;
use crate::protocol::{
    create_error_response, create_history_response, create_message_response,
    create_settings_response, ChatStateRequest, ChatStateResponse,
};
use crate::state::ChatState;
use crate::utils::current_timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, from_str, to_vec};

#[derive(Serialize, Deserialize, Debug)]
struct InitData {
    conversation_id: String,
}

struct Component;
impl Guest for Component {
    fn init(init_state: Option<Vec<u8>>, params: (String,)) -> Result<(Option<Vec<u8>>,), String> {
        log("Initializing chat-state actor");
        let (param,) = params;

        let state = match init_state {
            Some(state) => {
                let parsed_init_state: InitData = from_slice(&state)
                    .map_err(|e| format!("Error deserializing init state: {}", e))?;
                log(&format!(
                    "Chat state actor initialized with conversation_id: {}",
                    parsed_init_state.conversation_id
                ));
                let anthropic_proxy_id = spawn(
                    "/Users/colinrozzi/work/actor-registry/anthropic-proxy/manifest.toml",
                    None,
                )
                .map_err(|e| format!("Error spawning anthropic-proxy: {}", e))?;
                ChatState::new(
                    param,
                    parsed_init_state.conversation_id,
                    anthropic_proxy_id,
                    current_timestamp()?,
                )
            }
            None => {
                log("Chat state actor is not initialized");
                return Err("Chat state actor is not initialized".to_string());
            }
        };

        // Serialize the state to bytes
        let state_bytes = to_vec(&state).map_err(|e| format!("Error serializing state: {}", e))?;
        log("Chat state actor initialized successfully");
        Ok((Some(state_bytes),))
    }
}

impl MessageServerClient for Component {
    fn handle_send(
        state: Option<Vec<u8>>,
        params: (Vec<u8>,),
    ) -> Result<(Option<Vec<u8>>,), String> {
        log("Handling send message in chat-state");
        let (_data,) = params;

        Ok((state,))
    }

    fn handle_request(
        state: Option<Vec<u8>>,
        params: (String, Vec<u8>),
    ) -> Result<(Option<Vec<u8>>, (Option<Vec<u8>>,)), String> {
        log("Handling request in chat-state");
        let (_request_id, data) = params;

        // Skip processing if no state
        let state_bytes = match state {
            Some(s) => s,
            None => {
                log("No state available, returning error");
                let error_response =
                    create_error_response("state_missing", "Chat state not initialized");
                let response_bytes = to_vec(&error_response)
                    .map_err(|e| format!("Error serializing error response: {}", e))?;
                return Ok((None, (Some(response_bytes),)));
            }
        };

        // Deserialize state
        let mut chat_state: ChatState =
            from_slice(&state_bytes).map_err(|e| format!("Error deserializing state: {}", e))?;

        // Parse request
        let request: ChatStateRequest =
            from_slice(&data).map_err(|e| format!("Error parsing request: {}", e))?;

        // Get current timestamp
        let timestamp = current_timestamp()?;

        // Process request based on action
        let response = match request.action.as_str() {
            "send_message" => {
                if let Some(message_content) = request.message {
                    // Add user message to state
                    let message_id = chat_state.add_user_message(message_content, timestamp);
                    
                    log("Sending message to anthropic-proxy for response");
                    
                    // Get the timestamp before sending the request
                    let request_start = current_timestamp()?;
                    
                    // Get a prepared set of messages for Anthropic API
                    let messages_for_anthropic = utils::prepare_messages_for_anthropic(&chat_state.messages);
                    
                    // Create request for anthropic-proxy
                    let anthropic_request = protocol::create_anthropic_request(
                        &chat_state.conversation_id,
                        &messages_for_anthropic, 
                        chat_state.system_prompt.clone(),
                        &chat_state.settings,
                    );
                    
                    // Serialize the request
                    let request_bytes = to_vec(&anthropic_request)
                        .map_err(|e| format!("Error serializing request to anthropic-proxy: {}", e))?;
                    
                    // Send the request to the anthropic-proxy
                    let response_bytes = bindings::ntwk::theater::message_server::message_request(
                        &chat_state.anthropic_proxy_id,
                        "process_request".to_string(),
                        request_bytes,
                    )
                    .map_err(|e| format!("Error communicating with anthropic-proxy: {}", e))?;
                    
                    // Get the timestamp after receiving the response
                    let request_end = current_timestamp()?;
                    let response_time = request_end - request_start;
                    
                    // Parse the response
                    let anthropic_response: protocol::AnthropicResponse = from_slice(&response_bytes)
                        .map_err(|e| format!("Error parsing anthropic-proxy response: {}", e))?;
                    
                    // Process the response
                    match anthropic_response.status.as_str() {
                        "success" => {
                            if let Some(completion) = anthropic_response.completion {
                                // Extract the content
                                let content = completion.content;
                                
                                // Add the assistant's message to the state
                                let assistant_message_id = chat_state.add_assistant_message(
                                    content,
                                    response_time,
                                    request_end,
                                );
                                
                                // Generate a title if this is the first exchange
                                if chat_state.title.starts_with("Conversation ") && chat_state.messages.len() >= 4 {
                                    // Use the first few messages to generate a title
                                    let title = utils::generate_conversation_title(&chat_state)?;
                                    chat_state.update_title(title, timestamp);
                                }
                                
                                // Get the assistant message
                                let assistant_message = chat_state
                                    .messages
                                    .iter()
                                    .find(|m| m.id == assistant_message_id)
                                    .ok_or("Failed to find assistant message")?
                                    .clone();
                                
                                create_message_response(assistant_message)
                            } else {
                                create_error_response("missing_completion", "No completion in successful response")
                            }
                        },
                        "error" => {
                            create_error_response(
                                "anthropic_error",
                                &format!(
                                    "Error from Anthropic API: {}", 
                                    anthropic_response.error.unwrap_or_else(|| "Unknown error".to_string())
                                )
                            )
                        },
                        _ => create_error_response(
                            "unknown_status", 
                            &format!("Unknown response status: {}", anthropic_response.status)
                        ),
                    }
                } else {
                    create_error_response("missing_message", "Message content is required")
                }
            },
            "update_settings" => {
                if let Some(settings) = request.settings {
                    chat_state.update_settings(settings.clone(), timestamp);
                    create_settings_response(settings)
                } else {
                    create_error_response("missing_settings", "Settings are required")
                }
            },
            "update_system_prompt" => {
                chat_state.update_system_prompt(request.system_prompt, timestamp);
                ChatStateResponse {
                    response_type: "system_prompt_updated".to_string(),
                    message: None,
                    history: None,
                    settings: None,
                    error: None,
                }
            },
            "update_title" => {
                if let Some(title) = request.title {
                    chat_state.update_title(title, timestamp);
                    ChatStateResponse {
                        response_type: "title_updated".to_string(),
                        message: None,
                        history: None,
                        settings: None,
                        error: None,
                    }
                } else {
                    create_error_response("missing_title", "Title is required")
                }
            },
            "get_history" => {
                let limit = request.history_params.as_ref().and_then(|p| p.limit);
                let before_timestamp = request
                    .history_params
                    .as_ref()
                    .and_then(|p| p.before_timestamp);
                let messages = chat_state.get_messages(limit, before_timestamp);
                create_history_response(messages)
            },
            _ => create_error_response(
                "unknown_action",
                &format!("Unknown action: {}", request.action),
            ),
        };

        // Serialize updated state
        let updated_state_bytes =
            to_vec(&chat_state).map_err(|e| format!("Error serializing updated state: {}", e))?;

        // Serialize response
        let response_bytes =
            to_vec(&response).map_err(|e| format!("Error serializing response: {}", e))?;

        Ok((Some(updated_state_bytes), (Some(response_bytes),)))
    }

    fn handle_channel_open(
        state: Option<Vec<u8>>,
        params: (Vec<u8>,),
    ) -> Result<
        (
            Option<Vec<u8>>,
            (bindings::exports::ntwk::theater::message_server_client::ChannelAccept,),
        ),
        String,
    > {
        log("Handling channel open in chat-state");
        let (_data,) = params;

        // Accept all channel open requests
        Ok((
            state,
            (
                bindings::exports::ntwk::theater::message_server_client::ChannelAccept {
                    accepted: true,
                    message: None,
                },
            ),
        ))
    }

    fn handle_channel_close(
        state: Option<Vec<u8>>,
        params: (String,),
    ) -> Result<(Option<Vec<u8>>,), String> {
        log("Handling channel close in chat-state");
        let (_channel_id,) = params;

        // No state modification needed for channel close
        Ok((state,))
    }

    fn handle_channel_message(
        state: Option<Vec<u8>>,
        params: (String, Vec<u8>),
    ) -> Result<(Option<Vec<u8>>,), String> {
        log("Received channel message in chat-state");
        let (_channel_id, _message) = params;

        // No state modification needed for now
        Ok((state,))
    }
}

bindings::export!(Component with_types_in bindings);
