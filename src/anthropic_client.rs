use crate::bindings::ntwk::theater::message_server::message_request;
use crate::bindings::ntwk::theater::runtime::log;
use crate::protocol::{AnthropicRequest, AnthropicResponse, create_anthropic_request};
use crate::state::{ChatMessage, ChatState};
use crate::utils::{calculate_total_tokens, create_truncated_message_set};
use serde_json::to_vec;

/// Sends a request to the anthropic-proxy actor and returns the response
pub fn send_to_anthropic_proxy(
    chat_state: &ChatState,
    messages: &[ChatMessage],
) -> Result<String, String> {
    log("Sending request to anthropic-proxy");
    
    // Create the Anthropic request
    let request = create_anthropic_request(
        &chat_state.conversation_id,
        messages,
        chat_state.system_prompt.clone(),
        &chat_state.settings,
    );
    
    // Serialize the request
    let request_bytes = to_vec(&request)
        .map_err(|e| format!("Error serializing Anthropic request: {}", e))?;
    
    // Send the request to the anthropic-proxy
    log(&format!("Sending request to proxy actor: {}", chat_state.anthropic_proxy_id));
    let response_bytes = message_request(
        &chat_state.anthropic_proxy_id,
        "process_request".to_string(),
        request_bytes,
    )
    .map_err(|e| format!("Error sending request to anthropic-proxy: {}", e))?;
    
    // Parse the response
    let response: AnthropicResponse = serde_json::from_slice(&response_bytes)
        .map_err(|e| format!("Error parsing Anthropic response: {}", e))?;
    
    // Handle the response
    match response.status.as_str() {
        "success" => {
            if let Some(completion) = response.completion {
                Ok(completion.content)
            } else {
                Err("No completion in successful response".to_string())
            }
        },
        "error" => Err(format!(
            "Error from Anthropic API: {}",
            response.error.unwrap_or_else(|| "Unknown error".to_string())
        )),
        _ => Err(format!("Unknown response status: {}", response.status)),
    }
}

/// Prepares messages for sending to Anthropic
/// This function handles filtering and formatting messages
pub fn prepare_messages_for_anthropic(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    // Check if we need to truncate messages
    const MAX_CONTEXT_TOKENS: u32 = 200000; // Use a large value that accounts for Claude's context window
    const MAX_RESPONSE_TOKENS: u32 = 4096;  // Default max tokens for response
    
    let total_tokens = calculate_total_tokens(messages);
    log(&format!("Total estimated tokens: {}", total_tokens));
    
    if total_tokens > (MAX_CONTEXT_TOKENS - MAX_RESPONSE_TOKENS) {
        // We need to truncate the messages
        return create_truncated_message_set(messages, MAX_CONTEXT_TOKENS, MAX_RESPONSE_TOKENS);
    }
    
    // No truncation needed
    messages.to_vec()
}

/// Generates a title for the conversation based on its content
pub fn generate_conversation_title(
    chat_state: &ChatState,
) -> Result<String, String> {
    // Simple implementation for now
    if chat_state.messages.len() < 2 {
        return Ok(format!("Conversation {}", &chat_state.conversation_id[0..8]));
    }
    
    // Extract the first few messages
    let first_messages = chat_state.messages.iter()
        .take(2)
        .map(|msg| msg.content.clone())
        .collect::<Vec<String>>()
        .join("\n");
    
    // Create a title request
    let title_request = AnthropicRequest {
        version: "1.0".to_string(),
        operation_type: "chat_completion".to_string(),
        request_id: format!("title-{}", chat_state.conversation_id),
        completion_request: Some(crate::protocol::CompletionRequest {
            model: "claude-3-5-sonnet-20250404".to_string(), // Use a faster model for titles
            messages: vec![crate::protocol::AnthropicMessage {
                role: "user".to_string(),
                content: format!("Generate a very short title (5 words or less) that captures the essence of this conversation. Only respond with the title, nothing else:\n\n{}", first_messages),
            }],
            max_tokens: Some(20),
            temperature: Some(0.7),
            system: None,
            top_p: None,
            anthropic_version: None,
            additional_params: None,
        }),
        params: None,
    };
    
    // Serialize the request
    let request_bytes = to_vec(&title_request)
        .map_err(|e| format!("Error serializing title request: {}", e))?;
    
    // Send the request to the anthropic-proxy
    log("Sending title generation request to anthropic-proxy");
    let response_bytes = message_request(
        &chat_state.anthropic_proxy_id,
        "process_request".to_string(),
        request_bytes,
    )
    .map_err(|e| format!("Error sending title request: {}", e))?;
    
    // Parse the response
    let response: AnthropicResponse = serde_json::from_slice(&response_bytes)
        .map_err(|e| format!("Error parsing title response: {}", e))?;
    
    // Extract the title
    match response.status.as_str() {
        "success" => {
            if let Some(completion) = response.completion {
                // Clean up the response
                let title = completion.content.trim().to_string();
                if title.is_empty() {
                    Ok(format!("Conversation {}", &chat_state.conversation_id[0..8]))
                } else {
                    Ok(title)
                }
            } else {
                Ok(format!("Conversation {}", &chat_state.conversation_id[0..8]))
            }
        },
        _ => Ok(format!("Conversation {}", &chat_state.conversation_id[0..8])),
    }
}
