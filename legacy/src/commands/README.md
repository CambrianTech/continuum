# JTAG Command Development Guide

**How to write commands correctly - the elegant, simple way**

## 🎯 Core Philosophy

Commands are **self-routing** and **composable**. Each command knows its natural environment and automatically delegates when needed.

### The Golden Rules

1. **Look at your file path** - Are you in `/browser/`, `/server/`, or `/shared/`?
2. **Think about what you need** - DOM access, file system, database, etc.
3. **Just call it** - `await this.executeCommand('other/command', params)`
4. **Let commands route themselves** - Don't overthink the plumbing

## 🏗️ Universal Command Structure

```
commands/{command-name}/
├── shared/
│   ├── {CommandName}Types.ts        # Types and interfaces
│   ├── {CommandName}Command.ts      # Base class with shared logic (optional)
│   └── {CommandName}Validator.ts    # Validation logic (optional)
├── browser/
│   └── {CommandName}BrowserCommand.ts   # Browser implementation
├── server/
│   └── {CommandName}ServerCommand.ts    # Server implementation
└── README.md                            # Command-specific docs
```

**Sparse Override Pattern**: 80-90% shared logic, 5-10% environment-specific

## 📋 Simple Command Examples

### File Commands - Server-Native
```typescript
// server/FileSaveServerCommand.ts
async execute(params: FileSaveParams): Promise<FileSaveResult> {
  // I'm in server - just do the work
  const result = await fs.writeFile(params.filepath, params.content);
  return { success: true, filepath: result.filepath };
}

// browser/FileSaveBrowserCommand.ts  
async execute(params: FileSaveParams): Promise<FileSaveResult> {
  // I'm in browser but need server - delegate
  return await this.remoteExecute(params);
}
```

### Screenshot Commands - Browser-Native with Composition
```typescript
// browser/ScreenshotBrowserCommand.ts
async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
  // I'm in browser - capture the DOM
  const imageData = await this.captureDOM(params);
  
  if (params.filename) {
    // Need to save file - call file/save command
    await this.executeCommand('file/save', {
      filepath: `screenshots/${params.filename}`,
      content: imageData
    });
    return { success: true, saved: params.filename };
  }
  
  // Just return bytes
  return { success: true, bytes: imageData };
}

// server/ScreenshotServerCommand.ts
async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
  if (!params.dataUrl) {
    // Need browser for capture - delegate
    return await this.remoteExecute(params);
  }
  
  // I have data and I'm in server - save it
  return await this.executeCommand('file/save', {
    filepath: params.filepath,
    content: Buffer.from(params.dataUrl, 'base64')
  });
}
```

### Database Commands - Server-Native
```typescript
// server/DataCreateServerCommand.ts
async execute(params: DataCreateParams): Promise<DataCreateResult> {
  // I'm in server - just do database work
  const result = await this.database.create(params.collection, params.data);
  return { success: true, id: result.id };
}

// browser/DataCreateBrowserCommand.ts
async execute(params: DataCreateParams): Promise<DataCreateResult> {
  // I'm in browser but need server - delegate  
  return await this.remoteExecute(params);
}
```

## 🧠 Mental Model

### Before You Code - Ask Yourself:
1. **Where am I?** (`/browser/`, `/server/`, `/shared/`)
2. **What do I need?** (DOM, files, database, other commands)
3. **Can I do it here?** (Am I in the right environment?)

### If Yes - Do the Work:
```typescript
async execute(params: MyParams): Promise<MyResult> {
  // Do the actual work
  const result = await this.doTheWork(params);
  return result;
}
```

### If No - Delegate:
```typescript
async execute(params: MyParams): Promise<MyResult> {
  // Wrong environment - let the right one handle it
  return await this.remoteExecute(params);
}
```

### If Hybrid - Compose:
```typescript
async execute(params: MyParams): Promise<MyResult> {
  // Do what I can, call others for what I can't
  const data = await this.doMyPart(params);
  
  if (params.needsOtherThing) {
    await this.executeCommand('other/command', { data });
  }
  
  return { success: true, data };
}
```

## 🎨 Type Safety - Rust-Like Strict Typing

### Generic vs Concrete Types - The Design Decision

**Ask yourself: "Does this work with infinite entity types, or just specific ones?"**

#### ✅ USE Generics `<T extends BaseEntity>` For Infrastructure

**Data layer commands** that work with ANY entity type:

```typescript
// ✅ CORRECT: Generic data operation (data/read, data/list, data/update)
export class DataReadServerCommand<T extends BaseEntity> extends CommandBase<DataReadParams, T> {
  async execute(params: DataReadParams): Promise<T> {
    // Works with infinite entity types - just reads from collection
    const entity = await this.adapter.read(params.collection, params.id);
    return entity as T;
  }
}

// Caller gets type safety:
const user = await DataReadCommand.execute<UserEntity>({ collection: 'users', id: '123' });
const room = await DataReadCommand.execute<RoomEntity>({ collection: 'rooms', id: '456' });
// user is UserEntity, room is RoomEntity - both use same generic code!
```

**Why this is elegant:**
- Implementation knows only `BaseEntity` interface
- Zero code changes when adding `ProjectEntity`, `TaskEntity`, etc.
- Works like Java interfaces - generic implementation, specific usage
- Found in: `data/*`, `events/*`, storage adapters

#### ❌ DON'T Use Generics For Application Logic

**Orchestration commands** that are inherently specific:

```typescript
// ✅ CORRECT: Concrete types for specific orchestration (bag-of-words, screenshot, user/create)
export interface BagOfWordsParams extends CommandParams {
  roomId: UUID;           // Specific to rooms
  personaIds: UUID[];     // Specific to personas
  strategy: 'round-robin' | 'free-for-all';
  initialMessage?: string;
}

// ❌ WRONG: Unnecessary generic complexity
export interface BagOfWordsParams<T extends BaseEntity> extends CommandParams {
  entityId: string;       // Makes no sense - this command is ABOUT rooms/personas
  entityType: T;
}
```

**Why concrete types are better here:**
- Logic is specific to certain entity types (rooms, personas)
- Business/orchestration logic, not data infrastructure
- Using generics adds complexity without benefit
- More readable - `roomId` is clearer than `entityId`

#### 🎯 The Java Interface Analogy

```java
// Generic infrastructure (like our data layer)
public class Repository<T extends Entity> {
  T read(String collection, String id) {
    // Works with ANY entity type
  }
}

// Usage with type safety
User user = repository.read("users", "123");
Room room = repository.read("rooms", "456");

// Specific orchestration (like our application commands)
public class UserLoginService {
  void login(String username, String password) {
    // SPECIFICALLY about users - no generics needed
  }
}

public class RoomChatService {
  void startConversation(UUID roomId, UUID[] personaIds) {
    // SPECIFICALLY about rooms/personas - no generics needed
  }
}
```

### Shared Types
```typescript
// shared/MyCommandParams.ts
export interface MyCommandParams extends CommandParams {
  readonly input: string;
  readonly options?: {
    readonly format?: 'json' | 'text';
    readonly validate?: boolean;
  };
}

export interface MyCommandResult extends CommandResult {
  readonly output: string;
  readonly metadata?: {
    readonly processedAt: string;
    readonly format: string;
  };
}

// Helper functions
export function createMyCommandResult(
  params: MyCommandParams,
  result: Partial<MyCommandResult>
): MyCommandResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: true,
    output: '',
    ...result
  };
}
```

### Command Implementation
```typescript
// browser/MyCommandBrowserCommand.ts
export class MyCommandBrowserCommand extends CommandBase<MyCommandParams, MyCommandResult> {
  
  async execute(params: JTAGPayload): Promise<MyCommandResult> {
    const typedParams = params as MyCommandParams;
    
    try {
      // Do the work with full type safety
      const output = await this.processInput(typedParams.input);
      
      return createMyCommandResult(typedParams, {
        output,
        metadata: {
          processedAt: new Date().toISOString(),
          format: typedParams.options?.format || 'text'
        }
      });
      
    } catch (error) {
      return createMyCommandResult(typedParams, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}
```

## 🔄 Command Composition Patterns

### Sequential Composition
```typescript
async execute(params: MyParams): Promise<MyResult> {
  // Step 1: Get data
  const data = await this.executeCommand('data/read', {
    collection: 'items',
    id: params.itemId
  });
  
  // Step 2: Process it
  const processed = await this.processData(data);
  
  // Step 3: Save result
  await this.executeCommand('data/create', {
    collection: 'results',
    data: processed
  });
  
  return { success: true, processed };
}
```

### Conditional Composition
```typescript
async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
  const imageData = await this.captureImage(params);
  
  // Compose based on parameters
  if (params.filename) {
    await this.executeCommand('file/save', { 
      filepath: `screenshots/${params.filename}`,
      content: imageData 
    });
  }
  
  if (params.analyze) {
    await this.executeCommand('ai/analyze-image', { 
      imageData,
      prompt: 'Describe this screenshot'
    });
  }
  
  return { success: true, imageData };
}
```

### Parallel Composition
```typescript
async execute(params: MultiParams): Promise<MultiResult> {
  // Run multiple commands in parallel
  const [userData, settingsData, historyData] = await Promise.all([
    this.executeCommand('user/get-profile', { userId: params.userId }),
    this.executeCommand('user/get-settings', { userId: params.userId }),
    this.executeCommand('user/get-history', { userId: params.userId })
  ]);
  
  return {
    success: true,
    user: userData,
    settings: settingsData,
    history: historyData
  };
}
```

## 🚫 Anti-Patterns - What NOT to Do

### ❌ Don't Overthink Routing
```typescript
// WRONG - Complex routing logic
async execute(params) {
  if (this.context.environment === 'browser') {
    if (this.needsServer()) {
      return await this.callServer(params);
    } else {
      return await this.doBrowserWork(params);
    }
  } else if (this.context.environment === 'server') {
    // ... complex logic
  }
}

// RIGHT - Simple self-routing
async execute(params) {
  if (this.canDoWorkHere()) {
    return await this.doWork(params);
  } else {
    return await this.remoteExecute(params);
  }
}
```

### ❌ Don't Duplicate Logic
```typescript
// WRONG - Logic in both environments
// browser/BadCommand.ts
async execute(params) {
  const validated = this.validateParams(params); // duplicated
  const processed = this.processData(validated); // duplicated
  return { result: processed };
}

// server/BadCommand.ts  
async execute(params) {
  const validated = this.validateParams(params); // duplicated
  const processed = this.processData(validated); // duplicated
  return { result: processed };
}

// RIGHT - Shared base with sparse overrides
// shared/GoodCommandBase.ts
abstract class GoodCommandBase {
  protected validateParams(params) { /* shared logic */ }
  protected processData(data) { /* shared logic */ }
  abstract execute(params): Promise<Result>;
}
```

### ❌ Don't Use `any` Types
```typescript
// WRONG - Loose typing
async execute(params: any): Promise<any> {
  return await this.doSomething(params.whatever);
}

// RIGHT - Strict typing
async execute(params: JTAGPayload): Promise<MyResult> {
  const typedParams = params as MyParams;
  return createMyResult(typedParams, {
    output: await this.doSomething(typedParams.input)
  });
}
```

## 🎯 Command Testing

### Test Structure
```
commands/my-command/test/
├── unit/
│   ├── MyCommandBrowser.test.ts    # Browser-specific tests
│   ├── MyCommandServer.test.ts     # Server-specific tests  
│   └── MyCommandShared.test.ts     # Shared logic tests
├── integration/
│   └── MyCommandIntegration.test.ts # End-to-end tests
└── validation/
    └── MyCommandValidator.ts        # Real-world validation
```

### Testing Pattern
```typescript
describe('MyCommandBrowserCommand', () => {
  it('processes input correctly', async () => {
    const command = new MyCommandBrowserCommand(context, subpath, commander);
    const params: MyCommandParams = {
      input: 'test data',
      context,
      sessionId: 'test-session'
    };
    
    const result = await command.execute(params);
    
    expect(result.success).toBe(true);
    expect(result.output).toBe('processed: test data');
  });
});
```

## 📚 Learning from Examples

### Study These Working Commands:
- **`screenshot`** - Hybrid browser/server with file saving
- **`file/save`** - Pure server command with simple delegation
- **`data/create`** - Server-native database command
- **`debug/logs`** - Complex server command with file system access
- **`debug/widget-events`** - Browser-native DOM inspection

### Pattern Recognition:
1. Look at the `execute` method
2. See how they handle their natural environment
3. Notice how they delegate when needed  
4. Observe command composition patterns
5. Study the type definitions

## 🚀 Quick Start Template

```typescript
// 1. Create types
// shared/NewCommandParams.ts
export interface NewCommandParams extends CommandParams {
  readonly input: string;
}

export interface NewCommandResult extends CommandResult {
  readonly output: string;
}

// 2. Browser implementation  
// browser/NewCommandBrowserCommand.ts
export class NewCommandBrowserCommand extends CommandBase<NewCommandParams, NewCommandResult> {
  async execute(params: JTAGPayload): Promise<NewCommandResult> {
    const typedParams = params as NewCommandParams;
    
    // Can I do this work in browser?
    if (this.canDoWorkInBrowser(typedParams)) {
      return await this.doBrowserWork(typedParams);
    } else {
      return await this.remoteExecute(typedParams);
    }
  }
}

// 3. Server implementation
// server/NewCommandServerCommand.ts  
export class NewCommandServerCommand extends CommandBase<NewCommandParams, NewCommandResult> {
  async execute(params: JTAGPayload): Promise<NewCommandResult> {
    const typedParams = params as NewCommandParams;
    
    // Can I do this work in server?
    if (this.canDoWorkInServer(typedParams)) {
      return await this.doServerWork(typedParams);
    } else {
      return await this.remoteExecute(typedParams);
    }
  }
}
```

---

## ✨ Elegant Static Execute API - Zero Boilerplate

### The Ultimate Simplicity

Call any command from **anywhere** (browser, server, tests, widgets) with **zero cognitive overhead**:

```typescript
import { ScreenshotBrowserCommand } from './commands/screenshot/browser/ScreenshotBrowserCommand';

// Works identically everywhere - environment crossing automatic
const result = await ScreenshotBrowserCommand.execute({
  querySelector: 'body',
  filename: 'test.png'
});
// result: ScreenshotResult (fully typed, IntelliSense perfect)
```

### How It Works - The Magic of Inheritance

**Every command automatically inherits `static execute()` from `CommandBase`:**

```typescript
// In CommandBase - inherited by ALL commands
static async execute<TParams extends CommandParams, TResult extends CommandResult>(
  this: { commandName: string },
  params: Omit<TParams, 'context' | 'sessionId'>
): Promise<TResult> {
  const { Commands } = await import('../../../system/core/client/shared/Commands');
  return await Commands.execute<TParams, TResult>(
    this.commandName,  // Uses subclass's static property
    params
  );
}
```

### What You Need to Do - ONE Line Per Command

```typescript
export class ScreenshotBrowserCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {
  static readonly commandName = 'screenshot';  // That's it!

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('screenshot', context, subpath, commander);
  }

  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    // Implementation...
  }
}
```

**TypeScript enforces `commandName` declaration** - impossible to forget, compile-time error if missing.

### The Pattern - Enforced Static Properties

```typescript
// CommandBase enforces this via TypeScript's `this` parameter typing
export interface CommandConstructor {
  readonly commandName: string;
}

export abstract class CommandBase<TParams, TResult> {
  // Subclasses MUST override this
  static readonly commandName: string;

  // Methods use `this: CommandConstructor` to enforce it exists
  static async execute<TParams, TResult>(
    this: CommandConstructor,  // TypeScript checks commandName exists
    params: Omit<TParams, 'context' | 'sessionId'>
  ): Promise<TResult>;
}
```

### Benefits - Marshaling Like C#/Unity

✅ **Zero Boilerplate** - 440 lines saved across 44 commands
✅ **Type Safety** - Full IntelliSense, compile-time checks
✅ **Environment Agnostic** - Same code works everywhere
✅ **Automatic Marshaling** - Context injection, routing, unwrapping all handled
✅ **Enforced Pattern** - TypeScript prevents forgetting `commandName`
✅ **Clean Interface** - No string lookups, no manual unwrapping

### Real-World Usage Examples

#### From a Widget (Browser)
```typescript
class ChatWidget extends BaseWidget {
  async loadMessages() {
    const result = await DataReadBrowserCommand.execute({
      collection: 'ChatMessages',
      id: this.roomId,
      backend: 'server'  // Cross-environment call - automatic!
    });

    if (result.success && result.data) {
      this.messages = result.data;
    }
  }
}
```

#### From Server Handler
```typescript
async function processScreenshot(elementId: string) {
  // Calls browser automatically, waits for result
  const screenshot = await ScreenshotBrowserCommand.execute({
    querySelector: `#${elementId}`,
    format: 'png'
  });

  return screenshot.base64Data;
}
```

#### From Tests
```typescript
describe('Screenshot Command', () => {
  it('captures element', async () => {
    const result = await ScreenshotBrowserCommand.execute({
      querySelector: '#test-element'
    });

    expect(result.success).toBe(true);
    expect(result.base64Data).toBeDefined();
  });
});
```

### Cross-Environment Calls - Command Composition

**Commands can easily call their counterparts in other environments:**

#### Pattern 1: Call Yourself in Another Environment
```typescript
// In browser/ScreenshotBrowserCommand.ts
async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
  // Capture in browser
  const imageData = await this.captureDOM(params);

  if (params.saveToFile) {
    // Call myself (screenshot) on server to save the file
    const saved = await ScreenshotBrowserCommand.executeOnServer({
      ...params,
      base64Data: imageData
    });
    return saved;
  }

  return { success: true, base64Data: imageData };
}
```

#### Pattern 2: Call Another Command in Specific Environment
```typescript
// In server/ScreenshotServerCommand.ts
async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
  if (!params.base64Data) {
    // Need browser to capture - call browser version
    return await ScreenshotBrowserCommand.executeOnBrowser(params);
  }

  // I have data - save it using file/save on server
  const result = await FileSaveBrowserCommand.executeOnServer({
    filepath: `screenshots/${params.filename}`,
    content: Buffer.from(params.base64Data, 'base64')
  });

  return { success: true, filepath: result.filepath };
}
```

#### Pattern 3: Automatic Environment Detection
```typescript
// Just call execute() - it figures out where to run
async function processUpload(file: Buffer) {
  // These automatically route to the right environment
  const validated = await ValidateCommand.execute({ file });
  const screenshot = await ScreenshotCommand.execute({ querySelector: '.preview' });
  const saved = await FileSaveCommand.execute({ filepath: 'uploads/file.bin', content: file });

  return { validated, screenshot, saved };
}
```

### The Three Execute Methods - Choose Your Style

```typescript
// 1. execute() - Automatic routing (most common)
const result = await ScreenshotCommand.execute({ querySelector: 'body' });

// 2. executeOnServer() - Force server execution
const saved = await FileSaveCommand.executeOnServer({ filepath, content });

// 3. executeOnBrowser() - Force browser execution
const captured = await ScreenshotCommand.executeOnBrowser({ querySelector: '#element' });

// Or use executeIn() for dynamic environments
const target = needsBrowser ? 'browser' : 'server';
const result = await MyCommand.executeIn(target, { ...params });
```

**All three methods:**
- ✅ Fully typed with IntelliSense
- ✅ Auto-inject context and sessionId
- ✅ Handle marshaling automatically
- ✅ Work from anywhere (browser, server, tests)

### Under the Hood - Automatic Unwrapping

```typescript
// In JTAGClient.daemons.commands.execute()
const response = await this.commands[command](params);

// Check if wrapped in CommandResponse (has commandResult field)
if (response && typeof response === 'object' && 'commandResult' in response) {
  const wrapped = response as CommandSuccessResponse;
  return wrapped.commandResult as U;  // Extract the typed result
}

// Already unwrapped - return as-is
return response as U;
```

**Result**: You always get the properly typed command result, never wrappers or metadata.

### Comparison to Other Marshaling Systems

| System | Quality | TypeScript | Boilerplate | Pattern |
|--------|---------|------------|-------------|---------|
| JNI | Manual | ❌ | High | C↔Java |
| Unity | Good | ✅ | Medium | C#→Native |
| gRPC | Excellent | ✅ | High | Proto files |
| React Native | Manual | ✅ | Medium | JS↔Native |
| **JTAG** | **Seamless** | **✅ Full** | **One line** | **Browser↔Server** |

### The Abstraction Layers

The elegance comes from clean separation:

1. **`CommandClass.execute()`** - Static entry point (what you call)
2. **`Commands.execute()`** - Context injection + routing
3. **`JTAGClient.daemons.commands.execute()`** - Unwrapping + environment handling
4. **`this.commands[name]()`** - Proxy to command instance
5. **`CommandBase.remoteExecute()`** - Cross-environment delegation

**Each layer does ONE thing perfectly.**

### Applying This Pattern Elsewhere

The same `this: { property }` enforcement pattern works for:

- ✅ **Commands** - `commandName` property
- ✅ **Entities** - `collection` property for `BaseEntity.find()`
- ✅ **Widgets** - `widgetName` property for `BaseWidget.loadTemplate()`

**Any class hierarchy needing static properties can use this pattern.**

---

## 💡 Remember: Keep It Simple

**The system handles the complexity. You just:**

1. **Look where you are** (`/browser/` or `/server/`)
2. **Think what you need** (DOM, files, database)
3. **Do it if you can, delegate if you can't**
4. **Compose commands naturally**
5. **Add `static readonly commandName`** - Get elegant execute() for free

**That's it. The architecture is elegant because it's simple.**