/**
 * HelpFormatter - Rich CLI help output for the command generator
 *
 * Provides detailed help with example specs, field descriptions, and practical
 * guidance. This is the documentation that AI agents and humans read to understand
 * how to write specs. It must be comprehensive enough that reading --help alone
 * is sufficient to produce a correct spec.
 */

import * as fs from 'fs';
import * as path from 'path';

export class HelpFormatter {

  /**
   * Full help output — printed when --help is passed with no arguments
   */
  static fullHelp(): string {
    return `
${this.header()}

${this.usage()}

${this.modes()}

${this.specReference()}

${this.exampleSpecs()}

${this.fieldTypeReference()}

${this.accessLevels()}

${this.generatedFiles()}

${this.workflow()}

${this.auditHelp()}
`.trimStart();
  }

  /**
   * Short help — printed when no arguments are passed (error case)
   */
  static shortHelp(): string {
    return `${this.header()}

${this.usage()}

Run with --help for full documentation, spec reference, and example specs.
`;
  }

  /**
   * Help for a specific topic: --help=spec, --help=types, --help=examples, --help=audit
   */
  static topicHelp(topic: string): string {
    switch (topic) {
      case 'spec':
        return `${this.header()}\n\n${this.specReference()}\n\n${this.fieldTypeReference()}`;
      case 'types':
        return `${this.header()}\n\n${this.fieldTypeReference()}`;
      case 'examples':
        return `${this.header()}\n\n${this.exampleSpecs()}`;
      case 'audit':
        return `${this.header()}\n\n${this.auditHelp()}`;
      case 'workflow':
        return `${this.header()}\n\n${this.workflow()}`;
      default:
        return `Unknown help topic: "${topic}"\n\nAvailable topics: spec, types, examples, audit, workflow\n\nRun --help for full documentation.`;
    }
  }

  // ── Building blocks ──────────────────────────────────────────────────

  private static header(): string {
    return `╔══════════════════════════════════════════════════════════════╗
║              CONTINUUM COMMAND GENERATOR                      ║
║  One spec → complete command (types, server, browser, tests)  ║
╚══════════════════════════════════════════════════════════════╝`;
  }

  private static usage(): string {
    return `USAGE:

  npx tsx generator/CommandGenerator.ts <spec-file.json> [flags]
  npx tsx generator/CommandGenerator.ts <mode>

MODES:

  <spec-file.json>            Generate command from spec file
  --template                  Output a starter spec JSON to stdout
  --template=<type>           Output typed starter: minimal, rust-ipc, browser-only
  --test                      Generate a sample command to /tmp (smoke test)
  --help                      Full documentation
  --help=<topic>              Topic help: spec, types, examples, audit, workflow
  --audit                     Scan all commands, report conformance
  --reverse <command-dir>     Reverse-engineer a spec from existing command

FLAGS (with spec file):

  --force                     Overwrite existing command directory
  --backup                    Create timestamped backup before overwrite (requires --force)
  --dry-run                   Show what would be generated without writing files`;
  }

  private static modes(): string {
    return `GENERATION MODES:

  The generator produces 8 files from a single JSON spec:

    commands/<name>/
    ├── shared/<Name>Types.ts          # Params, Result, factory functions, static accessor
    ├── browser/<Name>BrowserCommand.ts # Browser-side implementation
    ├── server/<Name>ServerCommand.ts   # Server-side implementation
    ├── README.md                       # Auto-generated documentation
    ├── test/unit/<Name>Command.test.ts # Unit test scaffold
    ├── test/integration/<Name>Integration.test.ts
    ├── package.json                    # Module metadata
    └── .npmignore

  The shared Types file is the most important — it contains:
  • Typed params interface (extends CommandParams)
  • Typed result interface (extends CommandResult)
  • Factory functions (createParams, createResult, createResultFromParams)
  • Static accessor: <Name>.execute({ ... }) — the type-safe call site
  • Command name constant: <Name>.commandName`;
  }

  private static specReference(): string {
    return `SPEC REFERENCE (CommandSpec):

  A spec is a JSON file with these fields:

  ┌─────────────────┬──────────┬────────────────────────────────────────────┐
  │ Field           │ Required │ Description                                │
  ├─────────────────┼──────────┼────────────────────────────────────────────┤
  │ name            │ YES      │ Command path: "gpu/stats", "ping",         │
  │                 │          │ "genome/adapter-list"                      │
  ├─────────────────┼──────────┼────────────────────────────────────────────┤
  │ description     │ YES      │ What the command does. Be specific —       │
  │                 │          │ this becomes the README header and         │
  │                 │          │ help text that AIs read.                   │
  ├─────────────────┼──────────┼────────────────────────────────────────────┤
  │ params          │ YES      │ Array of ParamSpec (can be empty [])       │
  │                 │          │ Each: { name, type, optional?, description }│
  ├─────────────────┼──────────┼────────────────────────────────────────────┤
  │ results         │ YES      │ Array of ResultSpec (can be empty [])      │
  │                 │          │ Each: { name, type, description }          │
  ├─────────────────┼──────────┼────────────────────────────────────────────┤
  │ examples        │ optional │ Array of ExampleSpec                       │
  │                 │          │ Each: { description, command,              │
  │                 │          │         expectedResult? }                  │
  │                 │          │ STRONGLY RECOMMENDED — AIs use these       │
  │                 │          │ to understand how to call your command.    │
  ├─────────────────┼──────────┼────────────────────────────────────────────┤
  │ accessLevel     │ optional │ "ai-safe" | "internal" | "system" |       │
  │                 │          │ "dangerous"  (default: "internal")         │
  ├─────────────────┼──────────┼────────────────────────────────────────────┤
  │ environment     │ optional │ "server" | "browser" | "both"             │
  │                 │          │ (default: "server")                        │
  └─────────────────┴──────────┴────────────────────────────────────────────┘

  ParamSpec fields:
    name          string    Parameter name (camelCase)
    type          string    TypeScript type — see TYPE REFERENCE below
    optional?     boolean   Whether the parameter can be omitted (default: false)
    description?  string    What this parameter does — appears in help + README

  ResultSpec fields:
    name          string    Result field name (camelCase)
    type          string    TypeScript type — see TYPE REFERENCE below
    description?  string    What this field contains — appears in help + README

  ExampleSpec fields:
    description   string    What this example demonstrates
    command       string    CLI invocation: "./jtag gpu/stats --subsystem=inference"
    expectedResult? string  Example output (helps AIs understand return shape)`;
  }

  private static exampleSpecs(): string {
    return `EXAMPLE SPECS:

  ── Minimal (no params, simple result) ──────────────────────

  {
    "name": "ping",
    "description": "Health check — returns server status and uptime",
    "params": [],
    "results": [
      { "name": "uptime", "type": "number", "description": "Server uptime in ms" },
      { "name": "version", "type": "string", "description": "Server version string" }
    ],
    "examples": [
      {
        "description": "Basic health check",
        "command": "./jtag ping",
        "expectedResult": "{ uptime: 142000, version: '1.2.3' }"
      }
    ],
    "accessLevel": "ai-safe"
  }

  ── Standard (params + results + examples) ──────────────────

  {
    "name": "data/query",
    "description": "Query entities from a collection with optional filters, sorting, and pagination",
    "params": [
      { "name": "collection", "type": "string", "description": "Entity collection name" },
      { "name": "filter", "type": "Record<string, unknown>", "optional": true, "description": "MongoDB-style filter object" },
      { "name": "orderBy", "type": "Array<{ field: string; direction: 'asc' | 'desc' }>", "optional": true, "description": "Sort order" },
      { "name": "limit", "type": "number", "optional": true, "description": "Max results (default: 100)" },
      { "name": "offset", "type": "number", "optional": true, "description": "Skip N results for pagination" }
    ],
    "results": [
      { "name": "items", "type": "Array<Record<string, unknown>>", "description": "Matching entities" },
      { "name": "totalCount", "type": "number", "description": "Total matching (before limit)" },
      { "name": "hasMore", "type": "boolean", "description": "Whether more results exist" }
    ],
    "examples": [
      {
        "description": "List all users sorted by activity",
        "command": "./jtag data/query --collection=users --orderBy='[{\"field\":\"lastActiveAt\",\"direction\":\"desc\"}]' --limit=10",
        "expectedResult": "{ items: [...], totalCount: 42, hasMore: true }"
      },
      {
        "description": "Find active personas",
        "command": "./jtag data/query --collection=users --filter='{\"type\":\"persona\",\"isActive\":true}'",
        "expectedResult": "{ items: [...], totalCount: 14, hasMore: false }"
      }
    ],
    "accessLevel": "ai-safe"
  }

  ── Rust IPC-backed (calls into Rust worker) ────────────────

  {
    "name": "gpu/stats",
    "description": "Query GPU memory manager stats including VRAM detection, per-subsystem budgets, usage tracking, and memory pressure",
    "params": [
      { "name": "subsystem", "type": "string", "optional": true, "description": "Filter: 'inference', 'tts', or 'rendering'. Omit for all." }
    ],
    "results": [
      { "name": "gpuName", "type": "string", "description": "GPU hardware name" },
      { "name": "totalVramMb", "type": "number", "description": "Total detected VRAM in MB" },
      { "name": "totalUsedMb", "type": "number", "description": "Total used across subsystems" },
      { "name": "pressure", "type": "number", "description": "Memory pressure 0.0-1.0" },
      { "name": "rendering", "type": "SubsystemInfo", "description": "Rendering budget/usage" },
      { "name": "inference", "type": "SubsystemInfo", "description": "Inference budget/usage" },
      { "name": "tts", "type": "SubsystemInfo", "description": "TTS budget/usage" }
    ],
    "examples": [
      { "description": "Full GPU stats", "command": "./jtag gpu/stats" },
      { "description": "Inference only", "command": "./jtag gpu/stats --subsystem=inference" }
    ],
    "accessLevel": "ai-safe"
  }

  NOTE: For Rust IPC commands, the generator creates the TypeScript command
  scaffold. You still need:
  1. The Rust ServiceModule (handles the IPC command)
  2. The IPC mixin (workers/continuum-core/bindings/modules/<name>.ts)
  3. Wire mixin into RustCoreIPC.ts composition chain
  See docs/infrastructure/UNIFIED-GENERATOR-ARCHITECTURE.md for full details.

  ── Browser-only (runs in browser context) ───────────────────

  {
    "name": "screenshot",
    "description": "Capture a screenshot of the current page or a specific DOM element",
    "params": [
      { "name": "querySelector", "type": "string", "optional": true, "description": "CSS selector to capture. Omit for full page." },
      { "name": "filename", "type": "string", "optional": true, "description": "Output filename (default: screenshot.png)" }
    ],
    "results": [
      { "name": "dataUrl", "type": "string", "description": "Base64 PNG data URL" },
      { "name": "width", "type": "number", "description": "Capture width in px" },
      { "name": "height", "type": "number", "description": "Capture height in px" }
    ],
    "examples": [
      { "description": "Full page", "command": "./jtag screenshot" },
      { "description": "Chat widget only", "command": "./jtag screenshot --querySelector=\\"chat-widget\\"" }
    ],
    "accessLevel": "ai-safe",
    "environment": "browser"
  }`;
  }

  private static fieldTypeReference(): string {
    return `TYPE REFERENCE:

  Param and result types are TypeScript types. Common patterns:

  ┌──────────────────────────────────────┬───────────────────────────────────┐
  │ Type                                 │ When to use                       │
  ├──────────────────────────────────────┼───────────────────────────────────┤
  │ string                               │ Text, IDs, names, paths           │
  │ number                               │ Counts, sizes, timestamps, floats │
  │ boolean                              │ Flags, toggles                    │
  │ string[]                             │ Lists of strings                  │
  │ number[]                             │ Lists of numbers                  │
  │ Record<string, unknown>              │ Generic key-value (filters, meta) │
  │ Array<{ field: string; dir: string }>│ Structured arrays                 │
  │ 'a' | 'b' | 'c'                     │ String literal unions (enums)     │
  │ CustomTypeName                       │ Custom interface (define nearby)  │
  ├──────────────────────────────────────┼───────────────────────────────────┤
  │ AVOID:                               │                                   │
  │ any                                  │ NEVER — always specify the type   │
  │ unknown                              │ Only for truly opaque data        │
  │ object                               │ Use Record<string, T> instead     │
  └──────────────────────────────────────┴───────────────────────────────────┘

  For custom types (like SubsystemInfo in gpu/stats), define them in the
  server implementation or a shared types file. The generator produces
  primitive TypeScript types only — complex types referenced in specs must
  exist at import time.`;
  }

  private static accessLevels(): string {
    return `ACCESS LEVELS:

  ┌────────────┬───────────────────────────────────────────────────────────┐
  │ Level      │ Meaning                                                   │
  ├────────────┼───────────────────────────────────────────────────────────┤
  │ ai-safe    │ Safe for AI personas to call autonomously. Read-only or   │
  │            │ low-risk operations. Most commands should be ai-safe.     │
  ├────────────┼───────────────────────────────────────────────────────────┤
  │ internal   │ Internal infrastructure. Not exposed to AI personas.      │
  │            │ Default if omitted.                                       │
  ├────────────┼───────────────────────────────────────────────────────────┤
  │ system     │ System-level. Requires elevated permissions. Server       │
  │            │ management, config changes.                               │
  ├────────────┼───────────────────────────────────────────────────────────┤
  │ dangerous  │ Potentially destructive. Requires explicit user confirm.  │
  │            │ Data deletion, system reset, etc.                         │
  └────────────┴───────────────────────────────────────────────────────────┘`;
  }

  private static generatedFiles(): string {
    return `GENERATED FILES:

  For a spec with name "gpu/stats", the generator creates:

  commands/gpu/stats/
  ├── shared/GpuStatsTypes.ts              ← Params, Result, factories, static accessor
  │   • GpuStatsParams extends CommandParams
  │   • GpuStatsResult extends CommandResult
  │   • createGpuStatsParams(ctx, sid, data)
  │   • createGpuStatsResult(ctx, sid, data)
  │   • createGpuStatsResultFromParams(params, diffs)
  │   • GpuStats.execute({ ... })          ← THE type-safe call site
  │   • GpuStats.commandName               ← 'gpu/stats' as const
  │
  ├── browser/GpuStatsBrowserCommand.ts    ← Browser impl (delegates to server)
  ├── server/GpuStatsServerCommand.ts      ← Server impl (TODO: fill in logic)
  ├── README.md                            ← Auto-generated docs with examples
  ├── test/unit/GpuStatsCommand.test.ts    ← Unit test scaffold
  ├── test/integration/GpuStatsIntegration.test.ts
  ├── package.json
  └── .npmignore

  The static accessor is the primary call site everywhere in the codebase:

    import { GpuStats } from '@commands/gpu/stats/shared/GpuStatsTypes';

    // Type-safe! Params and result fully typed.
    const result = await GpuStats.execute({ subsystem: 'inference' });
    console.log(result.totalVramMb);`;
  }

  private static workflow(): string {
    return `WORKFLOW:

  ── Creating a new command ──────────────────────────────────

  1. Write the spec:
     $ cat > generator/specs/my-command.json << 'EOF'
     {
       "name": "my/command",
       "description": "What it does — be specific, AIs read this",
       "params": [...],
       "results": [...],
       "examples": [...],
       "accessLevel": "ai-safe"
     }
     EOF

  2. Generate:
     $ npx tsx generator/CommandGenerator.ts generator/specs/my-command.json

  3. Implement the server logic:
     Edit commands/my/command/server/MyCommandServerCommand.ts
     Fill in the execute() method.

  4. Build and test:
     $ npm run build:ts
     $ npx vitest commands/my/command/test/
     $ npm start
     $ ./jtag my/command --param=value

  ── Regenerating an existing command ────────────────────────

  1. Edit the spec in generator/specs/
  2. Regenerate with --force:
     $ npx tsx generator/CommandGenerator.ts generator/specs/my-command.json --force
  3. Re-implement any custom logic (the server TODO sections get overwritten)

  TIP: Keep custom logic minimal in the generated server file. Extract
  business logic into service classes that the generated command calls.
  This way regeneration only overwrites the thin command shell.

  ── Reverse-engineering an existing command ──────────────────

  $ npx tsx generator/CommandGenerator.ts --reverse commands/my/command

  Reads the existing Types file, extracts params/results/command name,
  and outputs a spec JSON. Use this to bring hand-written commands
  under generator control.

  ── Auditing all commands ───────────────────────────────────

  $ npx tsx generator/CommandGenerator.ts --audit

  Scans all command directories, checks for:
  • Missing generator spec (hand-written without spec)
  • Missing static accessor (GpuStats.execute pattern)
  • Type violations (any casts, untyped params)
  • Stale specs (spec exists but generated code has drifted)`;
  }

  private static auditHelp(): string {
    return `AUDIT MODE:

  $ npx tsx generator/CommandGenerator.ts --audit

  The audit scans every directory under commands/ and reports:

  ┌──────────────────────────────┬──────────────────────────────────────┐
  │ Check                        │ What it flags                        │
  ├──────────────────────────────┼──────────────────────────────────────┤
  │ Missing spec                 │ Command dir exists but no matching   │
  │                              │ spec in generator/specs/             │
  ├──────────────────────────────┼──────────────────────────────────────┤
  │ Missing static accessor      │ Types file lacks the                 │
  │                              │ Name.execute() / Name.commandName    │
  │                              │ pattern                              │
  ├──────────────────────────────┼──────────────────────────────────────┤
  │ Missing factory functions    │ No createParams / createResult       │
  ├──────────────────────────────┼──────────────────────────────────────┤
  │ any casts in Types           │ Types file contains 'as any' or      │
  │                              │ ': any'                              │
  ├──────────────────────────────┼──────────────────────────────────────┤
  │ Orphaned spec                │ Spec exists but command dir missing  │
  └──────────────────────────────┴──────────────────────────────────────┘

  Output format:
    ✅ gpu/stats           — spec: yes, accessor: yes, factories: yes
    ⚠️  sentinel/run       — spec: NO, accessor: no, factories: no
    ⚠️  data/create        — spec: NO, accessor: yes, factories: yes
    ❌ agent/execute       — spec: NO, accessor: no, factories: no, any: 3

  Summary:
    Commands: 313 total
    With specs: 47 (15%)
    Missing accessors: 39
    any casts in Types: 23 across 11 commands`;
  }

  /**
   * Generate a starter spec for the --template flag.
   * type: 'minimal' | 'standard' | 'rust-ipc' | 'browser-only'
   */
  static templateSpec(type: string = 'standard'): object {
    switch (type) {
      case 'minimal':
        return {
          name: 'my/command',
          description: 'Brief description — be specific, AIs read this to understand usage',
          params: [],
          results: [
            { name: 'status', type: 'string', description: 'Result status message' }
          ],
          examples: [
            {
              description: 'Basic usage',
              command: './jtag my/command',
              expectedResult: '{ status: "ok" }'
            }
          ],
          accessLevel: 'ai-safe'
        };

      case 'rust-ipc':
        return {
          name: 'system/example',
          description: 'Example Rust IPC command — server delegates to Rust worker via Unix socket',
          params: [
            { name: 'subsystem', type: 'string', optional: true, description: 'Filter to subsystem name' }
          ],
          results: [
            { name: 'name', type: 'string', description: 'Subsystem name' },
            { name: 'healthy', type: 'boolean', description: 'Whether subsystem is healthy' },
            { name: 'uptimeMs', type: 'number', description: 'Uptime in milliseconds' },
            { name: 'pressure', type: 'number', description: 'Memory pressure 0.0-1.0' }
          ],
          examples: [
            {
              description: 'Full system health',
              command: './jtag system/example',
              expectedResult: '{ name: "inference", healthy: true, uptimeMs: 142000, pressure: 0.12 }'
            }
          ],
          accessLevel: 'ai-safe'
        };

      case 'browser-only':
        return {
          name: 'interface/example',
          description: 'Example browser-only command — runs in browser context, accesses DOM',
          params: [
            { name: 'querySelector', type: 'string', optional: true, description: 'CSS selector to target' }
          ],
          results: [
            { name: 'found', type: 'boolean', description: 'Whether element was found' },
            { name: 'innerHTML', type: 'string', description: 'Element inner HTML content' }
          ],
          examples: [
            {
              description: 'Query chat widget',
              command: './jtag interface/example --querySelector="chat-widget"',
              expectedResult: '{ found: true, innerHTML: "<div>...</div>" }'
            }
          ],
          accessLevel: 'ai-safe',
          environment: 'browser'
        };

      default: // 'standard'
        return {
          name: 'example/command',
          description: 'Describe what this command does — be specific and detailed. This text appears in the README and help output that AIs use to understand your command.',
          params: [
            { name: 'requiredParam', type: 'string', description: 'A required parameter — describe what value to pass' },
            { name: 'optionalFilter', type: 'string', optional: true, description: 'Optional filter. Omit for default behavior.' },
            { name: 'limit', type: 'number', optional: true, description: 'Max results to return (default: 100)' },
            { name: 'verbose', type: 'boolean', optional: true, description: 'Include extended diagnostics in result' }
          ],
          results: [
            { name: 'items', type: 'string[]', description: 'Array of matching item names' },
            { name: 'totalCount', type: 'number', description: 'Total matching items (before limit)' },
            { name: 'executionMs', type: 'number', description: 'How long the operation took in ms' }
          ],
          examples: [
            {
              description: 'Basic usage with required param',
              command: './jtag example/command --requiredParam="my-value"',
              expectedResult: '{ items: ["a", "b"], totalCount: 2, executionMs: 45 }'
            },
            {
              description: 'Filtered with limit',
              command: './jtag example/command --requiredParam="search" --optionalFilter="active" --limit=5',
              expectedResult: '{ items: ["x"], totalCount: 1, executionMs: 12 }'
            },
            {
              description: 'Verbose diagnostics',
              command: './jtag example/command --requiredParam="debug" --verbose=true'
            }
          ],
          accessLevel: 'ai-safe'
        };
    }
  }
}
