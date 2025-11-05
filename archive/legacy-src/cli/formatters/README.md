# CLI Formatters - Modular Output System

This directory contains a modular system for formatting command output in the Continuum CLI.

## Architecture

### BaseFormatter
Abstract base class providing common formatting utilities:
- File size formatting (`formatFileSize`)
- Execution time formatting (`formatExecutionTime`) 
- Status display (`getStatusDisplay`)

### FormatterRegistry
Manages all registered formatters and routes command results to appropriate formatters.

### ScreenshotFormatter
Specialized formatter for screenshot command results, providing:
- User-friendly file path display
- AI-friendly metadata (dimensions, cropping, compression)
- Element targeting information
- Performance metrics

## Adding New Formatters

1. **Create a new formatter class** extending `BaseFormatter`:
```typescript
export class MyCommandFormatter extends BaseFormatter {
  canHandle(result: any, command: string): boolean {
    return command === 'mycommand' && result.data;
  }

  format(result: any): void {
    // Your custom formatting logic
  }
}
```

2. **Register the formatter** in `FormatterRegistry.ts`:
```typescript
private registerDefaultFormatters(): void {
  this.register(new ScreenshotFormatter());
  this.register(new MyCommandFormatter()); // Add your formatter
}
```

## Benefits

- **Modular**: Each command can have its own formatter
- **Extensible**: Easy to add new formatters without modifying CLI core
- **Reusable**: Common formatting utilities in base class
- **Type-safe**: Full TypeScript support with proper interfaces

## Usage

The formatter system is automatically used by the CLI when processing command results. No special configuration needed - just ensure your formatter is registered in the registry.