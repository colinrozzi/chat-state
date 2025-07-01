mod bindings;
mod protocol;
mod proxy;
mod state;

use crate::bindings::exports::theater::simple::actor::Guest;
use crate::bindings::exports::theater::simple::message_server_client::Guest as MessageServerClient;
use crate::bindings::exports::theater::simple::supervisor_handlers::Guest as SupervisorHandlers;
use crate::bindings::theater::simple::runtime::log;
use crate::bindings::theater::simple::store::new;
use crate::protocol::{create_error_response, ChatStateRequest, ChatStateResponse};
use crate::proxy::Proxy;
use crate::state::ChatState;

use bindings::theater::simple::random::generate_uuid;
use bindings::theater::simple::store::{self};
use bindings::theater::simple::types::{WitActorError, WitErrorType};
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec};
use state::{ChatEntry, ConversationSettings, InitConversationSettings};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
struct InitData {
    store_id: Option<String>,
    conversation_id: Option<String>,
    config: Option<InitConversationSettings>,
}

const ANTHROPIC_PROXY_MANIFEST: &str =
    "https://github.com/colinrozzi/anthropic-proxy/releases/latest/download/manifest.toml";
const GOOGLE_PROXY_MANIFEST: &str =
    "https://github.com/colinrozzi/google-proxy/releases/latest/download/manifest.toml";
const MCP_POC_MANIFEST: &str =
    "https://github.com/colinrozzi/mcp-poc/releases/latest/download/manifest.toml";

struct Component;
impl Guest for Component {
    fn init(init_state: Option<Vec<u8>>, params: (String,)) -> Result<(Option<Vec<u8>>,), String> {
        log("Initializing chat-state actor");
        let (param,) = params;

        let mut state = match init_state {
            Some(state) => {
                let parsed_init_state: InitData = from_slice(&state)
                    .map_err(|e| format!("Failed to deserialize init state: {}", e))?;
                log(&format!(
                    "Chat state actor initialized with conversation_id: {:?}",
                    parsed_init_state.conversation_id
                ));

                let mut proxies = HashMap::new();
                let anthropic_proxy = Proxy::new("anthropic", ANTHROPIC_PROXY_MANIFEST)
                    .map_err(|e| format!("Failed to spawn anthropic-proxy: {}", e))?;

                let google_proxy = Proxy::new("google", GOOGLE_PROXY_MANIFEST)
                    .map_err(|e| format!("Failed to spawn google-proxy: {}", e))?;
                proxies.insert("anthropic".to_string(), anthropic_proxy);
                proxies.insert("google".to_string(), google_proxy);

                let store_id = match parsed_init_state.store_id {
                    Some(store_id) => store_id,
                    None => {
                        log("No store_id provided, creating a new store");
                        new().map_err(|e| format!("Failed to create new store: {}", e))?
                    }
                };

                let conversation_id = match parsed_init_state.conversation_id {
                    Some(conversation_id) => conversation_id,
                    None => {
                        log("No conversation_id provided, generating a random one");
                        generate_uuid().map_err(|e| format!("Failed to generate UUID: {}", e))?
                    }
                };

                let conversation_settings = match parsed_init_state.config {
                    Some(config) => config.into(),
                    None => {
                        // check if we have conversation settings stored
                        log("No config provided, checking store for existing settings");
                        let settings_label = format!("settings_{}", conversation_id);
                        match store::get_by_label(&store_id, &settings_label) {
                            Ok(Some(settings_ref)) => {
                                log("Found existing settings in store");
                                match store::get(&store_id, &settings_ref) {
                                    Ok(settings) => from_slice(&settings)
                                        .map_err(|e| {
                                            format!("Failed to deserialize settings: {}", e)
                                        })
                                        .unwrap_or_else(|_| {
                                            log("Failed to deserialize settings, using default");
                                            ConversationSettings::default()
                                        }),
                                    Err(e) => {
                                        log(&format!(
                                            "Failed to retrieve settings from store: {}",
                                            e
                                        ));
                                        ConversationSettings::default()
                                    }
                                }
                            }
                            Ok(None) => {
                                log("No existing settings found in store, using default");
                                ConversationSettings::default()
                            }
                            Err(e) => {
                                log(&format!(
                                    "Failed to check for existing settings: {}, using default",
                                    e
                                ));
                                ConversationSettings::default()
                            }
                        }
                    }
                };

                let chat_state = ChatState::new(
                    param,
                    conversation_id,
                    proxies,
                    store_id,
                    conversation_settings,
                );
                chat_state
                    .store_settings()
                    .map_err(|e| format!("Failed to store initial settings: {}", e))?;
                chat_state
            }
            None => {
                log("Chat state actor is not initialized");
                return Err(
                    "Chat state actor initialization failed: no init state provided".to_string(),
                );
            }
        };

        // Start MCP servers
        if let Err(e) = state.start_mcp_servers() {
            log(&format!("Failed to start MCP servers: {}", e));
            return Err(format!("Failed to start MCP servers: {}", e));
        }

        // Serialize the state to bytes
        let state_bytes =
            to_vec(&state).map_err(|e| format!("Failed to serialize state: {}", e))?;
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

        // Continue the chain
        let mut chat_state: ChatState = match state {
            Some(s) => from_slice(&s).map_err(|e| format!("Failed to deserialize state: {}", e))?,
            None => return Ok((state,)),
        };

        match serde_json::from_slice::<ChatStateRequest>(&_data) {
            Ok(request) => match request {
                ChatStateRequest::ContinueProcessing => {
                    log("Received continue processing message");
                    if let Err(e) = chat_state.continue_chain() {
                        log(&format!("Failed to continue chain: {}", e));
                        return Err(format!("Failed to continue processing chain: {}", e));
                    }
                    let updated_state_bytes = to_vec(&chat_state)
                        .map_err(|e| format!("Failed to serialize updated state: {}", e))?;
                    Ok((Some(updated_state_bytes),))
                }
                ChatStateRequest::AddMessage { message } => {
                    log(&format!("Adding message: {:?}", message));
                    chat_state.add_message(ChatEntry::Message(message));
                    let updated_state_bytes = to_vec(&chat_state)
                        .map_err(|e| format!("Failed to serialize updated state: {}", e))?;
                    Ok((Some(updated_state_bytes),))
                }
                ChatStateRequest::GenerateCompletion => {
                    log("Generating completion");
                    if chat_state.pending_completion.is_none() {
                        chat_state.pending_completion = Some("pending_completion".to_string());
                        if let Err(e) = chat_state.generate_completion() {
                            log(&format!("Failed to generate completion: {}", e));
                            return Err(format!("Failed to generate completion: {}", e));
                        }
                    }
                    let updated_state_bytes = to_vec(&chat_state)
                        .map_err(|e| format!("Failed to serialize updated state: {}", e))?;
                    Ok((Some(updated_state_bytes),))
                }
                ChatStateRequest::SetHead { head } => {
                    log(&format!("Setting head to: {:?}", head));
                    if let Err(e) = chat_state.set_head(head) {
                        log(&format!("Failed to set head: {}", e));
                        // Don't return error here, just log it
                    }
                    let updated_state_bytes = to_vec(&chat_state)
                        .map_err(|e| format!("Failed to serialize updated state: {}", e))?;
                    Ok((Some(updated_state_bytes),))
                }
                _ => {
                    let updated_state_bytes = to_vec(&chat_state)
                        .map_err(|e| format!("Failed to serialize updated state: {}", e))?;
                    Ok((Some(updated_state_bytes),))
                }
            },
            Err(_) => {
                log(&format!(
                    "Received unrecognized message: {}",
                    String::from_utf8_lossy(&_data)
                ));
                // If the message is not a valid request, just return the state
                let updated_state_bytes = to_vec(&chat_state)
                    .map_err(|e| format!("Failed to serialize updated state: {}", e))?;
                Ok((Some(updated_state_bytes),))
            }
        }
    }

    fn handle_request(
        state: Option<Vec<u8>>,
        params: (String, Vec<u8>),
    ) -> Result<(Option<Vec<u8>>, (Option<Vec<u8>>,)), String> {
        log("Handling request in chat-state");
        let (request_id, data) = params;

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
                    .map_err(|e| format!("Failed to serialize error response: {}", e))?;
                return Ok((None, (Some(response_bytes),)));
            }
        };

        // Deserialize state
        let mut chat_state: ChatState =
            from_slice(&state_bytes).map_err(|e| format!("Failed to deserialize state: {}", e))?;

        log(&format!(
            "Stringified request data: {}",
            String::from_utf8_lossy(&data)
        ));
        // Parse request
        let request: ChatStateRequest =
            from_slice(&data).map_err(|e| format!("Failed to parse request: {}", e))?;

        // Process request based on action
        let response = match request {
            ChatStateRequest::ContinueProcessing => {
                log("Continuing processing chain");
                match chat_state.continue_chain() {
                    Ok(_) => ChatStateResponse::Success,
                    Err(e) => {
                        log(&format!("Failed to continue chain: {}", e));
                        create_error_response("continue_chain_error", &e)
                    }
                }
            }
            ChatStateRequest::AddMessage { message } => {
                chat_state.add_message(ChatEntry::Message(message));
                ChatStateResponse::Success
            }
            ChatStateRequest::GenerateCompletion => match chat_state.pending_completion {
                Some(_) => {
                    log("Pending completion already exists, skipping generation");
                    let err = ChatStateResponse::Error {
                        error: protocol::ErrorInfo {
                            code: "pending_completion".to_string(),
                            details: None,
                            message: "Pending completion already exists".to_string(),
                        },
                    };

                    let state_bytes = to_vec(&chat_state)
                        .map_err(|e| format!("Failed to serialize state: {}", e))?;

                    return Ok((
                        Some(state_bytes),
                        (Some(to_vec(&err).map_err(|e| {
                            format!("Failed to serialize error: {}", e)
                        })?),),
                    ));
                }
                None => {
                    log("Generating completion");
                    chat_state.pending_completion = Some(request_id);
                    match chat_state.generate_completion() {
                        Ok(_) => {
                            let state_bytes = to_vec(&chat_state)
                                .map_err(|e| format!("Failed to serialize state: {}", e))?;

                            return Ok((Some(state_bytes), (None,)));
                        }
                        Err(e) => {
                            log(&format!("Failed to generate completion: {}", e));
                            return Err(format!("Failed to generate completion: {}", e));
                        }
                    }
                }
            },
            ChatStateRequest::GetHead => ChatStateResponse::Head {
                head: chat_state.get_head(),
            },
            ChatStateRequest::SetHead { head } => {
                log(&format!("Setting head to: {:?}", head));
                match chat_state.set_head(head) {
                    Ok(_) => ChatStateResponse::Success,
                    Err(e) => {
                        log(&format!("Failed to set head: {}", e));
                        create_error_response("set_head_error", &e)
                    }
                }
            }
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
                        log(&format!("Failed to get message: {}", e));
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
            ChatStateRequest::GetHistory => ChatStateResponse::History {
                messages: chat_state.get_chain(),
            },
            ChatStateRequest::ListModels => {
                let models = chat_state.list_models();
                match models {
                    Ok(models) => ChatStateResponse::ModelsList { models },
                    Err(e) => {
                        log(&format!("Failed to list models: {}", e));
                        create_error_response("models_error", &e)
                    }
                }
            }
            ChatStateRequest::ListTools => match chat_state.list_tools() {
                Ok(tools) => ChatStateResponse::ToolsList { tools },
                Err(e) => {
                    log(&format!("Failed to list tools: {}", e));
                    create_error_response("tools_error", &e)
                }
            },
            ChatStateRequest::GetMetadata => ChatStateResponse::Metadata {
                conversation_id: chat_state.conversation_id.clone(),
                store_id: chat_state.store_id.clone(),
            },
        };

        // Serialize updated state
        let updated_state_bytes =
            to_vec(&chat_state).map_err(|e| format!("Failed to serialize updated state: {}", e))?;

        // Serialize response
        let response_bytes =
            to_vec(&response).map_err(|e| format!("Failed to serialize response: {}", e))?;

        Ok((Some(updated_state_bytes), (Some(response_bytes),)))
    }

    fn handle_channel_open(
        state: Option<Vec<u8>>,
        params: (String, Vec<u8>),
    ) -> Result<
        (
            Option<Vec<u8>>,
            (bindings::exports::theater::simple::message_server_client::ChannelAccept,),
        ),
        String,
    > {
        log("Accepting channel for subscription");
        let (channel_id, _initial_msg) = params; // Ignore initial message content

        let mut chat_state: ChatState = match state {
            Some(s) => from_slice(&s).map_err(|e| format!("Failed to deserialize state: {}", e))?,
            None => {
                return Ok((
                    state,
                    (
                        bindings::exports::theater::simple::message_server_client::ChannelAccept {
                            accepted: false,
                            message: None,
                        },
                    ),
                ))
            }
        };

        // Add channel to subscriptions
        chat_state.add_subscription_channel(channel_id.clone());

        // Serialize updated state
        let updated_state_bytes =
            to_vec(&chat_state).map_err(|e| format!("Failed to serialize updated state: {}", e))?;

        Ok((
            Some(updated_state_bytes),
            (
                bindings::exports::theater::simple::message_server_client::ChannelAccept {
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
            Some(s) => from_slice(&s).map_err(|e| format!("Failed to deserialize state: {}", e))?,
            None => return Ok((state,)),
        };

        // Remove closed channel from subscriptions
        chat_state.remove_subscription_channel(&channel_id);

        let updated_state_bytes =
            to_vec(&chat_state).map_err(|e| format!("Failed to serialize updated state: {}", e))?;

        Ok((Some(updated_state_bytes),))
    }

    fn handle_channel_message(
        state: Option<Vec<u8>>,
        params: (String, Vec<u8>),
    ) -> Result<(Option<Vec<u8>>,), String> {
        let (channel_id, _message) = params;

        let mut chat_state: ChatState = match state {
            Some(s) => from_slice(&s).map_err(|e| format!("Failed to deserialize state: {}", e))?,
            None => return Ok((state,)),
        };

        // Add channel to subscriptions if not already present
        chat_state.add_subscription_channel(channel_id);

        let updated_state_bytes =
            to_vec(&chat_state).map_err(|e| format!("Failed to serialize updated state: {}", e))?;

        Ok((Some(updated_state_bytes),))
    }
}

impl SupervisorHandlers for Component {
    fn handle_child_error(
        _state: Option<Vec<u8>>,
        params: (String, WitActorError),
    ) -> Result<(Option<Vec<u8>>,), String> {
        log("Handling child error in chat-state");

        let (child, error) = params;

        log(&format!(
            "Child {} encountered an error: {:?}",
            child, error
        ));

        match error {
            WitActorError {
                error_type: WitErrorType::Internal,
                data,
            } => {
                log("Internal error type");
                let error_msg = match data {
                    Some(d) => String::from_utf8_lossy(&d).to_string(),
                    None => "No error data provided".to_string(),
                };
                log(&format!("Error data: {}", error_msg));
                Err(format!(
                    "Internal error in child actor {}: {}",
                    child, error_msg
                ))
            }
            _ => {
                log("Other error type");
                let error_msg = match error.data {
                    Some(data) => {
                        log(&format!("Error data: {:?}", data));
                        String::from_utf8_lossy(&data).to_string()
                    }
                    None => "No error data provided".to_string(),
                };
                Err(format!("Error in child actor {}: {}", child, error_msg))
            }
        }
    }

    fn handle_child_exit(
        state: Option<Vec<u8>>,
        _params: (String, Option<Vec<u8>>),
    ) -> Result<(Option<Vec<u8>>,), String> {
        log("Handling child exit in chat-state");
        Ok((state,))
    }

    fn handle_child_external_stop(
        state: Option<Vec<u8>>,
        _params: (String,),
    ) -> Result<(Option<Vec<u8>>,), String> {
        log("Handling child external stop in chat-state");
        Ok((state,))
    }
}

bindings::export!(Component with_types_in bindings);
