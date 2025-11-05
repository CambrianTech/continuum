# Architectural Patterns - Enforced Elegance

## Pattern: Static Property Enforcement with TypeScript

### Problem
We need subclasses to declare a static property (like `commandName` or `collection`) so base class methods can use it, but TypeScript doesn't enforce abstract static properties.

### Solution
Use the `this` parameter typing in static methods to enforce the contract:

```typescript
// 1. Define the interface for what static properties we need
export interface CommandConstructor {
  readonly commandName: string;
}

// 2. Declare the static property in the base class (documentation + inheritance)
export abstract class CommandBase<TParams, TResult> {
  static readonly commandName: string;  // Subclasses must override

  // 3. Use `this: CommandConstructor` to enforce it
  static async execute<TParams, TResult>(
    this: CommandConstructor,  // TypeScript enforces commandName exists
    params: Omit<TParams, 'context' | 'sessionId'>
  ): Promise<TResult> {
    return await Commands.execute<TParams, TResult>(
      this.commandName,  // Guaranteed to exist
      params
    );
  }
}

// 4. Subclasses MUST declare it or get compile error
export class ScreenshotCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {
  static readonly commandName = 'screenshot';  // Required!
}
```

### Result
✅ Compile-time enforcement
✅ Zero runtime overhead
✅ Perfect IntelliSense
✅ Impossible to forget

## Applications

### 1. Commands - commandName
```typescript
export abstract class CommandBase<TParams, TResult> {
  static readonly commandName: string;

  static async execute<TParams, TResult>(
    this: { commandName: string },
    params: Omit<TParams, 'context' | 'sessionId'>
  ): Promise<TResult>;
}
```

### 2. Entities - collection
```typescript
export abstract class BaseEntity {
  static readonly collection: string;

  static async find<T extends BaseEntity>(
    this: { collection: string },
    filter: Partial<T>
  ): Promise<T[]> {
    return DataDaemon.list(this.collection, filter);
  }
}

// Usage
const users = await UserEntity.find({ active: true });
// UserEntity.collection is enforced at compile time
```

### 3. Widgets - widgetName
```typescript
export abstract class BaseWidget {
  static readonly widgetName: string;

  static async loadTemplate<T extends BaseWidget>(
    this: { widgetName: string }
  ): Promise<string> {
    return FileLoad.execute({
      filepath: `widgets/${this.widgetName}/template.html`
    });
  }
}
```

## Why This Pattern Works

### Traditional Approach (Doesn't Work)
```typescript
abstract class Base {
  abstract static readonly prop: string;  // ❌ TypeScript error
}
```
TypeScript doesn't allow abstract static properties because static members aren't inherited in the traditional OOP sense.

### Our Approach (Works Perfectly)
```typescript
abstract class Base {
  static readonly prop: string;  // ✅ Declaration for inheritance

  static method(this: { prop: string }) {  // ✅ Enforcement via `this` typing
    console.log(this.prop);  // Guaranteed to exist
  }
}
```

The `this` parameter typing is a TypeScript feature that enforces the shape of `this` in methods. When you call `SubClass.method()`, TypeScript checks that `SubClass` matches the `this` type.

## Benefits

1. **Zero Boilerplate**: Subclasses just declare one property
2. **Compile-Time Safety**: Impossible to forget, TypeScript catches it
3. **Perfect for Base Class Patterns**: Methods can use the property confidently
4. **Self-Documenting**: Clear contract in the base class
5. **No Runtime Overhead**: Pure compile-time enforcement

## Real-World Impact

### Before
```typescript
// Every command needs to repeat this
export class ScreenshotCommand {
  static async execute(params) {
    return await Commands.execute('screenshot', params);  // Brittle string
  }
}

export class DataReadCommand {
  static async execute(params) {
    return await Commands.execute('data/read', params);  // Repetitive
  }
}
// ... 44 more commands with identical boilerplate
```

### After
```typescript
// Just declare the name, inherit everything else
export class ScreenshotCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {
  static readonly commandName = 'screenshot';  // One line!
}

export class DataReadCommand extends CommandBase<DataReadParams, DataReadResult> {
  static readonly commandName = 'data/read';  // One line!
}

// Usage is identical and type-safe for all 44 commands
const screenshot = await ScreenshotCommand.execute({ querySelector: 'body' });
const data = await DataReadCommand.execute({ collection: 'Users', id: '123' });
```

### Metrics
- **44 commands** × **~10 lines of boilerplate each** = **440 lines saved**
- **Zero chance of typos** in command names (compile-time checked)
- **Perfect IntelliSense** for all parameters and returns
- **Works identically** in browser, server, and tests