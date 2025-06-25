# Command Processor Module

🚀 **Modern TypeScript command execution system** - Replaces legacy CommandRegistry.cjs with full type safety, embedded testing, and case-insensitive handling.

## 🎯 Purpose

This module provides a complete command processing system that:
- **Discovers TypeScript commands automatically** from configured directories
- **Executes commands with proper context** and error handling
- **Handles case-insensitive command lookup** (no more UPPERCASE annoyance!)
- **Validates all functionality** through embedded unit and integration tests

## 📦 Module Structure

```
src/modules/command-processor/
├── README.md                    # This documentation
├── CommandProcessor.ts          # Main processor class
├── types.ts                     # TypeScript interfaces
├── tests/                       # Test organization
│   ├── unit.test.ts            # Unit tests
│   └── integration.test.ts     # Integration tests
└── index.ts                    # Module exports
```

## 🚀 Quick Start

```typescript
import { CommandProcessor } from './command-processor';

const processor = new CommandProcessor({
  enableCaseInsensitive: true,  // Default: true
  logLevel: 'info'
});

await processor.initialize();

// Execute any command - case doesn't matter!
const result = await processor.executeCommand('INFO', { section: 'overview' });
const result2 = await processor.executeCommand('status_text', { text: 'Working...' });
```

## 🧪 Testing

The module includes comprehensive embedded tests:

```bash
# Run all tests
npx tsx src/modules/command-processor/CommandProcessor.ts

# Or import and run programmatically
import { CommandProcessorTests } from './command-processor';
await CommandProcessorTests.runAllTests();
```

## 🎯 Features

### ✅ Case-Insensitive Command Lookup
```typescript
// All of these work identically:
await processor.executeCommand('info', params);
await processor.executeCommand('INFO', params);
await processor.executeCommand('Info', params);
await processor.executeCommand('iNfO', params);
```

### ✅ Automatic Command Discovery
```typescript
// Scans these directories automatically:
const defaultDirs = [
  'src/commands/core',
  'src/commands/ui', 
  'src/commands/browser',
  'src/commands/file',
  // ... more
];
```

### ✅ Proper Context Passing
```typescript
const context = {
  continuum: continuumInstance,
  continuonStatus: statusInstance,
  processor: 'typescript',
  executionId: 'unique-id',
  timestamp: new Date()
};

const result = await processor.executeCommand('command', params, context);
```

### ✅ Error Handling & Reporting
```typescript
const result = await processor.executeCommand('broken-command', {});
// Returns: { success: false, message: "...", error: "...", timestamp: "..." }
```

## 📊 Statistics & Monitoring

```typescript
const stats = processor.getStats();
// Returns: command counts, categories, loaded commands

const commands = processor.getAllCommands();
const categories = processor.getCategories();
const definition = processor.getDefinition('command-name');
```

## 🔧 Configuration

```typescript
interface ProcessorConfig {
  commandDirs: string[];           // Directories to scan
  enableCaseInsensitive: boolean;  // Default: true
  enableTypeScriptOnly: boolean;   // Default: false
  logLevel: 'debug' | 'info' | 'warn' | 'error';
}
```

## 🎯 Integration Examples

### Replace Legacy CommandRegistry
```typescript
// Old way (CommandRegistry.cjs)
const registry = new CommandRegistry();
await registry.initPromise;
const command = registry.getCommand('INFO');  // Forced uppercase
const result = await command(params, continuum);

// New way (CommandProcessor.ts)
const processor = new CommandProcessor();
await processor.initialize();
const result = await processor.executeCommand('info', params, context);  // Case insensitive!
```

### Portal Integration
```typescript
// In your portal/API layer:
import { commandProcessor } from './modules/command-processor';

export async function executeCommand(name: string, params: any, context?: any) {
  return await commandProcessor.executeCommand(name, params, context);
}
```

## 🧪 Test Coverage

- ✅ **Unit Tests**: Processor initialization, case handling, context passing, error handling
- ✅ **Integration Tests**: Real command discovery, actual TypeScript command execution
- ✅ **Embedded Testing**: Tests live alongside code for immediate validation

## 🚀 Migration Path

1. **Phase 1**: Run alongside CommandRegistry.cjs for gradual migration
2. **Phase 2**: Route TypeScript commands through this processor
3. **Phase 3**: Mass-swap all commands to TypeScript
4. **Phase 4**: Remove CommandRegistry.cjs entirely

## 📈 Performance

- **Lazy initialization**: Only scans directories when needed
- **Cached commands**: Commands loaded once and reused
- **Minimal overhead**: Direct TypeScript execution without compilation layers

## 🔍 Debugging

Enable debug logging to see command discovery and execution:

```typescript
const processor = new CommandProcessor({ logLevel: 'debug' });
// Shows: command loading, execution traces, error details
```

## 🎯 Goals Achieved

- ✅ **No more legacy CJS pain**: Pure TypeScript architecture
- ✅ **Case-insensitive by default**: Handles command lookup properly  
- ✅ **Self-testing**: Embedded unit and integration tests
- ✅ **Modular design**: Clean namespace and folder organization
- ✅ **Ready for mass-swap**: Can replace all legacy command handling

---

*This module represents the future of command processing in Continuum - elegant, tested, and TypeScript-first.* 🚀