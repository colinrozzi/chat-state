# Chat State Actor Design Document

## Overview

The `chat-state` actor encapsulates and manages the state and behavior of a single conversation in the Claude Chat system. It's responsible for maintaining conversation history, communicating with the AI model via the `anthropic-proxy` actor, and applying conversation-specific settings and behaviors.

## Architecture

```
chat-interface <----> chat-state (one per conversation) <----> anthropic-proxy <----> Anthropic API
```

## Core Responsibilities

### Conversation State Management
- Store the complete message history
- Maintain conversation metadata (title, timestamps, etc.)
- Track conversation settings (model, parameters)
- Handle state persistence for recovery

### AI Model Interaction
- Communicate with the `anthropic-proxy` actor
- Format messages according to the model's requirements
- Apply system instructions and conversation settings
- Process and validate model responses

### Message Processing
- Validate incoming messages for format and content
- Format the conversation history for the model context
- Handle message truncation or compression when needed
- Process and format response messages for the client

### Conversation Enhancement
- Generate meaningful conversation titles
- Provide accurate message timestamps
- Handle special commands or features (e.g., /clear, /help)
- Support conversation-specific customizations

## State Structure

```rust
struct ChatState {
    // Basic information
    conversation_id: String,
    title: String,
    created_at: u64,
    updated_at: u64,
    
    // Actor references
    parent_interface_id: String,
    anthropic_proxy_id: String,
    
    // Conversation content
    system_prompt: Option<String>,
    messages: Vec<ChatMessage>,
    
    // Conversation settings
    settings: ConversationSettings,
}

struct ChatMessage {
    id: String,
    role: String,  // "user" or "assistant"
    content: String,
    timestamp: u64,
    metadata: Option<MessageMetadata>,
}

struct MessageMetadata {
    token_count: Option<u32>,
    response_time_ms: Option<u64>,
    // Additional message-specific metadata
}

struct ConversationSettings {
    model: String,
    temperature: f32,
    max_tokens: u32,
    top_p: Option<f32>,
    // Other model parameters
}
```

## Message Interface

### Incoming Messages

#### New Conversation

```rust
struct NewConversationMessage {
    conversation_id: String,
    system_prompt: Option<String>,
    settings: Option<ConversationSettings>,
    parent_interface_id: String,
}
```

#### User Message

```rust
struct UserMessageRequest {
    message: String,
    override_settings: Option<ConversationSettings>,
}
```

#### Settings Update

```rust
struct UpdateSettingsRequest {
    settings: ConversationSettings,
}
```

#### History Request

```rust
struct HistoryRequest {
    limit: Option<u32>,
    before_timestamp: Option<u64>,
}
```

### Outgoing Messages

#### Message Response

```rust
struct MessageResponse {
    message_id: String,
    content: String,
    role: String,
    timestamp: u64,
    finished: bool,
}
```

#### History Response

```rust
struct HistoryResponse {
    messages: Vec<ChatMessage>,
    has_more: bool,
}
```

## Interaction Flows

### Conversation Initialization

1. Receive initialization parameters from `chat-interface`
2. Set up state with conversation ID and initial settings
3. Connect to the `anthropic-proxy` actor
4. Generate a default title or use provided title
5. Return success confirmation to `chat-interface`

### Message Processing

1. Receive user message from `chat-interface`
2. Validate and add the message to history
3. Update the conversation timestamp
4. Prepare the complete message history for the model
   - Apply truncation if needed
   - Format with system prompt
5. Send request to `anthropic-proxy` with appropriate settings
6. Process the response from the model
7. Add the assistant's response to the history
8. Return the formatted response to `chat-interface`

### Settings Update

1. Receive settings update from `chat-interface`
2. Validate the new settings
3. Update the conversation settings
4. Return confirmation to `chat-interface`

### History Retrieval

1. Receive history request from `chat-interface`
2. Retrieve requested portion of message history
3. Format and return the messages to `chat-interface`

## Advanced Features

### Conversation Title Generation

1. After the first few messages, use the model to generate a title
2. Update the conversation metadata with the generated title
3. Notify the `chat-interface` of the new title

### Context Management

1. Track token usage in the conversation
2. When approaching context limits, implement truncation strategies:
   - Remove oldest messages
   - Summarize older parts of the conversation
   - Keep important context (system prompt, recent messages)

### Error Recovery

1. Handle failed model requests gracefully
2. Implement retry logic with backoff
3. Provide informative errors to the user via `chat-interface`

## Lifecycle Management

### Initialization

1. Created by `chat-interface` when a new conversation is started
2. Sets up initial state and connects to `anthropic-proxy`
3. Confirms successful initialization to `chat-interface`

### Persistence

1. Periodically save state to persistent storage
2. Implement checkpointing for recovery
3. Handle state migrations if needed

### Termination

1. Receive termination signal from `chat-interface`
2. Perform final state persistence
3. Clean up resources
4. Confirm termination to `chat-interface`

## Error Handling

### API Communication Errors
- Handle connection failures to `anthropic-proxy`
- Implement retries with exponential backoff
- Provide clear error messages to users

### Context Management Errors
- Detect and handle token limit issues
- Implement graceful degradation when limits are reached
- Provide feedback about truncation when it occurs

### State Persistence Errors
- Handle storage failures gracefully
- Keep backup copies of state when possible
- Implement recovery mechanisms

## Future Extensions

### Enhanced Context Management
- Semantic compression of conversation history
- Selective memory management based on importance
- Long-term vs. short-term memory separation

### Conversation Analysis
- Topic extraction and tracking
- Sentiment analysis on conversation
- Key points summarization for long conversations

### Multi-Model Support
- Switch between different AI models
- Specialized models for different conversation stages
- Model fallback strategies

### Tool Integration
- Web search capability
- Document analysis and reference
- Code execution and evaluation

## Implementation Notes

- Design for efficient state persistence
- Optimize message handling for larger conversations
- Carefully manage token limits and context windows
- Implement clear error reporting
- Structure for future extensibility
