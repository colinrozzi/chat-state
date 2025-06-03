use crate::bindings::theater::simple::message_server_host;
use crate::bindings::theater::simple::runtime::log;
use crate::bindings::theater::simple::supervisor::spawn;
use genai_types::{CompletionRequest, CompletionResponse, ProxyRequest, ProxyResponse};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Proxy {
    name: String,
    actor_id: String,
}

impl Proxy {
    pub fn new(name: &str, manifest_path: &str) -> Result<Self, String> {
        // Spawn the proxy actor using the manifest path
        let actor_id = spawn(manifest_path, None)
            .map_err(|e| format!("Failed to spawn proxy actor: {}", e))?;

        Ok(Proxy {
            name: name.to_string(),
            actor_id,
        })
    }

    /// Sends a request to the anthropic-proxy actor and returns the response
    pub fn send_to_proxy(&self, request: ProxyRequest) -> Result<ProxyResponse, String> {
        log(&format!("Sending request to proxy actor: {}", self.name));

        // Serialize the request
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| format!("Error serializing Anthropic request: {}", e))?;

        let response_bytes = message_server_host::request(&self.actor_id, &request_bytes)
            .map_err(|e| format!("Error sending request to anthropic-proxy: {}", e))?;

        // Parse the response
        let response: ProxyResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| format!("Error parsing Anthropic response: {}", e))?;

        Ok(response)
    }
}
