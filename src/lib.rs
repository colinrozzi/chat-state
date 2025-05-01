mod bindings;
mod protocol;
mod state;
mod utils;

use crate::bindings::exports::ntwk::theater::actor::Guest;
use crate::bindings::exports::ntwk::theater::message_server_client::Guest as MessageServerClient;
use crate::bindings::ntwk::theater::runtime::log;
use crate::protocol::{ChatStateRequest, ChatStateResponse, create_error_response, create_history_response, create_message_response, create_settings_response};
use crate::state::ChatState;
use crate::utils::current_timestamp;
use serde_json::{from_slice, from_str, to_vec};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct InitData {
    conversation_id: String,
    parent_interface_id: String,
    anthropic_proxy_id: String,
    system_prompt: Option<String>,
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
                log(&format!("Chat state actor initialized with conversation_id: {}", parsed_init_state.conversation_id));
                ChatState::new(
                    param,
                    parsed_init_state.conversation_id,
                    parsed_init_state.parent_interface_id,
                    parsed_init_state.anthropic_proxy_id,
                    parsed_init_state.system_prompt,
                    current_timestamp()?,
                )
            },
            None => {
                log("Chat state actor is not initialized");
                return Err("Chat state actor is not initialized".to_string())
            }
        };
        
        // Serialize the state to bytes
        let state_bytes = to_vec(&state)
            .map_err(|e| format!("Error serializing state: {}", e))?;
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
                let error_response = create_error_response("state_missing", "Chat state not initialized");
                let response_bytes = to_vec(&error_response)
                    .map_err(|e| format!("Error serializing error response: {}", e))?;
                return Ok((None, (Some(response_bytes),)));
            }
        };
        
        // Deserialize state
        let mut chat_state: ChatState = from_slice(&state_bytes)
            .map_err(|e| format!("Error deserializing state: {}", e))?;
        
        // Parse request
        let request: ChatStateRequest = from_slice(&data)
            .map_err(|e| format!("Error parsing request: {}", e))?;
        
        // Get current timestamp
        let timestamp = current_timestamp()?;
        
        // Process request based on action
        let response = match request.action.as_str() {
            "send_message" => {
                if let Some(message_content) = request.message {
                    // Add user message to state
                    let _message_id = add_user_message(&mut chat_state, message_content, timestamp);
                    
                    // For now, simulate a response since we don't have direct anthropic-proxy access
                    let response_time = 1500; // ms
                    let assistant_message_id = add_assistant_message(
                        &mut chat_state, 
                        "This is a simulated response from the assistant since we don't have direct access to the anthropic-proxy. In a real implementation, we would send the messages to the anthropic-proxy actor and get a response from the Claude model.".to_string(),
                        response_time,
                        timestamp + 2, // Add 2 seconds for simulation
                    );
                    
                    // Get the assistant message
                    let assistant_message = chat_state.messages.iter()
                        .find(|m| m.id == assistant_message_id)
                        .ok_or("Failed to find assistant message")?
                        .clone();
                    
                    // Simple title generation if this is the first message exchange
                    if chat_state.title.starts_with("Conversation ") && chat_state.messages.len() >= 4 {
                        if let Some(first_message) = chat_state.messages.first() {
                            // Generate a title based on the first user message
                            let content = &first_message.content;
                            let title = if content.len() > 30 {
                                format!("{}...", &content[0..30])
                            } else {
                                content.clone()
                            };
                            update_title(&mut chat_state, title, timestamp);
                        }
                    }
                    
                    create_message_response(assistant_message)
                } else {
                    create_error_response("missing_message", "Message content is required")
                }
            },
            "update_settings" => {
                if let Some(settings) = request.settings {
                    update_settings(&mut chat_state, settings.clone(), timestamp);
                    create_settings_response(settings)
                } else {
                    create_error_response("missing_settings", "Settings are required")
                }
            },
            "update_system_prompt" => {
                update_system_prompt(&mut chat_state, request.system_prompt, timestamp);
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
                    update_title(&mut chat_state, title, timestamp);
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
                let before_timestamp = request.history_params.as_ref().and_then(|p| p.before_timestamp);
                let messages = get_messages(&chat_state, limit, before_timestamp);
                create_history_response(messages)
            },
            _ => create_error_response("unknown_action", &format!("Unknown action: {}", request.action)),
        };
        
        // Serialize updated state
        let updated_state_bytes = to_vec(&chat_state)
            .map_err(|e| format!("Error serializing updated state: {}", e))?;
        
        // Serialize response
        let response_bytes = to_vec(&response)
            .map_err(|e| format!("Error serializing response: {}", e))?;
        
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
