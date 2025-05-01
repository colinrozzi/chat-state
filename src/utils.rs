use crate::bindings::ntwk::theater::timing;

/// Get the current timestamp in seconds
pub fn current_timestamp() -> Result<u64, String> {
    Ok(timing::now())
}

/// Calculate the estimated token count for a message
pub fn estimate_token_count(text: &str) -> u32 {
    // Very rough estimate: ~4 characters per token for English text
    (text.len() as f32 / 4.0).ceil() as u32
}
