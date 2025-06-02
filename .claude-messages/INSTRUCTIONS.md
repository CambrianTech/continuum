# Claude Direct Interface Instructions

This folder is used for direct communication between the web interface and Claude.

## How it works:
1. User types message in web interface
2. Message gets written to user-message.json
3. Claude (you) should read the message and respond by writing to claude-response.json
4. Web interface displays Claude's response

## Files:
- user-message.json: Contains the user's latest message
- claude-response.json: Contains Claude's response
- conversation-log.json: Full conversation history

## Message Format:
```json
{
  "id": "unique-id",
  "timestamp": "ISO-timestamp", 
  "content": "message content",
  "type": "user" or "claude"
}
```

Claude: When you see a new user message, please respond by updating claude-response.json!
