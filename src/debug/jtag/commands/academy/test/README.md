# Academy Command Tests

Tests for the academy command following middle-out testing methodology.

## Structure

```
test/
├── unit/                     # Unit tests (isolated, mocked dependencies)
│   └── AcademyCommand.test.ts
├── integration/              # Integration tests (real client connections)
│   └── AcademyIntegration.test.ts  
└── README.md                # This file
```

## Running Tests

```bash
# Run unit tests only
npx tsx test/unit/AcademyCommand.test.ts

# Run integration tests only  
npx tsx test/integration/AcademyIntegration.test.ts

# Run all tests for this command
npm test -- --testPathPattern="academy"
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
- May need custom type definitions

For reusable testing utilities, see `commands/test/utils/`.
