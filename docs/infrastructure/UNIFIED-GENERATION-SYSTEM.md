# Unified Code Generation System - Architecture

## Vision

A template-driven generation system that allows both humans and AIs to create, modify, and enrich code artifacts (commands, widgets, daemons) through simple, declarative specifications. This solves your Options A-D (tool descriptions, filtering, examples, documentation) as a unified system.

## Core Philosophy

**"Generate once, enrich continuously"** - The system creates complete, working artifacts from templates, then provides tools to iteratively improve them with better descriptions, examples, access levels, and documentation.

---

## System Architecture

### 1. Existing Foundation (What We Have)

```
generator/
├── generate-structure.ts    # Universal Module Pattern scanner
├── generate-command-schemas.ts  # Command schema generator
├── EventConstantsGenerator.ts   # Event constants
├── generate-version.ts      # Version tracking
├── types/GeneratorTypes.ts  # Type definitions
└── utils/                   # Helper utilities
```

**Strengths:**
- Sophisticated Universal Module Pattern understanding
- Type-safe schema generation
- Proven pattern for registry generation

**Gaps:**
- No template system for creating NEW artifacts
- No metadata enrichment (descriptions, examples, accessLevel)
- Not AI-callable (no command interface)
- Doesn't handle migrations/updates

### 2. New Unified System (What We Need)

```
commands/gen/                # AI-callable generation suite
├── command/                 # Generate commands
│   ├── shared/GenCommandTypes.ts
│   ├── server/GenCommandServer.ts
│   └── browser/GenCommandBrowser.ts
├── widget/                  # Generate widgets
├── daemon/                  # Generate daemons
├── enrich/                  # Enrich existing modules
│   ├── shared/GenEnrichTypes.ts
│   └── server/GenEnrichServer.ts
└── man/                     # Generate manual pages
    ├── shared/GenManTypes.ts
    └── server/GenManServer.ts

generator/templates/         # Template definitions
├── command.template.ts      # Command scaffold
├── widget.template.ts       # Widget scaffold
├── daemon.template.ts       # Daemon scaffold
└── README.template.md       # Documentation template

generator/enrichers/         # Metadata enrichment
├── DescriptionEnricher.ts   # Better descriptions
├── ExampleGenerator.ts      # Usage examples
├── AccessLevelTagger.ts     # AI-safe/internal tagging
└── ManualGenerator.ts       # man page generation
```

---

## Key Components

### A. Template System

**Purpose:** Generate complete, working modules from minimal input

**Example Usage:**
```bash
# Human via CLI
./jtag gen/command --name="user/promote" --description="Promote user to admin" --suite="user"

# AI via tool
<tool_use>
  <tool_name>gen/command</tool_name>
  <parameters>
    <name>user/promote</name>
    <description>Promote user to admin role</description>
    <suite>user</suite>
  </parameters>
</tool_use>
```

**Generated Structure:**
```
commands/user/promote/
├── shared/
│   └── UserPromoteTypes.ts    # Interface with CommandParams/CommandResult
├── server/
│   └── UserPromoteServer.ts   # Server-side logic
├── browser/
│   └── UserPromoteBrowser.ts  # Browser wrapper
└── README.md                   # Basic documentation
```

**Template Variables:**
- `{{COMMAND_NAME}}` - Full command path (e.g., "user/promote")
- `{{COMMAND_CLASS}}` - PascalCase class name (e.g., "UserPromote")
- `{{DESCRIPTION}}` - Human-readable description
- `{{SUITE}}` - Command suite/category
- `{{PARAMS}}` - Parameter interface (optional)
- `{{RESULT}}` - Result interface (optional)

### B. Enrichment System

**Purpose:** Add metadata to existing modules without manual editing

**Example Usage:**
```bash
# Add better description
./jtag gen/enrich --target="screenshot" --description="Capture browser viewport as PNG image"

# Add access level for AI filtering
./jtag gen/enrich --target="data/nuke" --accessLevel="internal" --reason="Destructive operation"

# Add usage examples
./jtag gen/enrich --target="chat/send" --examples='[
  {
    "description": "Send message to general room",
    "params": {"room": "general", "message": "Hello team"},
    "expectedResult": "Message posted successfully"
  }
]'
```

**Enrichment Targets:**
1. **Descriptions** - Replace generic "X command" with meaningful explanations
2. **Access Levels** - Tag commands as `ai-safe`, `internal`, `system`, `dangerous`
3. **Examples** - Concrete usage patterns for PersonaToolDefinitions
4. **Man Pages** - Comprehensive documentation accessible via `./jtag man screenshot`

### C. Documentation Command (docs/read or help/command)

**Purpose:** Display command documentation from README.md

**Simple approach - just read the README:**
```bash
$ ./jtag docs/read --command=screenshot
# Returns formatted README.md from commands/screenshot/README.md

$ ./jtag help screenshot
# Alias for docs/read --command=screenshot
```

**Why README.md instead of "man pages":**
- Already exists in every command directory
- Familiar format (markdown)
- Version controlled with the code
- Can include code examples, usage notes, access level
- No confusion about "man" terminology

**README Structure (from template):**
```markdown
# Screenshot

Capture browser viewport or DOM element as PNG image

## Usage

\`\`\`typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('screenshot', {
  querySelector: 'chat-widget'
});
\`\`\`

## Parameters

- **querySelector** (string, optional): CSS selector for element to capture (default: body)
- **filename** (string, optional): Output filename for CLI usage

## Result

- **success** (boolean): Whether operation succeeded
- **base64** (string): Base64-encoded PNG image data
- **width** (number): Image width in pixels
- **height** (number): Image height in pixels

## Access Level

\`ai-safe\` - Safe for AI tool calling

## Examples

Capture entire page:
\`\`\`bash
./jtag interface/screenshot
\`\`\`

Capture specific element:
\`\`\`bash
./jtag interface/screenshot --querySelector="chat-widget"
\`\`\`

AI tool usage:
\`\`\`xml
<tool_use>
  <tool_name>screenshot</tool_name>
  <parameters>
    <querySelector>chat-widget</querySelector>
  </parameters>
</tool_use>
\`\`\`

## Notes

- When called by PersonaUser, returns base64 PNG optimized for AI vision
- Supports any valid CSS selector
```

### D. Migration System

**Purpose:** Update existing modules to add new fields/patterns

**Example Usage:**
```bash
# Add accessLevel field to all command Types files
./jtag gen/migrate --pattern="commands/**/shared/*Types.ts" \
  --operation="addField" \
  --field="accessLevel" \
  --type="'ai-safe' | 'internal' | 'system' | 'dangerous'" \
  --defaultValue="'ai-safe'"
```

**Migration Operations:**
- `addField` - Add new interface field with default
- `updateDoc` - Add/update JSDoc comments
- `replacePattern` - Regex-based code transformation
- `restructure` - Move files to new Universal Module Pattern

---

## Integration with PersonaToolDefinitions

**Current Problem:**
```typescript
// PersonaToolDefinitions.ts line 123
toolCache = result.commands.map((cmd: CommandSignature) => convertCommandToTool(cmd));
// Generic descriptions, no examples, no filtering
```

**After Enrichment:**
```typescript
// Commands now have metadata
toolCache = result.commands
  .filter(cmd => cmd.accessLevel === 'ai-safe')  // Only AI-safe tools
  .map((cmd: CommandSignature) => ({
    name: cmd.name,
    description: cmd.description,  // NOW has meaningful description
    category: inferCategoryFromName(cmd.name),
    permissions: [cmd.accessLevel + ':execute'],
    parameters: { /* ... */ },
    examples: cmd.examples || []  // NOW has concrete examples
  }));
```

**Enriched Command Example:**
```typescript
// commands/screenshot/shared/ScreenshotTypes.ts (after enrichment)
export interface ScreenshotParams extends CommandParams {
  /** CSS selector for element to capture (default: body) */
  querySelector?: string;
  /** Output filename (optional, for CLI usage) */
  filename?: string;
}

export interface ScreenshotResult extends CommandResult {
  /** Base64-encoded PNG image data */
  base64?: string;
  /** Image dimensions */
  width: number;
  height: number;
  /** MIME type (always image/png) */
  mimeType: 'image/png';
}

// Metadata for PersonaToolDefinitions
export const SCREENSHOT_METADATA = {
  accessLevel: 'ai-safe' as const,
  description: 'Capture browser viewport or DOM element as PNG image',
  examples: [
    {
      description: 'Capture entire page',
      params: {},
      expectedResult: 'Returns base64 PNG with viewport dimensions'
    },
    {
      description: 'Capture specific element',
      params: { querySelector: 'chat-widget' },
      expectedResult: 'Returns base64 PNG of chat widget only'
    }
  ],
  usageNotes: [
    'When called by PersonaUser, automatically returns media format for AI vision',
    'Supports any valid CSS selector',
    'Images are optimized for AI analysis (not full quality)'
  ]
};
```

---

## Implementation Phases

### Phase 1: Template System (Foundation)
**Goal:** Generate working modules from templates

**Tasks:**
1. Create `commands/gen/command/` with Types/Server/Browser
2. Build template engine (simple variable replacement)
3. Create command.template.ts with Universal Module Pattern
4. Test: `./jtag gen/command --name="test/foo" --description="Test"`
5. Verify generated module compiles and registers

**Deliverable:** Working `gen/command` that creates complete command structures

### Phase 2: Enrichment System
**Goal:** Add metadata to existing modules

**Tasks:**
1. Create `commands/gen/enrich/` with enrichment logic
2. Build DescriptionEnricher (updates JSDoc comments)
3. Build AccessLevelTagger (adds metadata exports)
4. Build ExampleGenerator (adds example arrays)
5. Test on 5-10 key commands (screenshot, chat/send, data/list)

**Deliverable:** `gen/enrich` command that updates module metadata

### Phase 3: Man Page System
**Goal:** Unix-style documentation

**Tasks:**
1. Create `commands/man/` to query enriched metadata
2. Build ManualGenerator to format man pages
3. Integrate with `gen/enrich` to auto-generate man content
4. Test: `./jtag man screenshot` shows formatted docs

**Deliverable:** `man` command with rich documentation

### Phase 4: PersonaToolDefinitions Integration
**Goal:** Better AI tool discovery

**Tasks:**
1. Update PersonaToolDefinitions to read enriched metadata
2. Implement accessLevel filtering
3. Add examples to tool definitions
4. Update persona system prompts with tool usage guide

**Deliverable:** AIs get curated, well-documented tools

### Phase 5: Widget/Daemon Templates
**Goal:** Complete generation coverage

**Tasks:**
1. Create widget.template.ts
2. Create daemon.template.ts
3. Implement `gen/widget` and `gen/daemon` commands
4. Document template customization

**Deliverable:** Full artifact generation suite

### Phase 6: Migration System (Advanced)
**Goal:** Bulk updates and refactoring

**Tasks:**
1. Build AST-based code transformer
2. Implement common migration patterns
3. Create `gen/migrate` command
4. Test: Add accessLevel to all existing commands

**Deliverable:** Automated code migrations

---

## Example Workflows

### Workflow 1: Human Creates New Command
```bash
$ ./jtag gen/command --name="analytics/report" \
    --description="Generate analytics report" \
    --params='{"startDate": "string", "endDate": "string"}' \
    --suite="analytics"

✓ Generated commands/analytics/report/shared/AnalyticsReportTypes.ts
✓ Generated commands/analytics/report/server/AnalyticsReportServer.ts
✓ Generated commands/analytics/report/browser/AnalyticsReportBrowser.ts
✓ Generated commands/analytics/report/README.md
✓ Registered in command registry

Next steps:
  1. Implement business logic in AnalyticsReportServer.ts
  2. Run `npm start` to deploy
  3. Test with `./jtag analytics/report --startDate="2025-01-01"`
```

### Workflow 2: AI Enriches Command for Better Discoverability
```typescript
// PersonaUser realizes screenshot tool needs better docs
<tool_use>
  <tool_name>gen/enrich</tool_name>
  <parameters>
    <target>screenshot</target>
    <operation>addExamples</operation>
    <examples>[
      {
        "description": "Capture chat interface for debugging",
        "params": {"querySelector": "chat-widget"},
        "expectedResult": "PNG image of chat interface"
      }
    ]</examples>
  </parameters>
</tool_use>
```

### Workflow 3: Developer Adds Man Page
```bash
$ ./jtag gen/man --target="chat/send" --sections='[
  {
    "name": "DESCRIPTION",
    "content": "Post messages directly to chat database, bypassing UI..."
  },
  {
    "name": "USAGE NOTES",
    "content": "When used by AI personas, messages are attributed..."
  }
]'

✓ Generated man page for chat/send
✓ Updated metadata exports

$ ./jtag man chat/send
[Displays formatted man page]
```

### Workflow 4: Bulk Enrichment of Commands
```bash
# Tag all destructive commands as internal
$ ./jtag gen/enrich --pattern="**/nuke/**" --accessLevel="internal"
$ ./jtag gen/enrich --pattern="**/delete/**" --accessLevel="dangerous"

# Add basic descriptions to undocumented commands
$ ./jtag gen/enrich --auto-describe --dry-run
Found 23 commands with generic descriptions:
  - ai/adapter/test: "ai/adapter/test command"
  - ai/bag-of-words: "ai/bag-of-words command"
  ...

Apply AI-generated descriptions? [y/N] y
✓ Updated 23 command descriptions
```

---

## Benefits

### For Developers
- **Faster scaffolding:** Generate complete modules in seconds
- **Consistent structure:** All modules follow Universal Pattern
- **Better documentation:** Auto-generated man pages
- **Easy migrations:** Bulk updates across codebase

### For AI Personas
- **Discoverable tools:** Clear descriptions and examples
- **Safe operations:** accessLevel filtering prevents dangerous commands
- **Self-improvement:** Can enrich their own tool definitions
- **Better prompts:** Rich metadata enables better tool selection

### For the Community
- **Contribution templates:** Easy to add new commands/widgets
- **Self-documenting:** Man pages generated from code
- **Quality standards:** Templates enforce best practices
- **Extensibility:** Add custom templates for project needs

---

## Technical Considerations

### Template Engine Requirements
- Simple variable substitution (no complex logic)
- Support for conditional blocks
- File/directory structure generation
- Post-generation hooks (formatting, linting)

### Enrichment Strategy
- **Non-destructive:** Preserve existing code
- **Idempotent:** Re-running produces same result
- **Verifiable:** Show diff before applying
- **Reversible:** Track changes for rollback

### Migration Safety
- **AST-based:** Use TypeScript compiler API
- **Dry-run mode:** Preview changes first
- **Selective:** Target specific files/patterns
- **Tested:** Migration scripts have unit tests

---

## Implementation Decisions

### 1. Template Format: External Files with {{TOKEN}} Replacement

**Decision:** Pure text templates with `{{TOKEN}}` syntax (Mustache/Handlebars convention)

**Why:**
- Industry standard (`{{}}` used by Mustache, Handlebars, Vue, Angular)
- No conflict with TypeScript syntax
- Simple string replacement - no complex parsing needed
- Easy to read and edit
- Reusable across different module types

**Structure:**
```
generator/
├── templates/
│   ├── command/
│   │   ├── shared-types.ts      # Uses {{CLASS_NAME}}, {{DESCRIPTION}}, etc.
│   │   ├── browser-wrapper.ts
│   │   ├── server-impl.ts
│   │   └── readme.md
│   ├── widget/
│   │   └── ...
│   └── daemon/
│       └── ...
├── TokenReplacer.ts              # Simple regex-based replacement
├── TokenBuilder.ts               # Modular helpers to build token values
└── TemplateLoader.ts             # Load and render templates
```

**Example Template (generator/templates/command/shared-types.ts):**
```typescript
/**
 * {{DESCRIPTION}}
 */

import type { CommandParams, CommandResult } from '@system/core/shared/Commands';

export interface {{CLASS_NAME}}Params extends CommandParams {
{{PARAM_FIELDS}}
}

export interface {{CLASS_NAME}}Result extends CommandResult {
{{RESULT_FIELDS}}
}

export const {{METADATA_NAME}} = {
  accessLevel: '{{ACCESS_LEVEL}}' as const,
  description: '{{DESCRIPTION}}',
  examples: [{{EXAMPLES}}],
  usageNotes: [{{USAGE_NOTES}}]
};
```

**Token Replacement:**
```typescript
// generator/TokenReplacer.ts
export class TokenReplacer {
  static replace(template: string, tokens: Record<string, string>): string {
    let result = template;
    for (const [key, value] of Object.entries(tokens)) {
      result = result.replace(new RegExp(`{{${key}}}`, 'g'), value);
    }
    return result;
  }
}
```

**Token Building (Modular Logic):**
```typescript
// generator/TokenBuilder.ts
export class TokenBuilder {
  static toClassName(name: string): string {
    return name.split('/').map(p => p[0].toUpperCase() + p.slice(1)).join('');
  }

  static buildParamFields(params?: Record<string, any>): string {
    if (!params) return '  // Add parameters as needed';
    return Object.entries(params)
      .map(([n, i]) => `  /** ${i.description} */\n  ${n}${i.required === false ? '?' : ''}: ${i.type};`)
      .join('\n');
  }

  static buildCommandTokens(params: {...}): Record<string, string> {
    const className = this.toClassName(params.name);
    return {
      COMMAND_NAME: params.name,
      CLASS_NAME: className,
      METADATA_NAME: `${className.toUpperCase()}_METADATA`,
      DESCRIPTION: params.description,
      ACCESS_LEVEL: params.accessLevel || 'ai-safe',
      PARAM_FIELDS: this.buildParamFields(params.params),
      RESULT_FIELDS: '  // TODO: Add result fields',
      EXAMPLES: this.buildExamples(params.examples),
      USAGE_NOTES: this.buildUsageNotes(params.usageNotes)
    };
  }
}
```

### 2. Metadata Storage: Inline Exports

**Decision:** Metadata exported as constants in Types.ts files

**Why:**
- Single source of truth (metadata lives with type definitions)
- No separate files to maintain
- Easy to import and use in PersonaToolDefinitions
- Follows existing JTAG pattern (like EventConstants)

**Example:**
```typescript
// commands/screenshot/shared/ScreenshotTypes.ts
export const SCREENSHOT_METADATA = {
  accessLevel: 'ai-safe' as const,
  description: 'Capture browser viewport or DOM element as PNG image',
  examples: [{ description: 'Capture page', params: {}, expectedResult: '...' }],
  usageNotes: ['Returns base64 PNG optimized for AI vision']
};
```

### 3. Migration Strategy: AST-Based with TypeScript Compiler API

**Decision:** Use TypeScript Compiler API for safe code transformations

**Why:**
- Type-aware transformations (won't break code)
- Respects existing structure and formatting
- Can validate changes before applying
- Industry standard (used by ts-morph, Angular CLI, etc.)

**Example:**
```typescript
// generator/migrations/AddMetadataMigration.ts
import * as ts from 'typescript';

export class AddMetadataMigration extends BaseMigration {
  protected transformFile(sourceFile: ts.SourceFile): string {
    // Use TypeScript Compiler API to add METADATA export
    // Preserve existing code structure and formatting
    // Validate transformation maintains type safety
  }
}
```

### 4. AI Safety: Opt-In for AI Access

**Decision:** Default accessLevel to `'internal'`, require explicit `'ai-safe'` tagging

**Why:**
- Safe by default (commands hidden from AIs unless explicitly marked)
- Forces developers to think about AI implications
- Easy to audit (grep for `ai-safe` to see what AIs can access)
- Can be overridden with environment variable for testing

**Filter Implementation:**
```typescript
// PersonaToolDefinitions.ts
const safeCommands = allCommands.filter(cmd => {
  const metadata = getCommandMetadata(cmd.name);
  return metadata?.accessLevel === 'ai-safe';
});
```

### 5. Community Templates: Plugin System (Future)

**Decision:** Support custom templates via plugin directory (Phase 6+)

**Structure:**
```
generator/
├── templates/          # Built-in templates
└── plugins/            # User-provided templates
    └── my-org/
        ├── command/
        └── widget/
```

## Open Questions (Revisit in Phase 3)

1. **Man page format:** Plain text vs markdown vs hybrid?
2. **Template versioning:** How to handle template updates for existing modules?
3. **Dry-run visualization:** Show diffs before applying migrations?

---

## Next Steps

This document provides the architecture. Before implementation, we should:

1. **Review and refine** this architecture together
2. **Choose template engine** approach
3. **Design metadata format** (inline vs. separate files)
4. **Prioritize phases** based on immediate needs
5. **Create first template** for `gen/command`

Once we align on approach, I'll begin Phase 1 implementation.
