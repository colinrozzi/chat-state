# Chat State Actor

A Theater actor that encapsulates and manages the state and behavior of a single conversation in the Claude Chat system.

## Purpose

The `chat-state` actor is responsible for maintaining conversation history, communicating with the AI model, and applying conversation-specific settings and behaviors. Each conversation in the system gets its own dedicated chat-state actor instance.

## Core Responsibilities

1. **Conversation State Management**
   - Store the complete message history
   - Maintain conversation metadata
   - Track conversation settings
   - Handle state persistence

2. **AI Model Interaction**
   - Communicate with the `anthropic-proxy` actor
   - Format messages according to the model's requirements
   - Apply system instructions and settings
   - Process model responses

3. **Message Processing**
   - Validate incoming messages
   - Format messages for the model
   - Handle message truncation or compression when needed
   - Process and format response messages

4. **Conversation Enhancement**
   - Generate conversation titles
   - Provide message timestamps
   - Handle special commands or features

## Building

To build the actor:

```bash
cargo build --target wasm32-unknown-unknown --release
```

## Running

This actor is typically not started directly but is spawned by the `chat-interface` actor when a new conversation is created. However, it can be started manually for testing:

```bash
theater start manifest.toml
```

## Message Interface

The actor accepts the following message types:
- `new_conversation` - Initialize a new conversation
- `send_message` - Process a user message and get AI response
- `update_settings` - Change conversation settings
- `get_history` - Retrieve conversation history

## Implementation Notes

- Each actor instance manages exactly one conversation
- The actor connects to an `anthropic-proxy` actor to access Claude AI
- State is periodically persisted to ensure recovery
- The actor is designed to be lightweight with minimal dependencies
