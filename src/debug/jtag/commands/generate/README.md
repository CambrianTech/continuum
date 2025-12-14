# Generate Command

Generate new commands, daemons, or widgets using templates and CommandSpec definitions.

## Quick Start (Most Common Use Case)

```bash
# 1. Get a template to understand the spec format
./jtag generate --template=true > /tmp/my-command-spec.json

# 2. Edit the spec (change name, description, params, results)
# Edit /tmp/my-command-spec.json

# 3. Generate the command
./jtag generate --spec=/tmp/my-command-spec.json

# 4. Implement your logic in the generated server file
# Edit commands/your-command/server/YourCommandServerCommand.ts
```

## Command Templates Location

**IMPORTANT:** All command templates are in `generator/templates/command/`:
- `shared-types.template.ts` - Types file with params/results
- `server.template.ts` - Server implementation (where you add logic)
- `browser.template.ts` - Browser stub (usually minimal)
- `README.template.md` - Generated documentation
- `unit-test.template.ts` - Unit test template
- `integration-test.template.ts` - Integration test template

## Usage

```bash
./jtag generate [options]
```

## Parameters

- **spec** (required*): CommandSpec object, file path, or JSON string
  - Can be: `{"name": "my/command", ...}` (object)
  - Or: `/path/to/spec.json` (file path)
  - Or: `'{"name":"my/command",...}'` (JSON string)

- **template** (optional): Get an example spec instead of generating
  - Use `--template=true` to see the spec format
  - Returns a complete example you can copy and modify

## CommandSpec Format

A CommandSpec defines what files to generate. Required fields:

```json
{
  "name": "my/command",              // Command name (use / for nesting)
  "description": "What it does",     // Brief description
  "params": [                        // Input parameters
    {
      "name": "message",
      "type": "string",              // string, boolean, number, object, array
      "optional": false,             // Required or optional?
      "description": "What this param does"
    }
  ],
  "results": [                       // Output fields (beyond success/error)
    {
      "name": "result",
      "type": "string",
      "description": "What this result means"
    }
  ],
  "examples": [                      // Usage examples
    {
      "description": "Basic usage",
      "command": "./jtag my/command --message='test'",
      "expectedResult": "{ success: true, result: '...' }"
    }
  ],
  "accessLevel": "ai-safe"          // ai-safe, internal, or admin
}
```

## Result

Returns CommandResult with:
- **success**: `boolean` - Whether generation succeeded
- **filesCreated**: `string[]` - Paths of generated files
- **commandPath**: `string` - Base directory where command was created
- **templateSpec**: `object` - Example spec (only when template=true)
- **error**: `string` - Error message if generation failed

## Examples

### Get Template (Start Here!)

```bash
./jtag generate --template=true
```

Returns a complete example spec you can copy and modify.

**Pro tip:** Save to a file for editing:
```bash
./jtag generate --template=true | jq .templateSpec > /tmp/my-spec.json
```

### Generate from File (Recommended)

```bash
# 1. Create spec file
cat > /tmp/echo-spec.json <<'EOF'
{
  "name": "echo",
  "description": "Echo back a message",
  "params": [
    {"name": "message", "type": "string", "optional": false, "description": "Message to echo"}
  ],
  "results": [
    {"name": "echoed", "type": "string", "description": "The echoed message"}
  ],
  "examples": [
    {
      "description": "Basic echo",
      "command": "./jtag echo --message='Hello'",
      "expectedResult": "{ success: true, echoed: 'Hello' }"
    }
  ],
  "accessLevel": "ai-safe"
}
EOF

# 2. Generate
./jtag generate --spec=/tmp/echo-spec.json
```

### Generate from JSON String

```bash
./jtag generate --spec='{"name":"hello","description":"Say hello","params":[],"results":[],"examples":[],"accessLevel":"ai-safe"}'
```

### Generate Nested Commands

Use `/` in the name to create nested command structures:

```bash
# Creates commands/git/commit/
./jtag generate --spec='{"name":"git/commit","description":"Commit changes",...}'

# Creates commands/ai/model/list/
./jtag generate --spec='{"name":"ai/model/list","description":"List models",...}'
```

## After Generation: Implement Your Logic

The generator creates a skeleton - you implement the logic:

1. **Open the server file:**
   ```bash
   # Example for commands/echo/
   code commands/echo/server/EchoServerCommand.ts
   ```

2. **Find the TODO section:**
   ```typescript
   async execute(params: EchoParams): Promise<EchoResult> {
     // TODO: Implement your command logic here
   }
   ```

3. **Implement your logic:**
   ```typescript
   async execute(params: EchoParams): Promise<EchoResult> {
     const echoed = params.message.toUpperCase();

     return createEchoResultFromParams(params, {
       success: true,
       echoed
     });
   }
   ```

4. **Use the result helper:**
   - Always use `createYourCommandResultFromParams(params, {...})`
   - This automatically includes sessionId, contextId, etc.
   - Only specify what changed: success, error, and your result fields

## Common Patterns

### Required Parameter Validation

```typescript
if (!params.message?.trim()) {
  throw new ValidationError(
    'message',
    'Message is required. Use --message="your text"'
  );
}
```

### Optional Parameters with Defaults

```typescript
const uppercase = params.uppercase ?? false;
const limit = params.limit ?? 10;
```

### Error Handling

```typescript
try {
  // Your logic here
  return createYourResultFromParams(params, {
    success: true,
    result: data
  });
} catch (error: any) {
  return createYourResultFromParams(params, {
    success: false,
    error: error.message
  });
}
```

## Troubleshooting

### "Cannot find module" errors

**Problem:** Generated files have import errors
**Solution:** Run the structure generator to update imports:
```bash
npx tsx generator/generate-structure.ts
```

### "Command not found" after generation

**Problem:** New command doesn't show up in `./jtag list`
**Solution:** Regenerate the command registry:
```bash
npx tsx generator/generate-structure.ts
npm start  # Redeploy
```

### "ValidationError is not defined"

**Problem:** Uncomment the ValidationError import in your server file:
```typescript
import { ValidationError } from '../../../system/core/types/ErrorTypes';
```

### "Wrong number of type arguments"

**Problem:** Using wrong result creation function
**Solution:** Use the generated helper, not generic transformPayload:
```typescript
// ❌ Wrong
return transformPayload(params, { success: true });

// ✅ Correct
return createYourCommandResultFromParams(params, { success: true });
```

## Access Levels

- **ai-safe**: Safe for AI personas to use autonomously
- **internal**: System/daemon use only, not exposed in tool lists
- **admin**: Requires elevated permissions (not yet enforced)

## File Structure Created

```
commands/your-command/
├── shared/
│   └── YourCommandTypes.ts       # Params, results, factory functions
├── browser/
│   └── YourCommandBrowser.ts     # Browser implementation (usually stub)
├── server/
│   └── YourCommandServer.ts      # Server implementation (your logic here)
├── test/
│   ├── unit/
│   │   └── YourCommand.test.ts          # Unit tests
│   └── integration/
│       └── YourCommandIntegration.test.ts  # Integration tests
├── README.md                     # Generated documentation
├── package.json                  # Module metadata
└── .npmignore                    # NPM ignore rules
```

## Best Practices

1. **Start with the template** - Always use `--template=true` first
2. **Save specs to files** - Easier to edit than command-line JSON
3. **Descriptive examples** - Good examples become good documentation
4. **Use nested names** - Group related commands (git/*, ai/*, etc.)
5. **Implement incrementally** - Generate, implement, test, repeat

## See Also

- `generator/templates/command/` - All command templates
- `ARCHITECTURE-RULES.md` - System architecture guidelines
- `./jtag help` - List all available commands
- `./jtag generate/audit` - Audit generated commands for issues

## Access Level

**internal** - Internal use only, not exposed to AI personas
