# File Command Tests

Tests for the file command following middle-out testing methodology.

## Structure

```
test/
├── unit/                     # Unit tests (isolated, mocked dependencies)
│   └── FileCommand.test.ts
├── integration/              # Integration tests (real client connections)
│   └── FileIntegration.test.ts  
└── README.md                # This file
```

## Running Tests

```bash
# Run unit tests only
npx tsx test/unit/FileCommand.test.ts

# Run integration tests only  
npx tsx test/integration/FileIntegration.test.ts

# Run all tests for this command
npm test -- --testPathPattern="file"
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
- Browser-only implementation
- Uses shared types for consistency

For reusable testing utilities, see `commands/test/utils/`.
