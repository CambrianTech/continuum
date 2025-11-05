# Base Command

Foundation class for all Continuum commands. Provides standard command interface, parameter validation, and execution patterns.

## Usage
```typescript
import { BaseCommand } from './BaseCommand.js';

export class MyCommand extends BaseCommand {
  static async execute(params, context) {
    // Command implementation
  }
  
  static getDefinition() {
    return {
      name: 'my-command',
      description: 'My command description',
      // ...
    };
  }
}
```

## Features
- Standard command interface
- Parameter validation
- Error handling
- Result formatting
- Context management

## Configuration
```json
{
  "module": "base-command",
  "category": "Core",
  "capabilities": ["command-foundation"]
}
```

## Testing
```bash
npm test
```