# Screenshot Command Tests

Tests for the screenshot command following middle-out testing methodology.

## Structure

```
test/
├── unit/                     # Unit tests (isolated, mocked dependencies)
│   └── ScreenshotCommand.test.ts
├── integration/              # Integration tests (real client connections)
│   └── ScreenshotIntegration.test.ts  
└── README.md                # This file
```

## Running Tests

```bash
# Run unit tests only
npx tsx test/unit/ScreenshotCommand.test.ts

# Run integration tests only  
npx tsx test/integration/ScreenshotIntegration.test.ts

# Run all tests for this command
npm test -- --testPathPattern="screenshot"
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
