use crate::bindings::ntwk::theater::runtime::log;
use crate::bindings::ntwk::theater::timing;
use crate::state::{ChatMessage, ChatState};

/// Get the current timestamp in seconds
pub fn current_timestamp() -> Result<u64, String> {
    Ok(timing::now())
}

/// Calculate the estimated token count for a message
pub fn estimate_token_count(text: &str) -> u32 {
    // Very rough estimate: ~4 characters per token for English text
    (text.len() as f32 / 4.0).ceil() as u32
}

/// Prepare messages for sending to the Anthropic API
/// This function handles filtering and preparing messages
pub fn prepare_messages_for_anthropic(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    // In a more sophisticated implementation, we would handle:
    // - Token counting and truncation
    // - Filtering out system messages that are handled separately
    // - Reformatting messages if needed
    
    // For now, just return all messages
    messages.to_vec()
}

/// Generate a title for a conversation based on its content
pub fn generate_conversation_title(chat_state: &ChatState) -> Result<String, String> {
    // Only attempt to generate a title if we have at least 2 messages
    if chat_state.messages.len() < 2 {
        return Ok(format!("Conversation {}", &chat_state.conversation_id[0..8]));
    }
    
    // Simple implementation - use the first part of the first user message
    if let Some(first_message) = chat_state.messages.iter().find(|m| m.role == "user") {
        let content = &first_message.content;
        if content.len() > 30 {
            return Ok(format!("{}..", &content[0..30]));
        } else {
            return Ok(content.clone());
        }
    }
    
    // Fallback to default title
    Ok(format!("Conversation {}", &chat_state.conversation_id[0..8]))
    
    // NOTE: For a more sophisticated implementation, we could use the anthropic-proxy
    // to generate a title based on the conversation content.
}

/// Estimate the number of tokens in a message
pub fn estimate_message_tokens(message: &ChatMessage) -> u32 {
    // A very simple estimation - in a real implementation, 
    // you would use a proper tokenizer or the anthropic-proxy
    
    // Rough estimate: 1 token ≈ 4 characters
    let content_tokens = estimate_token_count(&message.content);
    
    // Add overhead for message structure
    let role_tokens = 5; // Rough estimate for role encoding
    let metadata_tokens = 10; // Overhead for message metadata
    
    role_tokens + content_tokens + metadata_tokens
}

/// Calculate the total estimated tokens in a set of messages
pub fn calculate_total_tokens(messages: &[ChatMessage]) -> u32 {
    messages.iter().map(estimate_message_tokens).sum()
}

/// Create a truncated message set that fits within token limits
pub fn create_truncated_message_set(
    messages: &[ChatMessage], 
    max_context_tokens: u32,
    reserved_tokens: u32
) -> Vec<ChatMessage> {
    let available_tokens = max_context_tokens.saturating_sub(reserved_tokens);
    let mut result = Vec::new();
    let mut token_count = 0;
    
    // Always include the most recent messages
    // Start from the end and work backwards
    for message in messages.iter().rev() {
        let message_tokens = estimate_message_tokens(message);
        
        if token_count + message_tokens <= available_tokens {
            result.push(message.clone());
            token_count += message_tokens;
        } else {
            // We can't fit any more messages
            break;
        }
    }
    
    // Reverse the result to maintain chronological order
    result.reverse();
    
    result
}

/// Log detailed information about errors when communicating with anthropic-proxy
pub fn log_anthropic_error(error: &str, context: &str) {
    log(&format!("ANTHROPIC ERROR [{}]: {}", context, error));
}
