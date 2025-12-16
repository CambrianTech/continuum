# Design Principle: Natural Idioms at Every Layer

## Core Philosophy

> **"We shall strive for natural user|persona|widget|mcp|etc idioms, not force a square peg into a round hole."**

Every interface layer should use its **natural, expected format** - the idiom that users/systems at that layer already know and expect. Translation happens at boundaries, **never** forcing one layer's format onto another.

---

## Anti-Pattern: Forcing CLI JSON Syntax

**❌ BAD (forcing API format into CLI):**
```bash
# Making humans write JSON is a square peg in a round hole:
./jtag collaboration/chat/send --message="Test" '--media=["/path/img1.png","/path/img2.jpg"]'
```

**✅ GOOD (CLI uses CLI idioms):**
```bash
# Natural CLI convention (Git, Docker, npm all use this):
./jtag collaboration/chat/send --message="Test" --media /path/img1.png --media /path/img2.jpg
```

**Why it matters:**
- CLI users expect repeated flags, not JSON
- JSON requires quoting nightmares in shell
- Error-prone (missing quotes, escaping)
- Violates principle of least surprise

---

## The Natural Idiom Map

| Interface Layer | Natural Format | Example | Why |
|----------------|----------------|---------|-----|
| **CLI** | Repeated flags | `--media p1 --media p2` | Unix convention since 1970s |
| **Anthropic Tools** | JSON arrays | `{"media": ["p1","p2"]}` | Native Claude tool format |
| **OpenAI Tools** | JSON arrays | `{"media": ["p1","p2"]}` | Native GPT tool format |
| **TypeScript API** | Native arrays | `{ media: ["p1","p2"] }` | Type-safe code |
| **Widget Drag-Drop** | FileList → Array | `[File, File]` | Browser native |
| **MCP Servers** | JSON-RPC | `{"params": {"media": ["p1"]}}` | MCP protocol spec |
| **HTTP API** | JSON body | `{"media": ["p1","p2"]}` | REST/GraphQL standard |
| **Database** | Array columns | `media: ["p1","p2"]` | Native SQL/NoSQL |

**Key Insight:** Each layer has **evolved conventions** that users/systems expect. Fighting these conventions creates friction.

---

## Translation at Boundaries

```
┌─────────────────────────────────────────────────────────┐
│ USER INTERFACES (speak their own language)              │
├─────────────────────────────────────────────────────────┤
│ CLI:      --flag value1 --flag value2                   │
│ Widget:   FileList from <input multiple>                │
│ Persona:  Tool calls with JSON arrays                   │
│ MCP:      JSON-RPC params                               │
└────────────────────┬────────────────────────────────────┘
                     │
                     │ 🔄 TRANSLATION LAYER
                     │    (boundary adapters)
                     │
                     ▼
┌─────────────────────────────────────────────────────────┐
│ INTERNAL CANONICAL FORMAT (TypeScript types)            │
├─────────────────────────────────────────────────────────┤
│ interface Params {                                       │
│   media: string[]  // Always properly typed             │
│ }                                                        │
└─────────────────────────────────────────────────────────┘
```

**Translation Examples:**

### CLI → Internal
```typescript
// cli.ts boundary adapter
function parseCLIArgs(argv: string[]): CommandParams {
  // CLI idiom: --media p1 --media p2
  const params = {};
  for (const arg of argv) {
    if (arg.startsWith('--')) {
      const [key, value] = parseFlag(arg);
      // Accumulate repeated flags into arrays
      if (params[key]) {
        params[key] = Array.isArray(params[key])
          ? [...params[key], value]
          : [params[key], value];
      } else {
        params[key] = value;
      }
    }
  }
  return params; // → { media: ["p1", "p2"] }
}
```

### Widget → Internal
```typescript
// ChatWidget.ts boundary adapter
function handleFileDrop(files: FileList): MediaItem[] {
  // Widget idiom: Browser FileList
  return Array.from(files).map(file => ({
    name: file.name,
    data: file,
    type: file.type
  })); // → [MediaItem, MediaItem]
}
```

### Persona Tool → Internal
```typescript
// PersonaToolExecutor.ts boundary adapter
function executeToolCall(toolCall: AnthropicToolCall): CommandResult {
  // Persona idiom: AI provider's native tool format
  const { tool_name, tool_input } = toolCall;

  // tool_input is already JSON with arrays - no translation needed!
  return Commands.execute(tool_name, tool_input);
}
```

### MCP → Internal
```typescript
// MCPServer.ts boundary adapter
async function handleMCPRequest(req: JSONRPCRequest): Promise<JSONRPCResponse> {
  // MCP idiom: JSON-RPC 2.0 protocol
  const { method, params } = req;

  // Translate MCP method names to command names if needed
  const commandName = translateMCPMethod(method);
  return Commands.execute(commandName, params);
}
```

---

## Anti-Pattern Examples (What NOT to Do)

### ❌ Don't Force Tool Format into CLI

**BAD:**
```bash
# Making CLI users write tool JSON:
./jtag collaboration/chat/send '{"message":"Test","media":["p1","p2"]}'
```

**Why it's bad:** CLI users don't think in JSON. They think in flags and arguments.

---

### ❌ Don't Force CLI Format into Tools

**BAD:**
```json
// Making AI parse CLI-style strings:
{
  "tool_name": "chat/send",
  "tool_input": {
    "args": "--message Test --media p1 --media p2"
  }
}
```

**Why it's bad:** AI models expect structured JSON. String parsing is error-prone and loses type safety.

---

### ❌ Don't Force Single Format Everywhere

**BAD:**
```typescript
// One format to rule them all:
interface UniversalParams {
  rawString: string; // Everyone must parse this!
}
```

**Why it's bad:** Forces every layer to do parsing. Loses type safety. Creates coupling.

---

## The Right Way: Boundary Adapters

Each interface has an **adapter** that:

1. **Accepts** input in the layer's natural format
2. **Validates** according to that format's conventions
3. **Translates** to canonical internal format
4. **Passes** to the core system

```typescript
// Pattern: Adapter at each boundary
interface BoundaryAdapter<InputFormat, CanonicalFormat> {
  accept(input: InputFormat): void;
  validate(input: InputFormat): ValidationResult;
  translate(input: InputFormat): CanonicalFormat;
}

// Example: CLI Adapter
class CLIAdapter implements BoundaryAdapter<string[], CommandParams> {
  accept(argv: string[]): void {
    // Accept CLI-style arguments
  }

  validate(argv: string[]): ValidationResult {
    // Validate using CLI conventions (flags, positions, etc.)
  }

  translate(argv: string[]): CommandParams {
    // Translate repeated flags → arrays, etc.
    return parseCLIArgs(argv);
  }
}

// Example: Tool Adapter
class ToolAdapter implements BoundaryAdapter<ToolCall, CommandParams> {
  accept(toolCall: ToolCall): void {
    // Accept AI provider's native tool format
  }

  validate(toolCall: ToolCall): ValidationResult {
    // Validate using tool schema
  }

  translate(toolCall: ToolCall): CommandParams {
    // Tool input is already the right format - pass through!
    return toolCall.tool_input;
  }
}
```

---

## Benefits of Natural Idioms

### 1. **Discoverability**
Users don't need to "learn our special format" - they already know it from other tools.

```bash
# CLI users already know this from Git:
git add file1.txt file2.txt
docker run -v vol1 -v vol2 image

# So this feels natural:
./jtag collaboration/chat/send --media img1.png --media img2.png
```

---

### 2. **Reduced Cognitive Load**
Each interface uses familiar patterns. No mental translation needed.

```typescript
// TypeScript developers already know arrays:
Commands.execute('chat/send', {
  message: 'Test',
  media: ['img1.png', 'img2.png']  // ✅ Obvious
});

// vs forcing CLI strings:
Commands.execute('chat/send', {
  args: '--message Test --media img1.png --media img2.png'  // ❌ Weird
});
```

---

### 3. **Better Error Messages**
Validation happens in the format users understand:

```bash
# CLI errors speak CLI language:
❌ Error: Parameter 'media' expected file path, got directory
./jtag collaboration/chat/send --media /path/to/directory

# Not cryptic JSON errors:
❌ Error: JSON parsing failed at position 42: unexpected token '/'
```

---

### 4. **Tooling Compatibility**
Standard formats work with existing tools:

```bash
# Shell completion works naturally:
./jtag collaboration/chat/send --media <TAB>
# → Shows files, because shells know --flag <value> pattern

# vs breaking completion:
./jtag collaboration/chat/send --media='["<TAB>
# → Shell has no idea what to complete inside JSON
```

---

### 5. **Future-Proof**
When new interfaces emerge, they use their own idioms without breaking others.

**Example:** Voice interface added later:
```
User: "Send chat message 'Hello' with images photo1 and photo2"
        ↓
VoiceAdapter translates to:
{
  message: "Hello",
  media: ["photo1.jpg", "photo2.jpg"]
}
```

Voice interface doesn't force CLI flags or JSON syntax on users!

---

## Implementation Checklist

When adding a new interface or parameter type:

- [ ] **Identify the natural idiom** for this interface
  - What do users expect in this context?
  - What do similar tools use?
  - What's the industry standard?

- [ ] **Create boundary adapter** if needed
  - Does translation logic exist?
  - Is it cleanly separated from core logic?

- [ ] **Validate in natural format** before translation
  - Errors reference the format users understand
  - Don't leak internal details

- [ ] **Document the idiom** clearly
  - Show examples in that format
  - Don't show "internal format" to users

- [ ] **Test with real users/systems** of that interface
  - Does it feel natural?
  - Are there surprises or friction?

---

## Case Study: chat/send media Parameter

### Before (Forcing JSON into CLI)

```bash
# ❌ Square peg in round hole:
./jtag collaboration/chat/send --message="Test" '--media=["/path/img1.png","/path/img2.jpg"]'
```

**Problems:**
- Requires quoting the entire array
- Escaping nightmare with spaces in paths
- Not how any Unix tool works
- Error-prone to type
- Shell completion doesn't work

---

### After (Natural Idioms)

**CLI (repeated flags):**
```bash
# ✅ Follows Git/Docker/npm convention:
./jtag collaboration/chat/send --message="Test" --media /path/img1.png --media /path/img2.jpg
```

**TypeScript API (native arrays):**
```typescript
// ✅ Natural TypeScript:
Commands.execute('chat/send', {
  message: 'Test',
  media: ['/path/img1.png', '/path/img2.jpg']
});
```

**Persona Tool (AI provider format):**
```json
// ✅ Natural Claude/GPT tool format:
{
  "tool_name": "chat/send",
  "tool_input": {
    "message": "Test",
    "media": ["/path/img1.png", "/path/img2.jpg"]
  }
}
```

**Widget (browser native):**
```typescript
// ✅ Natural browser event:
async function handleDrop(e: DragEvent) {
  const files = Array.from(e.dataTransfer.files);
  await Commands.execute('chat/send', {
    message: 'Test',
    media: files  // FileList → processed to paths
  });
}
```

**Result:** Each interface uses its natural format. No square pegs forced into round holes.

---

## Quotes from the Team

> "We shall strive for natural user|persona|widget|mcp|etc idioms, not force a square peg into a round hole."
>
> — Design principle established 2025-11-26

> "By its own idiom. We have tools or whatever format the base models prefer for each persona, and in the CLI, convention of expectations there, with natural frictionless control."
>
> — Clarifying the vision

---

## Related Documents

- **[CLI Array Parameters](./CLI-ARRAY-PARAMETERS.md)** - Detailed implementation for CLI repeated flags
- **[Vision Media Architecture](./VISION-MEDIA-ARCHITECTURE.md)** - Media handling across layers
- **[Universal Primitives](./UNIVERSAL-PRIMITIVES.md)** - Commands.execute() and Events for internal communication

---

## Summary

**The Principle:**
Every interface layer uses its natural, expected format. Translation happens at boundaries, never inside core logic.

**The Pattern:**
```
Natural Idiom → Boundary Adapter → Canonical Format → Core System
```

**The Benefit:**
Frictionless user experience at every layer. No "learning our special format." Users already know how to use it because it matches their existing mental models.

**The Test:**
If a new user says "why doesn't it work like [familiar tool]?" - you've probably forced a square peg into a round hole. Fix the idiom to match expectations.
