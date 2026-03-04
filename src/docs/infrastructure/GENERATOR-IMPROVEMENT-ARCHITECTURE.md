# Generator Improvement Architecture

**Status**: Design Phase
**Target**: Phase 1+2 Implementation (Spec Abstraction + Extraction)
**Goal**: Make command generation feel like "filling out a form" rather than "writing JSON"

---

## Problem Statement

**Current Pain Points:**
1. **JSON Hell**: Creating command specs manually is verbose and error-prone
2. **No Learning Path**: Can't see how existing commands are structured
3. **No Validation**: Errors only discovered when generator runs
4. **Hard to Remember**: What fields are required? What's the exact format?
5. **No Templates**: Can't easily copy/modify existing command patterns

**User Feedback:**
> "the generator seemed awfully hard for you to both remember to use and perhaps a pita to structure the json properly"

---

## Solution Architecture

### Phase 1: Spec Abstraction Layer

**Create reusable TypeScript interfaces for all spec types:**

```typescript
// system/generator/shared/specs/CommandSpec.ts
export interface CommandSpec {
  name: string;                    // "decision/finalize"
  description: string;              // Human-readable description
  params: ParamSpec[];              // Input parameters
  results: ResultSpec[];            // Output results
  examples?: string[];              // CLI usage examples
  accessLevel?: 'ai-safe' | 'admin-only' | 'public';
}

export interface ParamSpec {
  name: string;                     // Parameter name
  type: string;                     // TypeScript type
  description: string;              // What this param does
  required: boolean;                // Is it required?
  defaultValue?: any;               // Optional default
}

export interface ResultSpec {
  name: string;                     // Result field name
  type: string;                     // TypeScript type
  description: string;              // What this field means
}

// Validation & Serialization
export class SpecValidator {
  static validate(spec: CommandSpec): ValidationResult;
  static validateParams(params: ParamSpec[]): ValidationResult;
  static validateResults(results: ResultSpec[]): ValidationResult;
}

export class SpecSerializer {
  static toJSON(spec: CommandSpec, pretty?: boolean): string;
  static fromJSON(json: string): CommandSpec;
  static toFile(spec: CommandSpec, path: string): void;
  static fromFile(path: string): CommandSpec;
}
```

**Benefits:**
- Type safety for spec creation
- Validation before generation
- Reusable across all generator types (command, daemon, entity, widget)
- Versioning support (handle spec format changes)

---

### Phase 2: Spec Extraction (Reverse Engineering)

**Extract specs from existing commands:**

```typescript
// system/generator/shared/SpecExtractor.ts
export class SpecExtractor {
  /**
   * Extract command spec from existing command directory
   * Reads: Types file, README, Server implementation
   * Outputs: Valid CommandSpec JSON
   */
  static async extractCommand(commandName: string): Promise<CommandSpec> {
    // 1. Find command directory
    const commandDir = this.findCommandDirectory(commandName);

    // 2. Parse Types file for params/results
    const typesFile = path.join(commandDir, 'shared', '*Types.ts');
    const { params, results } = await this.parseTypesFile(typesFile);

    // 3. Parse README for description/examples
    const readmeFile = path.join(commandDir, 'README.md');
    const { description, examples } = await this.parseReadme(readmeFile);

    // 4. Detect access level from implementation
    const serverFile = path.join(commandDir, 'server', '*Command.ts');
    const accessLevel = await this.detectAccessLevel(serverFile);

    // 5. Build spec
    return {
      name: commandName,
      description,
      params,
      results,
      examples,
      accessLevel
    };
  }

  private static async parseTypesFile(path: string): Promise<{
    params: ParamSpec[];
    results: ResultSpec[];
  }> {
    // Use TypeScript compiler API to extract interface definitions
    // Look for: *Params extends CommandParams, *Result extends CommandResult
    // Extract: field names, types, JSDoc comments (descriptions)
  }

  private static async parseReadme(path: string): Promise<{
    description: string;
    examples: string[];
  }> {
    // Parse markdown README
    // Extract: First paragraph as description, ## Usage section for examples
  }
}
```

**New Command: `generate/extract`**

```bash
# Extract command spec to stdout
./jtag generate/extract --command="hello"

# Extract to file
./jtag generate/extract --command="decision/create" --output="/tmp/spec.json"

# Extract with pretty formatting
./jtag generate/extract --command="hello" --pretty
```

**Example Output (hello command):**

```json
{
  "name": "hello",
  "description": "Simple hello world command for testing",
  "params": [
    {
      "name": "name",
      "type": "string",
      "description": "Name to greet",
      "required": false,
      "defaultValue": "World"
    }
  ],
  "results": [
    {
      "name": "success",
      "type": "boolean",
      "description": "Whether the command succeeded"
    },
    {
      "name": "message",
      "type": "string",
      "description": "Hello world message"
    }
  ],
  "examples": [
    "./jtag hello",
    "./jtag hello --name=\"Joel\""
  ],
  "accessLevel": "ai-safe"
}
```

---

## Usage Patterns

### Pattern 1: Learn from Existing Commands

```bash
# "How did I structure decision/create?"
./jtag generate/extract --command="decision/create" --pretty

# Save as template
./jtag generate/extract --command="decision/create" --output="/tmp/template.json"

# Modify and regenerate
jq '.name = "decision/update"' /tmp/template.json | \
  ./jtag generate --spec=-
```

### Pattern 2: Clone and Modify

```bash
# Extract existing command
./jtag generate/extract --command="decision/create" > /tmp/spec.json

# Edit spec (change name, params, etc.)
vim /tmp/spec.json

# Generate new command from modified spec
./jtag generate --spec=/tmp/spec.json
```

### Pattern 3: Documentation

```bash
# Generate spec documentation for all commands
for cmd in decision/create decision/vote decision/finalize; do
  ./jtag generate/extract --command="$cmd" --output="docs/specs/$cmd.json"
done

# Now you have machine-readable command specs for docs generation
```

### Pattern 4: Validation (Detect Drift)

```bash
# Extract spec from implementation
./jtag generate/extract --command="hello" > /tmp/actual.json

# Compare with documented spec
diff /tmp/actual.json docs/specs/hello.json

# Detect when implementation drifts from spec
```

---

## Component Architecture

```
system/generator/
├── shared/
│   ├── specs/
│   │   ├── CommandSpec.ts         # NEW: TypeScript spec interfaces
│   │   ├── DaemonSpec.ts          # NEW: Daemon spec interfaces
│   │   ├── EntitySpec.ts          # NEW: Entity spec interfaces
│   │   └── WidgetSpec.ts          # NEW: Widget spec interfaces
│   ├── SpecValidator.ts           # NEW: Validation logic
│   ├── SpecSerializer.ts          # NEW: JSON serialization
│   └── SpecExtractor.ts           # NEW: Reverse engineer specs
├── commands/
│   └── extract/                   # NEW: generate/extract command
│       ├── shared/
│       │   └── ExtractTypes.ts
│       ├── server/
│       │   └── ExtractServerCommand.ts
│       ├── browser/
│       │   └── ExtractBrowserCommand.ts
│       └── README.md
├── templates/                     # EXISTING: Template files
│   ├── command/
│   ├── daemon/
│   ├── entity/
│   └── widget/
├── CommandGenerator.ts            # EXISTING: Enhanced with SpecValidator
└── TemplateLoader.ts              # EXISTING: Unchanged
```

---

## Implementation Plan

### Step 1: Spec Interfaces (30 minutes)

**Create**: `system/generator/shared/specs/CommandSpec.ts`
- Define interfaces: `CommandSpec`, `ParamSpec`, `ResultSpec`
- Add validation methods: `isRequired()`, `hasDefault()`, etc.
- Add helper methods: `toJSON()`, `fromJSON()`

**Create**: `system/generator/shared/SpecValidator.ts`
- Validate required fields
- Check type consistency
- Validate examples (parse as CLI args)

**Create**: `system/generator/shared/SpecSerializer.ts`
- JSON serialization with pretty-printing
- File I/O helpers
- Error handling

### Step 2: Extraction Engine (1 hour)

**Create**: `system/generator/shared/SpecExtractor.ts`
- TypeScript compiler API integration
- Parse interface definitions from Types files
- Extract JSDoc comments for descriptions
- Parse README markdown for examples
- Detect access level from implementation

### Step 3: Extract Command (30 minutes)

**Generate**: `./jtag generate/extract` command structure
- Params: `command: string`, `output?: string`, `pretty?: boolean`
- Results: `success: boolean`, `spec: CommandSpec`, `json: string`
- Server implementation calls `SpecExtractor.extractCommand()`

### Step 4: Enhance Existing Generator (30 minutes)

**Modify**: `generator/CommandGenerator.ts`
- Accept `CommandSpec` object instead of raw JSON
- Use `SpecValidator` before generation
- Better error messages (point to exact validation failure)

### Step 5: Documentation & Testing (30 minutes)

**Document**: Usage patterns in this file
**Test**: Extract hello command, verify output
**Test**: Extract → modify → regenerate workflow
**Update**: All generator READMEs to mention extraction

---

## Testing Strategy

### Unit Tests

```typescript
// Test spec validation
describe('SpecValidator', () => {
  it('should validate required fields', () => {
    const spec = { name: 'test' }; // Missing description
    const result = SpecValidator.validate(spec);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain('description is required');
  });
});

// Test spec extraction
describe('SpecExtractor', () => {
  it('should extract hello command spec', async () => {
    const spec = await SpecExtractor.extractCommand('hello');
    expect(spec.name).toBe('hello');
    expect(spec.params).toHaveLength(1);
    expect(spec.params[0].name).toBe('name');
  });
});
```

### Integration Tests

```bash
# Test extraction
./jtag generate/extract --command="hello" > /tmp/hello-spec.json

# Validate JSON
jq . /tmp/hello-spec.json

# Test regeneration (should match original)
./jtag generate --spec=/tmp/hello-spec.json
diff -r commands/hello commands/hello-regenerated
```

### End-to-End Test

```bash
# 1. Extract existing command
./jtag generate/extract --command="hello" --output="/tmp/hello.json"

# 2. Modify spec (add new param)
jq '.params += [{"name":"loud","type":"boolean","description":"Shout the greeting","required":false}]' \
  /tmp/hello.json > /tmp/hello-modified.json

# 3. Generate new command
./jtag generate --spec=/tmp/hello-modified.json --name="hello-loud"

# 4. Verify new command has additional param
grep "loud" commands/hello-loud/shared/HelloLoudTypes.ts
```

---

## Migration Path

### Phase 1: Extract All Existing Commands

```bash
# Create specs directory
mkdir -p docs/generator-specs

# Extract all commands
for cmd in $(./jtag list | grep -E '^\s+'); do
  ./jtag generate/extract --command="$cmd" \
    --output="docs/generator-specs/${cmd//\//-}.json"
done
```

### Phase 2: Validate Existing Commands

```bash
# For each command, verify spec matches implementation
for spec in docs/generator-specs/*.json; do
  cmd=$(basename "$spec" .json | tr '-' '/')
  ./jtag generate/validate --command="$cmd" --spec="$spec"
done
```

### Phase 3: Regenerate Commands (Optional)

```bash
# When templates improve, regenerate all commands
for spec in docs/generator-specs/*.json; do
  ./jtag generate --spec="$spec" --force
done
```

---

## Future Enhancements (Phase 3+)

### Interactive Mode

```bash
./jtag generate/command --interactive

# Prompts:
# > Command name: decision/finalize
# > Description: Close voting and calculate winner
# > Add parameter? (y/n): y
# > Parameter name: proposalId
# > Parameter type: string
# > Required? (y/n): y
# > Add another parameter? (y/n): n
# [... continues for results, examples ...]
```

### Template Support

```bash
# Clone decision/create pattern
./jtag generate/command --template="decision/create" --name="decision/update"
```

### Simplified CLI

```bash
# Declarative mode (parse inline params)
./jtag generate/command \
  --name="decision/finalize" \
  --params="proposalId:string!" \
  --results="winner:string,rounds:RoundResult[]"
```

### Validation Command

```bash
# Check if implementation matches spec
./jtag generate/validate --command="hello"
```

---

## Success Metrics

**Before:**
- Creating command spec: 10-15 minutes (error-prone)
- No way to learn from existing commands
- Generator errors cryptic and late

**After:**
- Extract existing spec: 5 seconds
- Modify spec: 2-3 minutes (JSON editing)
- Generate command: 10 seconds
- Total time: ~3 minutes (70% reduction)
- Learn from 100+ existing commands
- Validation errors immediate and clear

---

## Open Questions

1. **Spec Versioning**: How do we handle spec format changes over time?
   - **Answer**: Add `version` field to specs, migration scripts

2. **Complex Types**: How do we handle `DecisionOption[]` or custom types?
   - **Answer**: Allow import statements in spec, resolve from type registry

3. **Business Logic**: Can we extract TODO comments from implementation?
   - **Answer**: Yes, parse server file for TODO/FIXME comments

4. **Multiple Outputs**: Can we extract for daemons, entities, widgets?
   - **Answer**: Yes, similar extractors for each type

---

## Implementation Priority

**HIGH PRIORITY** (Do Now):
- ✅ Spec interfaces (CommandSpec, ParamSpec, ResultSpec)
- ✅ SpecValidator (validation logic)
- ✅ SpecExtractor (extraction from hello command)
- ✅ generate/extract command (CLI interface)

**MEDIUM PRIORITY** (Next Week):
- ⏳ Extract all existing commands to JSON
- ⏳ Validation command (detect drift)
- ⏳ Daemon/Entity/Widget extractors

**LOW PRIORITY** (Future):
- ⏳ Interactive mode
- ⏳ Template cloning
- ⏳ Simplified declarative CLI

---

## Related Documents

- [Command Generator Design](../generator/README.md) - Existing generator
- [UNIVERSAL-PRIMITIVES.md](UNIVERSAL-PRIMITIVES.md) - Command system architecture
- [ARCHITECTURE-RULES.md](ARCHITECTURE-RULES.md) - Type system rules

---

**Last Updated**: 2025-12-08
**Author**: Claude Code + Joel
**Status**: Design Complete, Ready for Implementation
