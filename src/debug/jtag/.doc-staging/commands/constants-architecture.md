# Command Constants Architecture

## Design Philosophy

**Modular Constants** - Each command domain has its own constants file in its `shared` directory, co-located with the commands themselves. This provides:

1. **Locality** - Constants live with the code that uses them
2. **Discoverability** - Easy to find what commands exist in each domain
3. **Tree-shaking** - Import only what you need
4. **Maintainability** - Add new commands alongside implementation

## Structure

```
commands/
├── data/
│   └── shared/
│       └── DataCommandConstants.ts        ← data/* commands
├── debug/
│   └── shared/
│       └── DebugCommandConstants.ts       ← debug/* commands
├── state/
│   └── shared/
│       └── StateCommandConstants.ts       ← state/* commands
├── file/
│   └── shared/
│       └── FileCommandConstants.ts        ← file/* commands
├── theme/
│   └── shared/
│       └── ThemeCommandConstants.ts       ← theme/* commands
└── shared/
    ├── UICommandConstants.ts              ← Top-level UI commands
    ├── SystemCommandConstants.ts          ← System commands
    └── CommandConstants.ts                ← Central re-export
```

## Usage Patterns

### Pattern 1: Granular Import (Recommended)

Import only the domain you need for optimal tree-shaking:

```typescript
import { DATA_COMMANDS } from './commands/data/shared/DataCommandConstants';

await Commands.execute(DATA_COMMANDS.LIST, {
  collection: 'users'
});
```

### Pattern 2: Central Import (Convenience)

Import all commands for autocomplete and exploration:

```typescript
import { ALL_COMMANDS } from './commands/shared/CommandConstants';

await Commands.execute(ALL_COMMANDS.DATA.LIST, params);
await Commands.execute(ALL_COMMANDS.DEBUG.LOGS, params);
await Commands.execute(ALL_COMMANDS.UI.SCREENSHOT, params);
```

### Pattern 3: Re-Export (Widget/Module Level)

Create local re-exports for frequently used commands:

```typescript
// In your widget/module
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';
import { UI_COMMANDS } from '../../commands/shared/UICommandConstants';

export const WIDGET_COMMANDS = {
  ...DATA_COMMANDS,
  ...UI_COMMANDS,
};

// Use in widget
await Commands.execute(WIDGET_COMMANDS.LIST, params);
```

## Command Domain Constants

### DATA_COMMANDS (`commands/data/shared/DataCommandConstants.ts`)
CRUD operations on entities:
- `DATA_COMMANDS.LIST` - `'data/list'`
- `DATA_COMMANDS.CREATE` - `'data/create'`
- `DATA_COMMANDS.READ` - `'data/read'`
- `DATA_COMMANDS.UPDATE` - `'data/update'`
- `DATA_COMMANDS.DELETE` - `'data/delete'`
- `DATA_COMMANDS.TRUNCATE` - `'data/truncate'`
- `DATA_COMMANDS.CLEAR` - `'data/clear'`
- `DATA_COMMANDS.SCHEMA` - `'data/schema'`

### DEBUG_COMMANDS (`commands/debug/shared/DebugCommandConstants.ts`)
Development and debugging tools:
- `DEBUG_COMMANDS.LOGS` - `'debug/logs'`
- `DEBUG_COMMANDS.HTML_INSPECTOR` - `'debug/html-inspector'`
- `DEBUG_COMMANDS.WIDGET_STATE` - `'debug/widget-state'`
- `DEBUG_COMMANDS.WIDGET_EVENTS` - `'debug/widget-events'`
- `DEBUG_COMMANDS.SCROLL_TEST` - `'debug/scroll-test'`
- `DEBUG_COMMANDS.CRUD_SYNC` - `'debug/crud-sync'`
- ... (see file for complete list)

### STATE_COMMANDS (`commands/state/shared/StateCommandConstants.ts`)
State management operations:
- `STATE_COMMANDS.GET` - `'state/get'`
- `STATE_COMMANDS.CREATE` - `'state/create'`
- `STATE_COMMANDS.UPDATE` - `'state/update'`

### FILE_COMMANDS (`commands/file/shared/FileCommandConstants.ts`)
File system operations:
- `FILE_COMMANDS.LOAD` - `'file/load'`
- `FILE_COMMANDS.SAVE` - `'file/save'`
- `FILE_COMMANDS.APPEND` - `'file/append'`

### THEME_COMMANDS (`commands/theme/shared/ThemeCommandConstants.ts`)
Theme management:
- `THEME_COMMANDS.GET` - `'theme/get'`
- `THEME_COMMANDS.LIST` - `'theme/list'`
- `THEME_COMMANDS.SET` - `'theme/set'`

### UI_COMMANDS (`commands/shared/UICommandConstants.ts`)
Top-level UI interactions:
- `UI_COMMANDS.SCREENSHOT` - `'screenshot'`
- `UI_COMMANDS.CLICK` - `'click'`
- `UI_COMMANDS.TYPE` - `'type'`
- `UI_COMMANDS.SCROLL` - `'scroll'`
- `UI_COMMANDS.NAVIGATE` - `'navigate'`
- ... (see file for complete list)

### SYSTEM_COMMANDS (`commands/shared/SystemCommandConstants.ts`)
System-level operations:
- `SYSTEM_COMMANDS.PING` - `'ping'`
- `SYSTEM_COMMANDS.EXEC` - `'exec'`
- `SYSTEM_COMMANDS.LIST` - `'list'`
- `SESSION_COMMANDS.CREATE` - `'session/create'`
- `USER_COMMANDS.CREATE` - `'user/create'`
- `TEST_COMMANDS.RUN_SUITE` - `'test/run/suite'`
- ... (see file for complete list)

## Adding New Commands

When creating a new command:

1. **Add to appropriate module constants file**:
   ```typescript
   // commands/data/shared/DataCommandConstants.ts
   export const DATA_COMMANDS = {
     // ... existing
     MY_NEW_COMMAND: 'data/my-new-command',  // ← Add here
   } as const;
   ```

2. **Implementation will automatically be typed**:
   ```typescript
   // Import at top of your file
   import { DATA_COMMANDS } from './commands/data/shared/DataCommandConstants';

   // Use throughout
   await Commands.execute(DATA_COMMANDS.MY_NEW_COMMAND, params);
   ```

3. **Benefits**:
   - Typos caught at compile time
   - Autocomplete in IDE
   - Refactoring is safe (change in one place)
   - Easy to discover what commands exist

## Helper Functions

### `isValidCommand(command: string): boolean`
Check if a string is a valid command name:
```typescript
if (isValidCommand(userInput)) {
  await Commands.execute(userInput, params);
}
```

### `parseCommand(command: string): { domain, action } | null`
Parse command into domain and action:
```typescript
const parsed = parseCommand('data/list');
// { domain: 'data', action: 'list' }

const parsed = parseCommand('screenshot');
// { domain: 'ui', action: 'screenshot' }
```

## Migration Guide

### Before (Magic Strings ❌)
```typescript
await Commands.execute('data/list', params);
await Commands.execute('debug/logs', params);
await Commands.execute('screenshot', params);
```

### After (Constants ✅)
```typescript
import { DATA_COMMANDS } from './commands/data/shared/DataCommandConstants';
import { DEBUG_COMMANDS } from './commands/debug/shared/DebugCommandConstants';
import { UI_COMMANDS } from './commands/shared/UICommandConstants';

await Commands.execute(DATA_COMMANDS.LIST, params);
await Commands.execute(DEBUG_COMMANDS.LOGS, params);
await Commands.execute(UI_COMMANDS.SCREENSHOT, params);
```

## Consistency with Events

This matches the pattern established in `EventConstants.ts`:
- **Events**: Modular by domain (`DATA_EVENTS`, `UI_EVENTS`, `SYSTEM_EVENTS`)
- **Commands**: Modular by domain (`DATA_COMMANDS`, `DEBUG_COMMANDS`, `UI_COMMANDS`)

Both provide:
- Type safety
- Autocomplete
- Centralized definitions
- Easy refactoring
- Clear API surface

## Testing

Constants make testing easier:

```typescript
import { DATA_COMMANDS } from './commands/data/shared/DataCommandConstants';

describe('Data Commands', () => {
  it('should execute list command', async () => {
    const result = await Commands.execute(DATA_COMMANDS.LIST, {
      collection: 'users'
    });
    expect(result.success).toBe(true);
  });
});
```

No typos, no magic strings, full IDE support!
