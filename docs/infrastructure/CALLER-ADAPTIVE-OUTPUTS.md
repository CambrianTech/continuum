# Caller-Adaptive Command Output Architecture

**Last Updated**: 2025-11-21
**Status**: Design Pattern - Implementation Pending

## Overview

Commands in the JTAG system should return **output optimized for the caller type**, ensuring each caller receives data in the most useful format for their capabilities and context.

**Core Principle**: One command implementation serves all caller types, but adapts result population based on who's calling.

## The Three Caller Types

### 1. PersonaUser (AI Agents)
**Capabilities**: Vision models, code parsing, audio processing, structured data analysis

**What they need**:
- **Rich media data**: Image/audio bytes fed directly into cognition systems
- **Parsed content**: AST trees, structured data objects, analyzed output
- **Context for reasoning**: Metadata, relationships, provenance
- **Machine-readable formats**: JSON, structured objects, type-safe data

**Example screenshot result**:
```typescript
ScreenshotResult {
  filepath: "/path/screenshot.png",     // ← Reference for logs/debugging
  media: MediaItem {                     // ← Actual visual data
    type: "image/png",
    data: Uint8Array<...>,              // ← Fed into vision model
    dimensions: { width: 1920, height: 1080 },
    metadata: { captureTime: 245, selector: "chat-widget" }
  },
  metadata: { ... }                      // ← Additional context
}
```

### 2. HumanUser (Interactive Users)
**Capabilities**: OS file viewers, terminal display, web browser

**What they need**:
- **File paths**: References to open in native OS viewers
- **Display-formatted data**: Pretty-printed text, tables, formatted output
- **Human-readable messages**: Natural language descriptions
- **Navigation aids**: Links, breadcrumbs, context

**Example screenshot result**:
```typescript
ScreenshotResult {
  filepath: "/path/screenshot.png",     // ← Opens in Preview/Photos/etc
  filename: "screenshot.png",            // ← Display name
  metadata: {                            // ← Human-readable info
    size: 245678,
    format: "png",
    dimensions: "1920x1080"
  }
  // media: undefined                    ← No bytes needed
}
```

### 3. Script (Programmatic Callers)
**Capabilities**: Data processing, automation, testing, CI/CD pipelines

**What they need**:
- **Structured data**: Consistent schemas, predictable formats
- **Exit codes and status**: Success/failure indicators
- **Machine-parseable output**: JSON, CSV, structured text
- **Deterministic behavior**: Same input = same output format

**Example screenshot result**:
```typescript
ScreenshotResult {
  success: true,
  filepath: "/path/screenshot.png",     // ← Absolute path for processing
  metadata: {
    size: 245678,
    width: 1920,
    height: 1080,
    format: "png"
  },
  // Omit visual data (too large), omit display formatting
}
```

## Architecture Pattern

### Detection Strategy

Commands detect caller type via the `JTAGContext` passed in params:

```typescript
interface JTAGContext {
  userId: UUID;           // Who is calling
  sessionId: UUID;        // Which session/connection
  callerType?: 'persona' | 'human' | 'script';  // Explicit type hint
  capabilities?: {        // What caller can process
    vision?: boolean;
    audio?: boolean;
    parsing?: boolean;
    display?: 'terminal' | 'browser' | 'none';
  };
}
```

### Implementation Pattern

Commands implement a **two-phase result construction**:

#### Phase 1: Core Execution (Caller-Agnostic)
```typescript
async execute(params: CommandParams): Promise<CommandResult> {
  // 1. Perform the core operation (same for all callers)
  const screenshot = await this.captureScreenshot(params);

  // 2. Save artifacts (same for all callers)
  const filepath = await this.saveToFile(screenshot);

  // 3. Gather metadata (same for all callers)
  const metadata = this.extractMetadata(screenshot);

  // 4. Build base result
  const baseResult = {
    success: true,
    filepath,
    filename: path.basename(filepath),
    metadata
  };

  // Phase 2: Adapt output based on caller
  return await this.adaptResultForCaller(baseResult, params.context, screenshot);
}
```

#### Phase 2: Caller-Adaptive Population
```typescript
private async adaptResultForCaller(
  baseResult: ScreenshotResult,
  context: JTAGContext,
  screenshot: Buffer
): Promise<ScreenshotResult> {
  const callerType = await this.detectCallerType(context);

  switch (callerType) {
    case 'persona':
      // Populate media field with actual bytes for vision
      return {
        ...baseResult,
        media: {
          type: 'image/png',
          data: new Uint8Array(screenshot),
          dimensions: {
            width: baseResult.metadata.width,
            height: baseResult.metadata.height
          }
        }
      };

    case 'human':
      // Just filepath and human-readable metadata
      return {
        ...baseResult,
        // media remains undefined
      };

    case 'script':
      // Structured data only, no display formatting
      return {
        success: baseResult.success,
        filepath: baseResult.filepath,
        metadata: {
          size: baseResult.metadata.size,
          width: baseResult.metadata.width,
          height: baseResult.metadata.height,
          format: baseResult.metadata.format
        }
      };
  }
}

private async detectCallerType(context: JTAGContext): Promise<CallerType> {
  // 1. Check explicit hint
  if (context.callerType) {
    return context.callerType;
  }

  // 2. Lookup user entity
  const user = await this.getUserById(context.userId);

  if (user instanceof PersonaUser) {
    return 'persona';
  } else if (user instanceof HumanUser) {
    return 'human';
  } else {
    // Default to script for unknown callers (CLI, tests, etc.)
    return 'script';
  }
}
```

## Command-Specific Examples

### Screenshot Command

| Caller Type | filepath | filename | media | metadata |
|-------------|----------|----------|-------|----------|
| PersonaUser | ✅ (ref) | ✅ (display) | ✅ (bytes for vision) | ✅ (full) |
| HumanUser | ✅ (opens in viewer) | ✅ (display) | ❌ | ✅ (human-readable) |
| Script | ✅ (processing) | ✅ (path.basename) | ❌ | ✅ (structured only) |

### File Read Command

**Base Operation**: Read file from disk

**PersonaUser Result**:
```typescript
{
  filepath: "/src/PersonaUser.ts",
  content: "class PersonaUser...",    // ← Raw text
  parsed: {                           // ← AST tree for code analysis
    imports: [...],
    classes: [...],
    functions: [...]
  },
  metadata: {
    language: "typescript",
    loc: 1850,
    complexity: "high"
  }
}
```

**HumanUser Result**:
```typescript
{
  filepath: "/src/PersonaUser.ts",
  content: "class PersonaUser...",    // ← Syntax-highlighted display
  metadata: {
    size: "45.2 KB",                 // ← Human-readable units
    lines: 1850,
    language: "TypeScript"
  }
}
```

**Script Result**:
```typescript
{
  filepath: "/src/PersonaUser.ts",
  content: "class PersonaUser...",    // ← Raw text
  metadata: {
    size: 45234,                     // ← Machine units (bytes)
    loc: 1850,
    encoding: "utf-8"
  }
}
```

### Data Query Command

**Base Operation**: Query database

**PersonaUser Result**:
```typescript
{
  items: [...],                       // ← Structured objects
  relationships: {                    // ← Graph for reasoning
    users: [...],
    messages: [...],
    connections: [...]
  },
  metadata: {
    totalCount: 150,
    schema: {...}                    // ← Type info for understanding
  }
}
```

**HumanUser Result**:
```typescript
{
  items: [...],                       // ← Structured objects
  display: {                          // ← Formatted for terminal
    table: "┌──────┬──────┐\n...",
    summary: "150 users found"
  },
  metadata: {
    totalCount: 150,
    page: "1 of 15"
  }
}
```

**Script Result**:
```typescript
{
  items: [...],                       // ← Just the data
  metadata: {
    totalCount: 150,
    hasMore: true,
    nextCursor: "abc123"
  }
}
```

### Shell Execute Command

**Base Operation**: Run shell command

**PersonaUser Result**:
```typescript
{
  stdout: "...",
  stderr: "...",
  exitCode: 0,
  parsed: {                           // ← Structured parse of output
    errors: [...],
    warnings: [...],
    summary: {...}
  }
}
```

**HumanUser Result**:
```typescript
{
  stdout: "...",                      // ← ANSI-colored for terminal
  stderr: "...",                      // ← Error formatting
  exitCode: 0,
  display: "✅ Command succeeded"    // ← Human message
}
```

**Script Result**:
```typescript
{
  stdout: "...",                      // ← Raw text
  stderr: "...",                      // ← Raw text
  exitCode: 0                         // ← Machine-readable status
}
```

## Implementation Strategy

### Phase 1: Base Infrastructure (Week 1)

1. **Extend JTAGContext**:
   ```typescript
   interface JTAGContext {
     userId: UUID;
     sessionId: UUID;
     callerType?: CallerType;
     capabilities?: CallerCapabilities;
   }
   ```

2. **Create CallerDetector utility**:
   ```typescript
   class CallerDetector {
     static async detect(context: JTAGContext): Promise<CallerType>;
     static async getCapabilities(userId: UUID): Promise<CallerCapabilities>;
   }
   ```

3. **Add CommandBase.adaptResult() helper**:
   ```typescript
   abstract class CommandBase {
     protected async adaptResultForCaller<R extends CommandResult>(
       baseResult: R,
       context: JTAGContext,
       richData?: any
     ): Promise<R>;
   }
   ```

### Phase 2: High-Value Commands (Week 2-3)

Implement caller-adaptive output for:
1. **screenshot** - Most impactful (PersonaUsers can't see current screenshots)
2. **code/read** - Enable PersonaUsers to parse code properly
3. **data/list** - Optimize PersonaUser reasoning over data
4. **shell/execute** - Structured output parsing for PersonaUsers

### Phase 3: Remaining Commands (Week 4+)

Roll out pattern to all commands systematically.

## Design Principles

### 1. **Everyone Gets a Reference**
All callers get filepath/identifier for logging and debugging:
```typescript
// Always include
result.filepath = "/path/to/artifact";
```

### 2. **Graceful Degradation**
If capability detection fails, default to script output (safest):
```typescript
const callerType = await detectCallerType(context) ?? 'script';
```

### 3. **No Duplicate Work**
Core operation runs once, only result population differs:
```typescript
// ✅ Good
const data = await this.fetchData();  // Once
return adaptForCaller(data);          // Adapt

// ❌ Bad
if (isPersona) {
  return await this.fetchDataForPersona();
} else {
  return await this.fetchDataForHuman();
}
```

### 4. **Explicit Over Implicit**
Allow callers to request specific format via params:
```typescript
interface ScreenshotParams {
  resultType?: 'file' | 'bytes' | 'both';  // Override detection
}
```

### 5. **Type Safety**
Use TypeScript to ensure result fields are populated correctly:
```typescript
type PersonaScreenshotResult = ScreenshotResult & Required<Pick<ScreenshotResult, 'media'>>;
type HumanScreenshotResult = ScreenshotResult & { media?: never };
```

## Testing Strategy

### Unit Tests
```typescript
describe('CallerDetector', () => {
  it('detects PersonaUser from context', async () => {
    const context = createContext({ userId: personaUserId });
    expect(await CallerDetector.detect(context)).toBe('persona');
  });

  it('defaults to script for unknown users', async () => {
    const context = createContext({ userId: unknownUserId });
    expect(await CallerDetector.detect(context)).toBe('script');
  });
});
```

### Integration Tests
```typescript
describe('Screenshot caller adaptation', () => {
  it('populates media field for PersonaUser', async () => {
    const params = createScreenshotParams({ context: personaContext });
    const result = await Commands.execute<ScreenshotResult>('screenshot', params);

    expect(result.filepath).toBeDefined();
    expect(result.media).toBeDefined();
    expect(result.media.data).toBeInstanceOf(Uint8Array);
  });

  it('omits media field for HumanUser', async () => {
    const params = createScreenshotParams({ context: humanContext });
    const result = await Commands.execute<ScreenshotResult>('screenshot', params);

    expect(result.filepath).toBeDefined();
    expect(result.media).toBeUndefined();
  });
});
```

## Future Enhancements

### 1. Capability Negotiation
```typescript
// PersonaUser advertises capabilities on connection
Events.emit('user:capabilities', {
  userId: personaUserId,
  vision: { models: ['gpt-4-vision', 'claude-3-opus'] },
  audio: { formats: ['mp3', 'wav'] },
  parsing: { languages: ['typescript', 'python', 'rust'] }
});

// Commands check capabilities before expensive operations
if (context.capabilities?.vision?.models.includes('gpt-4-vision')) {
  result.media = await this.prepareForGPT4Vision(screenshot);
}
```

### 2. Adaptive Quality/Size
```typescript
// Reduce image quality for low-bandwidth PersonaUsers
if (callerCapabilities.bandwidth === 'low') {
  result.media.data = await compressImage(screenshot, { quality: 0.6 });
}
```

### 3. Multi-Format Results
```typescript
// Return multiple formats, let caller pick
result.formats = {
  png: { filepath: "...", bytes: ... },
  jpeg: { filepath: "...", bytes: ... },
  webp: { filepath: "...", bytes: ... }
};
```

### 4. Streaming for Large Data
```typescript
// Stream large media to PersonaUsers instead of buffering
result.mediaStream = createReadStream(filepath);
```

## Migration Path

**Backwards Compatibility**: Existing callers continue working during migration:
- Commands without adaptation return base result (script-like output)
- New capability fields are optional
- Callers gradually adopt richer result types

**No Breaking Changes**: All changes are additive:
- ✅ Add optional fields to results
- ✅ Add optional params for format hints
- ❌ Don't remove existing fields
- ❌ Don't change existing field types

## References

- `commands/screenshot/shared/ScreenshotTypes.ts:185` - `media?: MediaItem` field (currently unpopulated)
- `system/core/types/JTAGTypes.ts` - Core context and result types
- `system/user/shared/BaseUser.ts` - User type hierarchy
- `system/data/entities/ChatMessageEntity.ts:MediaItem` - Media data structure

## Questions to Resolve

1. **Performance**: Should we lazy-load rich data (parse on-demand) or eagerly populate?
2. **Caching**: Can we cache parsed results for repeated access by PersonaUsers?
3. **Size Limits**: What's the maximum size for `media.data` before we must use streaming?
4. **Format Support**: Which media formats should PersonaUsers' vision systems accept?
5. **Fallback Strategy**: What if PersonaUser's vision model is unavailable? Store for later? Skip?

---

**Status**: Documentation complete, ready for implementation. Next step: Extend JTAGContext and create CallerDetector utility.
