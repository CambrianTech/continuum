# Command Testing Utilities

Reusable testing patterns for all JTAG commands following middle-out methodology.

## 🏗️ Architecture

```
commands/test/
├── utils/                    # Reusable testing utilities
│   ├── CommandTestUtils.ts   # Command execution testing patterns
│   ├── ClientTestUtils.ts    # Client connection testing patterns  
│   ├── MockUtils.ts          # Mock objects and test data
│   └── AssertionUtils.ts     # Custom assertion helpers
├── fixtures/                 # Test data and fixtures
│   ├── environments/         # Environment-specific test data
│   └── payloads/            # Sample command payloads
└── README.md                # This file
```

## 📋 Usage Pattern

Every command follows this structure:

```
commands/[command-name]/
├── test/
│   ├── unit/                # Unit tests (isolated)
│   │   └── [Command].test.ts
│   └── integration/         # Integration tests (with dependencies)
│       └── [Command].integration.test.ts
├── browser/
├── server/
└── shared/
```

## 🧪 Test Types

### Unit Tests
- Test command logic in isolation
- Mock all external dependencies
- Fast execution, no network/filesystem

### Integration Tests  
- Test command with real client connections
- Test browser ↔ server communication
- Test end-to-end command execution

## 🔄 Reusable Patterns

All commands can use the same testing utilities for:
- Client connection testing
- Environment data validation
- Error handling scenarios
- Correlation system testing
- Bootstrap session handling