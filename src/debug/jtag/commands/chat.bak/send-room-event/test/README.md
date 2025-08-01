# Chat.baksendRoomEvent Command Tests

Tests for the chat.bak/send-room-event command following middle-out testing methodology.

## Structure

```
test/
├── unit/                     # Unit tests (isolated, mocked dependencies)
│   └── Chat.baksendRoomEventCommand.test.ts
├── integration/              # Integration tests (real client connections)
│   └── Chat.baksendRoomEventIntegration.test.ts  
└── README.md                # This file
```

## Running Tests

```bash
# Run unit tests only
npx tsx test/unit/Chat.baksendRoomEventCommand.test.ts

# Run integration tests only  
npx tsx test/integration/Chat.baksendRoomEventIntegration.test.ts

# Run all tests for this command
npm test -- --testPathPattern="chat.bak/send-room-event"
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
- Uses shared types for consistency

For reusable testing utilities, see `commands/test/utils/`.
