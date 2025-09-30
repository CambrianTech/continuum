# Elegant Command Usage - Zero Boilerplate Marshaling

## The Vision

Execute commands from **anywhere** (browser, server, tests, widgets) with **zero cognitive overhead**:

```typescript
import { ScreenshotBrowserCommand } from './commands/screenshot/browser/ScreenshotBrowserCommand';
import { DataReadBrowserCommand } from './commands/data/read/browser/DataReadBrowserCommand';

// Works identically in browser OR server - environment crossing automatic
const screenshot = await ScreenshotBrowserCommand.execute({
  querySelector: 'body',
  filename: 'test.png'
});
// screenshot: ScreenshotResult (properly typed)

const data = await DataReadBrowserCommand.execute({
  collection: 'Users',
  id: '12345',
  backend: 'server'  // Cross-environment: browser code calling server storage
});
// data: DataReadResult<BaseEntity> (properly typed)
```

## How It Works

### 1. CommandBase provides static execute() (inherited by ALL commands)

```typescript
// In CommandBase<TParams, TResult>
static async execute<TParams extends CommandParams, TResult extends CommandResult>(
  this: { commandName: string },
  params: Omit<TParams, 'context' | 'sessionId'>
): Promise<TResult> {
  const { Commands } = await import('../../../system/core/client/shared/Commands');
  return await Commands.execute<TParams, TResult>(
    this.commandName,  // Uses the static property from subclass
    params
  );
}
```

### 2. Each command declares its name (ONE line of boilerplate)

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

### 3. Commands.execute handles marshaling automatically

- Auto-injects `context` and `sessionId`
- Routes to correct environment (browser/server)
- Unwraps `CommandResponse` wrappers
- Returns properly typed results

## What You Get

✅ **Type Safety**: Full IntelliSense and compile-time checks
✅ **Zero Boilerplate**: Inherit once, use everywhere
✅ **Environment Agnostic**: Same code works in browser/server/tests
✅ **Automatic Marshaling**: JNI/Unity-level seamlessness
✅ **Clean Interface**: No string-based lookups, no manual unwrapping

## Comparison to Other Systems

| System | Marshaling Quality | TypeScript Support | Boilerplate |
|--------|-------------------|-------------------|-------------|
| JNI | Manual, error-prone | ❌ | High |
| Unity C#→Native | Good, but verbose | ✅ | Medium |
| gRPC | Excellent | ✅ | High (proto files) |
| **JTAG Commands** | **Seamless** | **✅ Full** | **One line** |

## Real-World Usage

```typescript
// In a widget (browser)
class ChatWidget extends BaseWidget {
  async loadMessages() {
    // Calls server automatically, returns typed results
    const result = await DataReadBrowserCommand.execute({
      collection: 'ChatMessages',
      id: this.roomId,
      backend: 'server'
    });

    if (result.success && result.data) {
      this.messages = result.data;
    }
  }
}

// In a server handler
async function processUpload(file: Buffer) {
  // Calls browser to capture screenshot, waits for result
  const screenshot = await ScreenshotBrowserCommand.execute({
    querySelector: '.upload-preview',
    format: 'jpeg'
  });

  return { file, preview: screenshot.base64Data };
}

// In tests
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

## The Magic

The brilliance is in the **abstraction layers**:

1. `CommandBase.execute()` - Universal entry point (static)
2. `Commands.execute()` - Marshaling layer (context injection)
3. `JTAGClient.daemons.commands.execute()` - Environment routing
4. `this.commands[name]()` - Proxy to actual command instance
5. `CommandBase.remoteExecute()` - Cross-environment delegation

**Result**: Call any command from anywhere, and it Just Works™

## Next Steps

Apply this pattern to ALL commands:
- ✅ ScreenshotCommand
- ⏳ DataReadCommand
- ⏳ DataCreateCommand
- ⏳ DataUpdateCommand
- ⏳ FileLoadCommand
- ⏳ ThemeSetCommand
- ... (all 44 commands inherit automatically)

Just add `static readonly commandName = 'command-name'` to each command class.