# CLI Array Parameter Design

## Problem Statement

The current CLI parser requires awkward JSON syntax for array parameters:

```bash
# ❌ Current (terrible UX):
./jtag chat/send --message="Test" '--media=["/path/img1.png","/path/img2.jpg"]'

# ✅ Desired (industry standard):
./jtag chat/send --message="Test" --media /path/img1.png --media /path/img2.jpg
```

**Root Cause:** CLI parser (`cli.ts:159-205`) uses direct assignment (`params[key] = value`), which overwrites previous values instead of accumulating them into arrays.

---

## Design Principle: Format Translation at Boundaries

Each layer should use its **natural idiom**, with translation at boundaries:

```
┌─────────────────────────────────────────────────────────────┐
│ USER INTERFACES (multiple input formats)                    │
├─────────────────────────────────────────────────────────────┤
│ CLI:      --media path1 --media path2                       │
│ Persona:  tools with array parameters in natural format     │
│ Widget:   drag-and-drop multiple files                      │
│ API:      JSON: { media: ["path1", "path2"] }               │
└──────────────────────┬──────────────────────────────────────┘
                       │ Translation Layer
                       ▼
┌─────────────────────────────────────────────────────────────┐
│ INTERNAL FORMAT (canonical)                                 │
├─────────────────────────────────────────────────────────────┤
│ TypeScript: { media: string[] }                             │
│ Commands always receive properly typed arrays               │
└─────────────────────────────────────────────────────────────┘
```

**Key Insight:** Translation happens at the **boundary**, not inside commands. Commands receive clean, typed parameters.

---

## Implementation Strategy

### Phase 1: CLI Layer - Repeated Flag Accumulation

**File:** `cli.ts` (lines 159-205)

**Current Logic:**
```typescript
params[key] = parsedValue;  // ❌ Overwrites
```

**New Logic:**
```typescript
// Check if key already exists
if (params[key] !== undefined) {
  // Accumulate into array
  if (!Array.isArray(params[key])) {
    params[key] = [params[key]];  // Convert existing value to array
  }
  params[key].push(parsedValue);  // Append new value
} else {
  // First occurrence - store as-is
  params[key] = parsedValue;
}
```

**Result:**
```bash
# Single value:
./jtag cmd --media path1
# → { media: "path1" }

# Multiple values:
./jtag cmd --media path1 --media path2
# → { media: ["path1", "path2"] }

# JSON still works (backward compat):
./jtag cmd --media='["path1","path2"]'
# → { media: ["path1", "path2"] }
```

**Backward Compatibility:** Commands that expect `string[]` already handle arrays. Commands expecting `string` will now get arrays when multiple values provided (may need defensive coding).

---

### Phase 2: Persona Tool Layer - Tool Definition Translation

**Problem:** PersonaUser tools need to know which parameters accept arrays so the AI can call them correctly.

**Solution:** Tool definitions declare parameter types, and the system generates appropriate examples:

**File:** `system/user/server/modules/PersonaToolDefinitions.ts`

```typescript
interface ToolParameterSchema {
  name: string;
  type: 'string' | 'number' | 'boolean' | 'string[]' | 'number[]';
  description: string;
  required?: boolean;
}

// Example tool definition:
{
  name: 'chat/send',
  description: 'Send a chat message with optional media attachments',
  parameters: [
    { name: 'message', type: 'string', required: true },
    { name: 'media', type: 'string[]', description: 'Array of file paths' }
  ]
}
```

**AI Tool Call Format (Anthropic):**
```json
{
  "tool_name": "chat/send",
  "tool_input": {
    "message": "Test",
    "media": ["path1.png", "path2.jpg"]
  }
}
```

**Translation:** `PersonaToolExecutor` receives AI tool calls and passes arrays directly to commands (no translation needed - AIs already use JSON arrays).

---

### Phase 3: Help System - Context-Aware Examples

**File:** `commands/help/server/HelpServerCommand.ts`

Help output should show **appropriate examples for the calling context**:

```typescript
function generateExamples(command: CommandSignature, callerContext: 'cli' | 'api' | 'tool'): string[] {
  const examples = [];

  for (const param of command.parameters) {
    if (param.type === 'string[]') {
      switch (callerContext) {
        case 'cli':
          examples.push(
            `# Single file:`,
            `./jtag ${command.name} --${param.name} path1.png`,
            ``,
            `# Multiple files:`,
            `./jtag ${command.name} --${param.name} path1.png --${param.name} path2.jpg`
          );
          break;

        case 'api':
          examples.push(
            `// API call:`,
            `Commands.execute('${command.name}', {`,
            `  ${param.name}: ['path1.png', 'path2.jpg']`,
            `})`
          );
          break;

        case 'tool':
          examples.push(
            `Tool call format:`,
            `{`,
            `  "tool_name": "${command.name}",`,
            `  "tool_input": {`,
            `    "${param.name}": ["path1.png", "path2.jpg"]`,
            `  }`,
            `}`
          );
          break;
      }
    }
  }

  return examples;
}
```

**Usage:**
```bash
# CLI user sees CLI examples:
./jtag help chat/send
# → Shows: ./jtag chat/send --media path1 --media path2

# AI asking for help sees tool format:
Commands.execute('help', { commandName: 'chat/send', callerType: 'tool' })
# → Shows JSON tool call format
```

---

## Type Safety Considerations

### Commands Must Declare Array Parameters

**Current Problem:** TypeScript interfaces use `string[]`, but there's no runtime metadata.

**Solution:** Commands export parameter metadata:

```typescript
// commands/chat/send/shared/ChatSendTypes.ts
export interface ChatSendParams extends CommandParams {
  message: string;
  media?: string[];  // Type declaration
}

// NEW: Runtime metadata for CLI parser and help system
export const ChatSendParamMetadata: ParameterMetadata[] = [
  { name: 'message', type: 'string', required: true },
  { name: 'media', type: 'string[]', required: false },
  { name: 'room', type: 'string', required: false },
];
```

**Alternative:** Generate metadata from TypeScript types using `ts-morph` during build (more advanced).

---

## Edge Cases and Decisions

### 1. What if command expects `string` but receives array?

**Scenario:**
```bash
./jtag cmd --name foo --name bar
# Result: { name: ["foo", "bar"] }
# But command signature expects: { name: string }
```

**Options:**

A. **Strict Mode (recommended):** CLI validates against command schema, rejects multiple values for non-array params
```bash
./jtag cmd --name foo --name bar
# Error: Parameter 'name' does not accept multiple values
```

B. **Permissive Mode:** Take last value (current behavior)
```typescript
params[key] = Array.isArray(parsedValue) ? parsedValue[parsedValue.length - 1] : parsedValue;
```

C. **Join Mode:** Convert to comma-separated string
```typescript
params[key] = Array.isArray(parsedValue) ? parsedValue.join(',') : parsedValue;
```

**Decision:** Start with **Option A (strict mode)** - require commands to explicitly declare array params. This prevents accidental bugs and makes the API surface clear.

---

### 2. Mixed repeated flags and JSON arrays?

**Scenario:**
```bash
./jtag cmd --media path1 --media='["path2","path3"]'
# Result: ???
```

**Decision:** **Last value wins** (simpler to reason about). If you provide JSON array, it replaces all previous values.

---

### 3. Comma-separated values?

**Scenario:**
```bash
./jtag cmd --media="path1,path2,path3"
# Should this split into array?
```

**Decision:** **No automatic splitting**. Commas are valid in file paths (rare but possible). Users must use:
- Repeated flags: `--media path1 --media path2`
- JSON arrays: `--media='["path1","path2"]'`

**Exception:** Commands can opt-in to comma-splitting by handling it in their own implementation (like `git issue create` does for labels).

---

### 4. Empty arrays?

**Scenario:**
```bash
./jtag cmd --media='[]'
# vs
./jtag cmd
# (no --media flag)
```

**Decision:**
- `--media='[]'` → Explicit empty array: `{ media: [] }`
- No `--media` flag → Undefined: `{ media: undefined }`

Commands should treat these differently if semantics require.

---

## Migration Path

### Phase 1: CLI Parser (Immediate)

✅ **No Breaking Changes**
- Implement repeated flag accumulation in `cli.ts`
- Keep JSON array support for backward compatibility
- Single values remain single values (not wrapped in array)

**Testing:**
```bash
# Existing commands continue working:
./jtag ping
./jtag chat/send --message="Test"
./jtag screenshot --querySelector="body"

# New array syntax works:
./jtag chat/send --message="Test" --media p1.png --media p2.jpg
```

---

### Phase 2: Parameter Metadata (Non-Breaking)

✅ **Additive Only**
- Add `ParameterMetadata` exports to command types files
- Start with high-traffic commands: `chat/send`, `media/*`, `file/*`
- CLI parser uses metadata for validation (if available)
- Commands without metadata work as before

---

### Phase 3: Help System Enhancement (Non-Breaking)

✅ **Better UX**
- Help command detects caller context (CLI vs API vs Tool)
- Shows appropriate examples for that context
- Existing help output remains functional

---

### Phase 4: Strict Validation (Opt-In)

⚠️ **Potentially Breaking**
- Add `--strict` flag to CLI: `./jtag --strict cmd --name foo --name bar`
- In strict mode, reject multiple values for non-array parameters
- Default remains permissive for compatibility

---

## Testing Strategy

### Unit Tests: `cli.test.ts`

```typescript
describe('CLI Array Parameter Parsing', () => {
  it('should accumulate repeated flags into array', () => {
    const args = ['cmd', '--media', 'p1.png', '--media', 'p2.jpg'];
    const params = parseArgs(args);
    expect(params.media).toEqual(['p1.png', 'p2.jpg']);
  });

  it('should keep single value as string', () => {
    const args = ['cmd', '--media', 'p1.png'];
    const params = parseArgs(args);
    expect(params.media).toBe('p1.png');
  });

  it('should handle JSON arrays', () => {
    const args = ['cmd', '--media=["p1.png","p2.jpg"]'];
    const params = parseArgs(args);
    expect(params.media).toEqual(['p1.png', 'p2.jpg']);
  });

  it('should handle mixed types correctly', () => {
    const args = ['cmd', '--media', 'p1.png', '--room', 'general', '--media', 'p2.jpg'];
    const params = parseArgs(args);
    expect(params.media).toEqual(['p1.png', 'p2.jpg']);
    expect(params.room).toBe('general');
  });
});
```

---

### Integration Tests: `commands/chat/send.test.ts`

```typescript
describe('chat/send with multiple media', () => {
  it('should accept repeated --media flags', async () => {
    const result = await runCLI([
      'chat/send',
      '--message', 'Test',
      '--media', '/path/img1.png',
      '--media', '/path/img2.jpg',
      '--room', 'general'
    ]);

    expect(result.success).toBe(true);
    expect(result.messageEntity.content.media).toHaveLength(2);
  });
});
```

---

### Manual Testing

```bash
# Test progression:
npm start

# 1. Single value (should work as before)
./jtag chat/send --message="Single" --media /test-images/image-1.webp

# 2. Multiple values (new syntax)
./jtag chat/send --message="Multiple" \
  --media /test-images/image-1.webp \
  --media /test-images/image-3.jpg \
  --media /test-images/image-6.png

# 3. JSON array (backward compat)
./jtag chat/send --message="JSON" --media='["/test-images/image-1.webp","/test-images/image-3.jpg"]'

# 4. Verify in chat export
./jtag chat/export --room="general" --limit=10
```

---

## Documentation Updates Required

### 1. CLAUDE.md

Add section on CLI array parameters:

```markdown
### CLI Array Parameters

Commands that accept arrays (like `media`, `labels`, `files`) support multiple values via repeated flags:

```bash
# ✅ Industry standard (Git, Docker, npm):
./jtag chat/send --message="Test" --media img1.png --media img2.jpg

# ✅ Also works - JSON arrays:
./jtag chat/send --message="Test" --media='["img1.png","img2.jpg"]'
```

**Rule:** If you repeat a flag, values accumulate into an array. Single flags remain single values.
```

---

### 2. Command-Specific READMEs

Update docs for commands with array params:

- `commands/chat/send/README.md`
- `commands/media/*/README.md`
- `commands/file/*/README.md`
- `commands/git/issue/create/README.md`

---

### 3. Help Output

```bash
./jtag help chat/send

# Should show:
Usage: ./jtag chat/send [options]

Options:
  --message <text>     Message text (required)
  --room <name>        Room name (default: "general")
  --media <path>       Media file path (can be repeated for multiple files)

Examples:
  # Single image:
  ./jtag chat/send --message="Test" --media screenshot.png

  # Multiple images:
  ./jtag chat/send --message="Test" \
    --media image1.png \
    --media image2.jpg \
    --media image3.webp
```

---

## Future Enhancements

### 1. Glob Expansion

```bash
# Shell glob expansion (already works via bash):
./jtag chat/send --message="Test" --media /test-images/*.png

# Native glob support (future):
./jtag chat/send --message="Test" --media="/test-images/*.png"
```

**Implementation:** Use `glob` library to expand patterns before processing.

---

### 2. Stdin for Arrays

```bash
# Pipe file list into command:
find /test-images -name "*.png" | ./jtag chat/send --message="Test" --media=@stdin
```

**Use Case:** Processing large lists of files.

---

### 3. File-Based Parameter Input

```bash
# Read parameters from file:
./jtag chat/send @params.json

# Where params.json contains:
{
  "message": "Test",
  "media": ["img1.png", "img2.jpg"]
}
```

**Use Case:** Complex commands with many parameters.

---

## Summary

**The Fix:**
1. **CLI Layer:** Accumulate repeated flags into arrays
2. **Commands:** Receive properly typed parameters (no change needed)
3. **Help System:** Show context-appropriate examples
4. **Persona Tools:** Already use JSON arrays (no change needed)

**Backward Compatibility:**
- Single values remain single values
- JSON arrays still work
- Existing commands unaffected

**Testing:**
- Unit tests for CLI parser
- Integration tests for commands with array params
- Manual testing with real media files

**Next Steps:**
1. Implement Phase 1 (CLI parser changes in `cli.ts`)
2. Test with `chat/send` command
3. Add parameter metadata to high-traffic commands
4. Update documentation

---

## Files to Modify

1. **`cli.ts`** (lines 159-205) - Add accumulation logic
2. **`commands/chat/send/shared/ChatSendTypes.ts`** - Add parameter metadata export
3. **`commands/help/server/HelpServerCommand.ts`** - Context-aware examples
4. **`docs/CLAUDE.md`** - Document array parameter syntax
5. **`tests/unit/cli.test.ts`** - Add array parsing tests (new file)
6. **`tests/integration/chat-send-media.test.ts`** - Add integration tests (new file)
