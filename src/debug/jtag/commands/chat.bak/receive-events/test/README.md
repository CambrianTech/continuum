# Chat.bakreceiveEvents Command Tests

Tests for the chat.bak/receive-events command following middle-out testing methodology.

## Structure

```
test/
├── unit/                     # Unit tests (isolated, mocked dependencies)
│   └── Chat.bakreceiveEventsCommand.test.ts
├── integration/              # Integration tests (real client connections)
│   └── Chat.bakreceiveEventsIntegration.test.ts  
└── README.md                # This file
```

## Running Tests

```bash
# Run unit tests only
npx tsx test/unit/Chat.bakreceiveEventsCommand.test.ts

# Run integration tests only  
npx tsx test/integration/Chat.bakreceiveEventsIntegration.test.ts

# Run all tests for this command
npm test -- --testPathPattern="chat.bak/receive-events"
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

- Server-only implementation
- Has server implementation
- Uses shared types for consistency

For reusable testing utilities, see `commands/test/utils/`.
