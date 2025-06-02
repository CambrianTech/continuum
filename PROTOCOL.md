# Continuum AI Communication Protocol v1.0

## Overview
This protocol defines how AI agents communicate with the Continuum system through structured channels.

## Response Structure
AI responses must use these exact markers:

### Status Updates
```
[STATUS] message
```
Examples:
- `[STATUS] Checking CNN for latest news...`
- `[STATUS] Analyzing file contents...`
- `[STATUS] Searching for weather information...`

### Commands
```
[CMD:ACTION] parameters
```

### Chat Messages
```
[CHAT] message
```
OR plain text without markers (will be treated as chat)

## Available Commands

### Available Commands

- `[CMD:WEBFETCH] url` - Fetch content from any website
- `[CMD:FILE_READ] path` - Read file contents
- `[CMD:FILE_WRITE] path content` - Write content to file  
- `[CMD:EXEC] command` - Execute any shell command
- `[CMD:GIT_STATUS]` - Check git repository status
- `[HELP]` - Show available commands and protocol
- `[MAN] command` - Get help for specific command

### Intelligence Notes
The AI should be smart about using these tools:
- For weather: Use WEBFETCH with weather sites or EXEC with curl
- For news: Use WEBFETCH with news websites  
- For complex tasks: Combine multiple commands intelligently
- Think about what URLs or commands would work, don't expect hardcoded helpers

## Protocol Examples

### Example 1: Web Content Request
User: "What's on CNN right now?"

AI Response:
```
[STATUS] Checking CNN for current headlines...
[CMD:WEBFETCH] https://cnn.com
[CHAT] I'll check the latest news on CNN for you.
```

System: Executes WEBFETCH, sends callback with results

AI Callback Response:
```
Based on CNN's homepage, here are the top stories: [analysis of content]
```

### Example 2: Weather Request
User: "What's the weather like in San Francisco?"

AI Response:
```
[STATUS] Getting weather information for San Francisco...
[CMD:EXEC] curl wttr.in/San Francisco?format=3
[CHAT] Let me check the current weather in San Francisco.
```

### Example 3: File Operation
User: "Show me what's in config.json"

AI Response:
```
[STATUS] Reading config.json file...
[CMD:FILE_READ] config.json
[CHAT] I'll read the config.json file for you.
```

## Protocol Rules

1. **Channel Separation**: Commands go to command channel, status to status channel, chat to chat channel
2. **No Command Mentions in Chat**: Don't mention tool execution in chat messages
3. **Callback Handling**: System will execute commands and send results back for analysis
4. **Natural Responses**: After callbacks, provide natural analysis of results
5. **Status Updates**: Always provide status when executing commands

## Error Handling

If a command fails, the system will send an error in the callback. Respond naturally:
- Don't expose technical error details to users
- Suggest alternatives when possible
- Maintain conversational tone

## Protocol Compliance

AI agents MUST follow this protocol exactly. Non-compliant responses will be processed as legacy format but may not work correctly.