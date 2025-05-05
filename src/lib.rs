mod bindings;
mod protocol;
mod state;

use crate::bindings::exports::ntwk::theater::actor::Guest;
use crate::bindings::exports::ntwk::theater::message_server_client::Guest as MessageServerClient;
use crate::bindings::ntwk::theater::runtime::log;
use crate::bindings::ntwk::theater::supervisor::spawn;
use crate::protocol::{create_error_response, ChatStateRequest, ChatStateResponse};
use crate::state::ChatState;

use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec};

#[derive(Serialize, Deserialize, Debug)]
struct InitData {
    conversation_id: String,
}

struct Component;
impl Guest for Component {
    fn init(init_state: Option<Vec<u8>>, params: (String,)) -> Result<(Option<Vec<u8>>,), String> {
        log("Initializing chat-state actor");
        let (param,) = params;

        let mut state = match init_state {
            Some(state) => {
                let parsed_init_state: InitData = from_slice(&state)
                    .map_err(|e| format!("Error deserializing init state: {}", e))?;
                log(&format!(
                    "Chat state actor initialized with conversation_id: {}",
                    parsed_init_state.conversation_id
                ));
                let anthropic_proxy_id = spawn(
                    "/Users/colinrozzi/work/actor-registry/google-proxy/manifest.toml",
                    None,
                )
                .map_err(|e| format!("Error spawning anthropic-proxy: {}", e))?;
                ChatState::new(param, parsed_init_state.conversation_id, anthropic_proxy_id)
            }
            None => {
                log("Chat state actor is not initialized");
                return Err("Chat state actor is not initialized".to_string());
            }
        };

        // Start MCP servers
        state
            .start_mcp_servers()
            .expect("Failed to start MCP servers");

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
                let error_response = create_error_response(
                    "no_state",
                    "Chat state actor is not initialized or has no state",
                );
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

        // Process request based on action
        let response = match request {
            ChatStateRequest::AddMessage { message } => {
                chat_state.add_message(message.clone());
                ChatStateResponse::Success
            }
            ChatStateRequest::GenerateCompletion => {
                let response = chat_state.generate_completion();
                match response {
                    Ok(completion) => ChatStateResponse::Completion {
                        messages: completion,
                    },
                    Err(e) => {
                        log(&format!("Error generating completion: {}", e));
                        create_error_response("completion_error", &e)
                    }
                }
            }
            ChatStateRequest::GetSettings => {
                let settings = chat_state.get_settings();

                // Convert internal settings to client-compatible format
                let client_settings = protocol::internal_to_client_settings(settings);

                ChatStateResponse::Settings {
                    settings: client_settings,
                }
            }
            ChatStateRequest::UpdateSettings { settings } => {
                log("Updating settings");
                log(&format!("Settings: {:?}", settings));
                chat_state.update_settings(settings.clone());
                ChatStateResponse::Success
            }
            ChatStateRequest::Subscribe { sub_id } => {
                chat_state.subscribe(sub_id);
                ChatStateResponse::Success
            }
            ChatStateRequest::Unsubscribe { sub_id } => {
                chat_state.unsubscribe(sub_id);
                ChatStateResponse::Success
            }
            ChatStateRequest::GetHistory => ChatStateResponse::History {
                messages: chat_state.messages.clone(),
            },
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

        // Reject all channel open requests
        Ok((
            state,
            (
                bindings::exports::ntwk::theater::message_server_client::ChannelAccept {
                    accepted: false,
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
