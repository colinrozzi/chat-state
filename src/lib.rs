mod bindings;
mod protocol;
mod proxy;
mod state;

use crate::bindings::exports::ntwk::theater::actor::Guest;
use crate::bindings::exports::ntwk::theater::message_server_client::Guest as MessageServerClient;
use crate::bindings::exports::ntwk::theater::supervisor_handlers::Guest as SupervisorHandlers;
use crate::bindings::ntwk::theater::runtime::log;
use crate::bindings::ntwk::theater::store::new;
use crate::protocol::{create_error_response, ChatStateRequest, ChatStateResponse};
use crate::proxy::Proxy;
use crate::state::ChatState;

use bindings::ntwk::theater::types::WitActorError;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec};
use state::ConversationSettings;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
struct InitData {
    store_id: Option<String>,
    conversation_id: String,
    config: Option<ConversationSettings>,
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

                let mut proxies = HashMap::new();
                let anthropic_proxy = Proxy::new(
                    "anthropic",
                    "/Users/colinrozzi/work/actor-registry/anthropic-proxy/manifest.toml",
                )
                .map_err(|e| format!("Error spawning anthropic-proxy: {}", e))?;

                let google_proxy = Proxy::new(
                    "google",
                    "/Users/colinrozzi/work/actor-registry/google-proxy/manifest.toml",
                )
                .map_err(|e| format!("Error spawning google-proxy: {}", e))?;
                proxies.insert("anthropic".to_string(), anthropic_proxy);
                proxies.insert("google".to_string(), google_proxy);

                let store_id = match parsed_init_state.store_id {
                    Some(store_id) => store_id,
                    None => {
                        log("No store_id provided, creating a new store");
                        new().map_err(|e| format!("Error creating new store: {}", e))?
                    }
                };

                ChatState::new(
                    param,
                    parsed_init_state.conversation_id,
                    proxies,
                    store_id,
                    parsed_init_state.config,
                )
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

        log(&format!(
            "Stringified request data: {}",
            String::from_utf8_lossy(&data)
        ));
        // Parse request
        let request: ChatStateRequest =
            from_slice(&data).map_err(|e| format!("Error parsing request: {}", e))?;

        // Process request based on action
        let response = match request {
            ChatStateRequest::AddMessage { message } => {
                chat_state.add_message(message);
                ChatStateResponse::Success
            }
            ChatStateRequest::GenerateCompletion => {
                let response = chat_state.generate_completion();
                match response {
                    Ok(head) => ChatStateResponse::Head { head: Some(head) },
                    Err(e) => {
                        log(&format!("Error generating completion: {}", e));
                        create_error_response("completion_error", &e)
                    }
                }
            }
            ChatStateRequest::GetHead => ChatStateResponse::Head {
                head: chat_state.get_head(),
            },
            ChatStateRequest::GetMessage { message_id } => {
                match chat_state.get_message(&message_id) {
                    Ok(Some(message)) => ChatStateResponse::ChatMessage {
                        message: message.clone(),
                    },
                    Ok(None) => ChatStateResponse::Error {
                        error: protocol::ErrorInfo {
                            code: "404".to_string(),
                            details: None,
                            message: "Message not found".to_string(),
                        },
                    },
                    Err(e) => {
                        log(&format!("Error getting message: {}", e));
                        create_error_response("message_error", &e)
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
            // Note: Subscribe/Unsubscribe removed - channels handle this automatically
            ChatStateRequest::GetHistory => ChatStateResponse::History {
                messages: chat_state.get_chain(),
            },
            ChatStateRequest::ListModels => {
                let models = chat_state.list_models();
                match models {
                    Ok(models) => ChatStateResponse::ModelsList { models },
                    Err(e) => {
                        log(&format!("Error listing models: {}", e));
                        create_error_response("models_error", &e)
                    }
                }
            }
            ChatStateRequest::ListTools => match chat_state.list_tools() {
                Ok(tools) => ChatStateResponse::ToolsList { tools },
                Err(e) => {
                    log(&format!("Error listing tools: {}", e));
                    create_error_response("tools_error", &e)
                }
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
        log("Accepting channel for subscription");
        let (_initial_msg,) = params;  // Ignore initial message content

        // Accept all channels - Theater will provide channel_id later
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
        let (channel_id,) = params;
        
        let mut chat_state: ChatState = match state {
            Some(s) => from_slice(&s).map_err(|e| format!("Error deserializing state: {}", e))?,
            None => return Ok((state,)),
        };
        
        // Remove closed channel from subscriptions
        chat_state.remove_subscription_channel(&channel_id);
        
        let updated_state_bytes = to_vec(&chat_state)
            .map_err(|e| format!("Error serializing updated state: {}", e))?;
        
        Ok((Some(updated_state_bytes),))
    }

    fn handle_channel_message(
        state: Option<Vec<u8>>,
        params: (String, Vec<u8>),
    ) -> Result<(Option<Vec<u8>>,), String> {
        let (channel_id, _message) = params;
        
        let mut chat_state: ChatState = match state {
            Some(s) => from_slice(&s).map_err(|e| format!("Error deserializing state: {}", e))?,
            None => return Ok((state,)),
        };
        
        // Add channel to subscriptions if not already present
        chat_state.add_subscription_channel(channel_id);
        
        let updated_state_bytes = to_vec(&chat_state)
            .map_err(|e| format!("Error serializing updated state: {}", e))?;
        
        Ok((Some(updated_state_bytes),))
    }
}

impl SupervisorHandlers for Component {
    fn handle_child_error(
        state: Option<Vec<u8>>,
        _params: (String, WitActorError),
    ) -> Result<(Option<Vec<u8>>,), String> {
        log("Handling child error in chat-state");
        Ok((state,))
    }

    fn handle_child_exit(
        state: Option<Vec<u8>>,
        _params: (String, Option<Vec<u8>>),
    ) -> Result<(Option<Vec<u8>>,), String> {
        log("Handling child exit in chat-state");
        Ok((state,))
    }
}

bindings::export!(Component with_types_in bindings);
