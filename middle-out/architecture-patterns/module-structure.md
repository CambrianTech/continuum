# Module Structure Pattern

## Overview

Continuum uses a **module-first architecture** where each module is self-contained with its own shared/client/server structure and inheritance hierarchy.

## Pattern Structure

```
src/[module]/
├── shared/
│   ├── Base[Module].ts         # Abstract base class
│   ├── [Module]Types.ts        # Shared types
│   └── [Module]Registry.ts     # Registration system
├── client/
│   └── Client[Module].ts       # Client-specific base (extends Base[Module])
├── server/
│   └── Server[Module].ts       # Server-specific base (extends Base[Module])
└── integrations/
    └── [integration-name]/
        ├── shared/
        │   └── [Integration]Types.ts
        ├── client/
        │   └── [Integration]Client[Module].ts    # extends Client[Module]
        └── server/
            └── [Integration]Server[Module].ts    # extends Server[Module]
```

## Current Module Examples

### Parsers Module (Reference Implementation)

```
src/parsers/
├── shared/
│   ├── ParserBase.ts
│   └── ValidationTypes.ts
├── client/
│   └── ClientParser.ts (optional)
├── server/
│   └── ServerParser.ts (optional)
└── integrations/
    └── cli-parser/
        ├── shared/
        │   └── CLIParserTypes.ts
        ├── client/
        │   └── CLIClientParser.ts (extends ParserBase)
        └── server/
            └── CLIServerParser.ts (extends ParserBase)
```

### Daemons Module (Planned)

```
src/daemons/
├── shared/
│   ├── BaseDaemon.ts
│   ├── DaemonTypes.ts
│   └── DaemonRegistry.ts
├── client/
│   └── ClientDaemon.ts (extends BaseDaemon)
├── server/
│   └── ServerDaemon.ts (extends BaseDaemon)
└── integrations/
    ├── websocket/
    │   ├── shared/
    │   │   └── WebSocketTypes.ts
    │   ├── client/
    │   │   └── WebSocketClientDaemon.ts (extends ClientDaemon)
    │   └── server/
    │       └── WebSocketServerDaemon.ts (extends ServerDaemon)
    └── session-manager/
        ├── shared/
        └── server/
            └── SessionManagerDaemon.ts (extends ServerDaemon)
```

### Commands Module (Existing Example)

```
src/commands/
├── shared/
│   ├── BaseCommand.ts          # Already exists
│   └── CommandTypes.ts         # Already exists
├── browser/
│   └── screenshot/
│       ├── shared/
│       │   └── ScreenshotTypes.ts
│       ├── client/
│       │   └── ScreenshotClient.ts
│       └── server/
│           └── ScreenshotCommand.ts
└── file/
    └── fileSave/
        ├── shared/
        │   ├── FileTypes.ts
        │   └── FileValidator.ts
        ├── client/
        │   └── FileSaveClient.ts
        └── server/
            └── FileSaveCommand.ts
```

### API Module (New Universal Pattern)

```
src/api/
└── continuum/
    ├── shared/
    │   ├── ContinuumClient.ts      # Base client interface
    │   ├── ContinuumTypes.ts       # Shared types
    │   └── ContinuumValidator.ts   # Shared validation
    ├── client/
    │   └── ContinuumBrowserClient.ts # Browser-specific (~100 lines)
    ├── server/
    │   └── ContinuumServerClient.ts  # Server-specific (~80 lines)
    └── README.md
```

## Inheritance Pattern

Each module follows a consistent inheritance hierarchy:

```typescript
// Base abstract class
abstract class Base[Module]<TInput, TOutput> {
  abstract method1(): void;
  abstract method2(): TOutput;
  
  // Common utilities
  protected commonUtility(): void { ... }
}

// Client-specific base (optional)
abstract class Client[Module]<TInput, TOutput> extends Base[Module]<TInput, TOutput> {
  // Client-specific common logic
  protected clientSpecificMethod(): void { ... }
}

// Server-specific base (optional)
abstract class Server[Module]<TInput, TOutput> extends Base[Module]<TInput, TOutput> {
  // Server-specific common logic
  protected serverSpecificMethod(): void { ... }
}

// Integration implementations
class CLIClientParser extends Base[Module]<CLIInput, CLIOutput> {
  // OR extends ClientParser if client-specific base exists
}
```

## Design Principles

### 1. Module-First Organization
- Each module is self-contained
- No cross-module dependencies in shared code
- Clear module boundaries and responsibilities

### 2. Shared-by-Default
- Most code is shared between client and server
- Only split when genuinely different implementations needed
- Types are inherently shared

### 3. Consistent Inheritance
- Every module has a Base[Module] abstract class
- Client/Server base classes only when needed
- Integration implementations extend appropriate base

### 4. Optional Intermediate Layers
- Client[Module] and Server[Module] are optional
- Use only when multiple integrations share client/server logic
- Direct inheritance from Base[Module] is preferred when possible

## Migration Strategy

### Current State (Mixed)
- Some modules follow pattern (commands/browser/screenshot)
- Some scattered in src/shared/, src/types/
- Inconsistent organization

### Migration Approach
1. **New modules** follow the pattern from start (parsers)
2. **Existing modules** migrate gradually
3. **Shared dumps** (`src/shared/`, `src/types/`) get distributed to proper modules
4. **No breaking changes** during migration

## Benefits

- **Predictable structure** - Every module follows same pattern
- **Clear inheritance** - Easy to understand class relationships
- **Testable modules** - Each module can be tested independently
- **Scalable architecture** - Pattern scales to any number of modules
- **Type safety** - Consistent typing across all modules

## Code Compaction Through Elegant Abstraction

### **Massive Reduction in Code Volume**

**Before Modular Pattern** (scattered, duplicated):
```
src/ui/continuum-browser-client/ContinuumBrowserClient.ts     # 386 lines
src/ui/continuum-server-client/ContinuumServerClient.ts       # ~300 lines
src/commands/browser/screenshot/ScreenshotCommand.ts          # 368 lines
src/commands/file/fileSave/FileSaveCommand.ts                 # 152 lines
```

**After Modular Pattern** (shared abstractions):
```
src/api/continuum/shared/ContinuumClient.ts                   # ~50 lines (interface)
src/api/continuum/client/ContinuumBrowserClient.ts            # ~100 lines (browser-specific)
src/api/continuum/server/ContinuumServerClient.ts             # ~80 lines (server-specific)

src/commands/browser/screenshot/shared/ScreenshotTypes.ts     # ~30 lines
src/commands/browser/screenshot/client/ScreenshotClient.ts    # ~100 lines
src/commands/browser/screenshot/server/ScreenshotCommand.ts   # ~80 lines
```

### **Abstraction Benefits**

The shared base eliminates:
- ✅ **Duplicate validation logic** - Single source of truth
- ✅ **Duplicate parameter processing** - Shared interface contracts
- ✅ **Duplicate response formatting** - Consistent output patterns
- ✅ **Duplicate error handling** - Unified error management
- ✅ **Duplicate type definitions** - DRY principle in practice

**Code compression ratio**: ~40% reduction through smart abstraction layers

### **Maintenance Benefits**

- **Surgical Changes**: Modify shared behavior once, propagates everywhere
- **Type Safety**: Compile-time validation of interface contracts
- **Testing Efficiency**: Test shared logic once, integration-specific tests separately
- **Onboarding Speed**: Consistent patterns reduce learning curve
- **Refactoring Safety**: Clear boundaries prevent accidental breakages

## Sparse Override Pattern - Centralization of Burden

### **Three-Layer Test Architecture**

```
src/api/continuum/
├── shared/
│   └── ContinuumClient.test.ts     # Core contract & business logic
├── client/
│   └── ContinuumBrowserClient.test.ts  # WebSocket, DOM, browser APIs
└── server/
    └── ContinuumServerClient.test.ts   # File system, process, daemon routing
```

### **Burden Distribution Strategy**

**Shared Base** (80-90% of complexity):
```typescript
// shared/ContinuumClient.ts - Heavy lifting (200+ lines)
abstract class ContinuumClient {
  validateParams() { /* complex validation */ }
  processResponse() { /* heavy processing */ }
  handleErrors() { /* comprehensive error handling */ }
  formatOutput() { /* standardized formatting */ }
}
```

**Client Override** (5-10% - sparse, specific):
```typescript
// client/ContinuumBrowserClient.ts - Sparse overrides (~30 lines)
class ContinuumBrowserClient extends ContinuumClient {
  protected transport = 'websocket';
  protected sendMessage(msg) { this.ws.send(msg); }
}
```

**Server Override** (5-10% - sparse, specific):
```typescript
// server/ContinuumServerClient.ts - Sparse overrides (~25 lines)  
class ContinuumServerClient extends ContinuumClient {
  protected transport = 'direct';
  protected sendMessage(msg) { this.daemon.route(msg); }
}
```

### **Testing Layer Responsibilities**

**✅ Shared Tests (Integration-Agnostic):**
- Interface contract validation
- Parameter normalization
- Error handling patterns
- Type safety guarantees

**✅ Client Tests (Browser-Specific):**
- WebSocket connection handling
- DOM interaction edge cases
- Browser API quirks (localStorage, cookies)
- Cross-browser compatibility

**✅ Server Tests (Server-Specific):**
- File system operations
- Process management
- Daemon communication
- Security validation

### **Centralization Benefits**

- **Single source of truth** for complex logic
- **Minimal surface area** for bugs in overrides
- **Easy maintenance** - fix/enhance in one place
- **Consistent behavior** across all environments
- **Reduced cognitive load** - most files are thin transport adapters
- **Testing pyramid** - test core logic once, environment quirks separately

## Usage Example

```typescript
// Create new integration
import { ParserBase } from '../../shared/ParserBase';
import { ValidationResult } from '../../shared/ValidationTypes';

export class MCPClientParser extends ParserBase<MCPInput, MCPOutput> {
  parseInput(input: MCPInput): Record<string, any> {
    // Implementation
  }
  
  formatOutput(output: MCPOutput, command: string): void {
    // Implementation
  }
  
  // ... other required methods
}
```

This pattern ensures consistency, maintainability, and clear architectural boundaries across all Continuum modules.