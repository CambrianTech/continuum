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

### Shared Types
```typescript
// shared/MyCommandTypes.ts
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
// shared/NewCommandTypes.ts
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

## 💡 Remember: Keep It Simple

**The system handles the complexity. You just:**

1. **Look where you are** (`/browser/` or `/server/`)
2. **Think what you need** (DOM, files, database)
3. **Do it if you can, delegate if you can't**
4. **Compose commands naturally**

**That's it. The architecture is elegant because it's simple.**