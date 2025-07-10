# JTAG CLI Module

**🛸 AI Autonomous Debugging Tool** - Clean, modular command delegation for browser debugging.

## 🚀 Usage

### CLI Interface
```bash
# Direct usage via root jtag.ts
./jtag.ts screenshot
./jtag.ts probe widgets
./jtag.ts logs
./jtag.ts health
```

### Programmatic Usage
```typescript
import { JtagCLI } from './JtagCLI';

const jtag = new JtagCLI({
  continuumBinary: './continuum',
  sessionsPath: '.continuum/sessions'
});

// Execute commands
await jtag.screenshot('saved-personas', 2.0);
await jtag.probe('widgets');
await jtag.logs();
await jtag.health();
```

## ⚙️ Configuration

```typescript
interface JtagConfig {
  continuumBinary?: string;  // Path to continuum binary (default: './continuum')
  sessionsPath?: string;     // Path to sessions directory (default: '.continuum/sessions')
}
```

## 🧪 Testing

```bash
# Run all tests
npm test

# Run specific test types
npm run test:unit
npm run test:integration

# Test from project root
npx tsx src/commands/development/jtag-cli/test/unit/JtagCLI.test.ts
npx tsx src/commands/development/jtag-cli/test/integration/JtagCLI.integration.test.ts
```

## 🔍 Commands

| Command | Description | Example |
|---------|-------------|---------|
| `screenshot` | Capture browser screenshot with targeting | `jtag.screenshot('saved-personas', 2.0)` |
| `probe` | Analyze widgets, shadowDOM, health | `jtag.probe('widgets')` |
| `logs` | Show recent browser logs | `jtag.logs()` |
| `errors` | Show recent browser errors | `jtag.errors()` |
| `warnings` | Show recent browser warnings | `jtag.warnings()` |
| `health` | Check system health | `jtag.health()` |
| `session` | Show current session info | `jtag.session()` |
| `hotreload` | Rebuild and reload browser | `jtag.hotreload()` |
| `help` | Show help information | `jtag.help()` |

## 🏗️ Architecture

This module follows the Continuum modular architecture:

- **Self-contained**: Complete module with package.json and tests
- **Command delegation**: Uses spawn() to delegate to continuum binary
- **Portal pattern**: Same delegation approach as Python portal client
- **Clean interface**: Simple, consistent API across all commands
- **Error handling**: Graceful failure handling with logging

## 🎯 Design Principles

1. **Delegation over Implementation** - Delegate complex logic to proper command system
2. **Portal Pattern** - Follow proven patterns from Python portal client  
3. **Clean API** - Simple, predictable method signatures
4. **Graceful Failure** - Commands log failures but don't throw exceptions
5. **Testable** - Clear separation of concerns enables proper testing

## 📦 Module Structure
```
jtag-cli/
├── JtagCLI.ts              # Main implementation
├── test/
│   ├── unit/               # Unit tests
│   │   └── JtagCLI.test.ts
│   └── integration/        # Integration tests
│       └── JtagCLI.integration.test.ts
├── package.json            # Module configuration
└── README.md              # This file
```

## 🔧 Development

### Adding New Commands
1. Add method to JtagCLI class
2. Add unit test in `test/unit/JtagCLI.test.ts`
3. Add integration test in `test/integration/JtagCLI.integration.test.ts`
4. Update help text and README

### Testing Strategy
- **Unit tests**: Test class methods and configuration
- **Integration tests**: Test actual command execution with continuum binary
- **Error handling**: Ensure graceful failure in all scenarios

## 🎉 Benefits

- **75% less code** than previous implementation (180 lines vs 500+ lines)
- **Portal-style delegation** - Proven pattern from Python client
- **Complete test coverage** - Unit and integration tests
- **Modular architecture** - Self-contained with proper package.json
- **Clean API** - Easy to use programmatically or via CLI
- **Graceful failure** - Robust error handling throughout