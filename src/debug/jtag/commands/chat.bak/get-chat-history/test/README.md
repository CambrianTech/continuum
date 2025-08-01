# Chat.bakgetChatHistory Command Tests

Tests for the chat.bak/get-chat-history command following middle-out testing methodology.

## Structure

```
test/
├── unit/                     # Unit tests (isolated, mocked dependencies)
│   └── Chat.bakgetChatHistoryCommand.test.ts
├── integration/              # Integration tests (real client connections)
│   └── Chat.bakgetChatHistoryIntegration.test.ts  
└── README.md                # This file
```

## Running Tests

```bash
# Run unit tests only
npx tsx test/unit/Chat.bakgetChatHistoryCommand.test.ts

# Run integration tests only  
npx tsx test/integration/Chat.bakgetChatHistoryIntegration.test.ts

# Run all tests for this command
npm test -- --testPathPattern="chat.bak/get-chat-history"
```

## Test Coverage

### Unit Tests
- ✅ Basic command execution with mocked dependencies
- ✅ Error handling scenarios
- ✅ Performance validation
- ✅ Parameter validation

### Integration Tests  
- ✅ Real command execution through client
- ✅ Cross-environment testing (browser/server)
- ✅ End-to-end correlation and response handling
- ✅ Bootstrap session compatibility

## Command-Specific Notes

- Has browser implementation
- Has server implementation
- May need custom type definitions

For reusable testing utilities, see `commands/test/utils/`.
