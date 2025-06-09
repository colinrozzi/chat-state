# OpenCode Configuration

## Build Commands
- **Build**: `cargo build --target wasm32-unknown-unknown --release`
- **Test**: `cargo test` (if tests exist)
- **Lint**: `cargo clippy`
- **Format**: `cargo fmt`
- **Run**: `theater start manifest.toml`

## Code Style Guidelines
- **Language**: Rust with WebAssembly target
- **Imports**: Group std imports first, then external crates, then local modules
- **Naming**: snake_case for functions/variables, PascalCase for types/structs
- **Error Handling**: Use `Result<T, String>` for fallible operations, log errors before returning
- **Logging**: Use `log()` function from theater runtime for all debug output
- **Serialization**: Use serde with `#[derive(Serialize, Deserialize, Debug, Clone)]`
- **State Management**: All actor state must be serializable via serde_json
- **Message Handling**: Use tagged enums with `#[serde(tag = "type")]` for request/response types
- **Store Operations**: Use theater store API for persistence, always handle errors
- **Proxy Communication**: Use message_server_host for inter-actor communication
- **Tool Integration**: MCP servers for external tool access, validate tool availability before use
- **Channel Management**: Auto-subscribe/unsubscribe pattern for real-time updates