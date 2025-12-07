# Meta-Language Generator Roadmap

## Vision: AI-Learnable System Architecture

**Core Principle**: For AI agents to learn and contribute to Continuum, the system must be **self-documenting, consistent, and validated**. The meta-language generator makes this possible.

## Why Generators Are Essential for Alpha

### The Problem Without Generators

Traditional development at scale creates inconsistency:
- 121+ commands with different patterns
- Copy-paste leads to divergence
- AI agents can't reliably learn "how to add a feature"
- Lower-intelligence AIs produce buggy code
- Only expert-level AIs can contribute safely

### The Solution: Meta-Language + Generation

**One pattern to learn**: Read spec → Generate perfect code

```bash
# AI agent writes a simple spec
cat > my-feature.spec.json <<EOF
{
  "name": "my-feature",
  "description": "Does something useful",
  "params": [...]
}
EOF

# Generator produces perfect, tested, documented code
./jtag generate commands/my-feature.spec.json
```

**Benefits**:
- ✅ Consistency guaranteed (templates enforce patterns)
- ✅ Self-documenting (spec IS documentation)
- ✅ Validated correctness (audit catches errors)
- ✅ Discoverable (file scanning finds everything)
- ✅ AI-learnable (simple spec, complex output)

### Critical Dependencies for Specialized Personas

**CSS Guru Persona** needs:
```
Widget Generator → Generate UI components
  ↓
Screenshot Command → Visual feedback
  ↓
CSS Guru can see and iterate on designs
  ↓
LoRA training on visual tasks
```

**Without widget generator**: CSS guru is blind, can't validate their work, can't learn.

## Phase 1: Command Generator ✅ COMPLETE

**Status**: Shipped in PR #223

**What We Built**:
- Template-driven generation (specs → code + tests + docs)
- Token replacement system (`{{MODULE_NAME}}`, `{{PARAMS_INTERFACE}}`, etc.)
- Recursive module discovery
- Self-policing audit system (7 check types)
- Auto-fix for missing files, outdated patterns, linting errors
- Dogfooding: Generator commands pass their own audits

**Proof of Concept**:
```bash
# Generate command from spec
./jtag generate commands/hello.spec.json

# Audit finds issues
./jtag generate/audit --type="command"

# Auto-fix resolves them
./jtag generate/audit --type="command" --fix

# Dogfooding verification
./jtag generate/audit --module="commands/generate/audit"
# ✅ Zero errors, zero warnings
```

**Files**:
- `generator/CommandGenerator.ts` - Core generation logic
- `generator/TemplateLoader.ts` - Token replacement
- `generator/core/FileScanner.ts` - Recursive discovery
- `generator/templates/command/*.ts` - Command templates
- `generator/auditors/*` - 7 audit checks

**Documentation**:
- [AUDIT-SYSTEM-DESIGN.md](../generator/AUDIT-SYSTEM-DESIGN.md)
- [generate/audit/README.md](../commands/generate/audit/README.md)

## Phase 2: Daemon Generator 📋 NEXT

**Timeline**: Next PR (2-4 weeks)

**Why This Matters**:
Daemons are **long-running background services** that will power:
- LoRA adapter manager (load/unload adapters)
- Training pipeline (fine-tuning workflow)
- RAG context builder (codebase indexing)
- Cost optimizer (model selection)
- Health monitor (system status)

**Without daemon generator**: These would be hand-written, inconsistent, and hard for AIs to learn.

### Daemon Structure Pattern

```typescript
// daemons/lora-manager/shared/LoRAManagerTypes.ts
export interface LoRAManagerDaemonParams {
  maxAdapters: number;      // LRU cache size
  evictionPolicy: 'lru' | 'priority';
}

export interface LoRAManagerDaemonJobs {
  loadAdapter(path: string): Promise<AdapterHandle>;
  unloadAdapter(adapterId: UUID): Promise<void>;
  evictLRU(): Promise<UUID>;
  listAdapters(): Promise<AdapterInfo[]>;
}

export interface LoRAManagerDaemonEvents {
  'adapter:loaded': { adapterId: UUID; domain: string };
  'adapter:unloaded': { adapterId: UUID; reason: string };
  'adapter:evicted': { adapterId: UUID };
}

// daemons/lora-manager/server/LoRAManagerDaemon.ts
export class LoRAManagerDaemon extends BaseDaemon<
  LoRAManagerDaemonParams,
  LoRAManagerDaemonJobs,
  LoRAManagerDaemonEvents
> {
  async onStart(): Promise<void> { /* initialization */ }
  async onStop(): Promise<void> { /* cleanup */ }

  async loadAdapter(path: string): Promise<AdapterHandle> { /* ... */ }
  // ... other jobs
}
```

### Template Tokens for Daemons

```typescript
// generator/templates/daemon/server.template.ts
export class {{DAEMON_NAME_PASCAL}}Daemon extends BaseDaemon<
  {{DAEMON_NAME_PASCAL}}Params,
  {{DAEMON_NAME_PASCAL}}Jobs,
  {{DAEMON_NAME_PASCAL}}Events
> {
  {{LIFECYCLE_HOOKS}}
  {{JOB_IMPLEMENTATIONS}}
  {{EVENT_EMITTERS}}
}
```

### Daemon Spec Example

```json
{
  "name": "lora-manager",
  "description": "Manages LoRA adapter loading/unloading with LRU eviction",
  "params": {
    "maxAdapters": { "type": "number", "default": 5 },
    "evictionPolicy": { "type": "enum", "values": ["lru", "priority"] }
  },
  "jobs": [
    {
      "name": "loadAdapter",
      "params": [{ "name": "path", "type": "string" }],
      "returns": "AdapterHandle",
      "async": true
    },
    {
      "name": "unloadAdapter",
      "params": [{ "name": "adapterId", "type": "UUID" }],
      "async": true
    }
  ],
  "events": [
    {
      "name": "adapter:loaded",
      "payload": { "adapterId": "UUID", "domain": "string" }
    }
  ],
  "lifecycle": {
    "onStart": "Initialize adapter cache",
    "onStop": "Unload all adapters gracefully"
  }
}
```

### Implementation Tasks

1. **Study Daemon Patterns** (~2 hours)
   - Analyze existing daemons (data-daemon, ai-provider-daemon)
   - Identify common lifecycle patterns
   - Extract reusable abstractions

2. **Create Templates** (~4 hours)
   - `daemon/shared-types.template.ts` - Interfaces
   - `daemon/server.template.ts` - Daemon implementation
   - `daemon/README.template.md` - Documentation

3. **Extend Generator** (~2 hours)
   - Add daemon support to CommandGenerator
   - Handle lifecycle hooks, jobs, events
   - Test with example daemon

4. **Update Audit System** (~2 hours)
   - Daemon-specific checks (lifecycle, jobs, events)
   - Validate daemon structure
   - Auto-fix capabilities

5. **Dogfooding Test** (~2 hours)
   - Generate LoRA manager daemon from spec
   - Generate training pipeline daemon from spec
   - Verify generated code works end-to-end

**Total Estimate**: 12-16 hours

## Phase 3: Widget Generator 📋 AFTER DAEMONS

**Timeline**: Same PR as daemons (or separate if complex)

**Why This Matters**:
Widgets are **UI components** that enable:
- CSS guru personas (need visual feedback)
- Graphic designer personas (need to see their work)
- Dashboard widgets (system monitoring)
- Control panels (LoRA adapter management UI)
- Design system (consistent UI patterns)

**Critical Dependency**: CSS/design personas CANNOT function without reliable widget generation + screenshots.

### Widget Structure Pattern

```typescript
// widgets/lora-dashboard/shared/LoRADashboardTypes.ts
export interface LoRADashboardWidgetProps {
  refreshInterval: number;
}

export interface LoRADashboardWidgetState {
  adapters: AdapterInfo[];
  memoryUsage: number;
}

// widgets/lora-dashboard/browser/LoRADashboardWidget.ts
export class LoRADashboardWidget extends BaseWidget<
  LoRADashboardWidgetProps,
  LoRADashboardWidgetState
> {
  async onMount(): Promise<void> { /* subscribe to events */ }
  async onUnmount(): Promise<void> { /* cleanup */ }

  render(): string {
    return `
      <div class="lora-dashboard">
        {{WIDGET_CONTENT}}
      </div>
    `;
  }

  getStyles(): string {
    return `
      .lora-dashboard {
        {{WIDGET_STYLES}}
      }
    `;
  }
}
```

### Widget Spec Example

```json
{
  "name": "lora-dashboard",
  "description": "Displays loaded LoRA adapters and memory usage",
  "props": {
    "refreshInterval": { "type": "number", "default": 5000 }
  },
  "state": {
    "adapters": { "type": "AdapterInfo[]" },
    "memoryUsage": { "type": "number" }
  },
  "events": {
    "subscribes": ["adapter:loaded", "adapter:unloaded"],
    "emits": []
  },
  "template": "dashboard",
  "styles": {
    "layout": "grid",
    "theme": "dark"
  }
}
```

### Implementation Tasks

1. **Study Widget Patterns** (~3 hours)
   - Analyze existing widgets (chat-widget, user-list)
   - Identify common patterns (lifecycle, props, state, render)
   - Extract reusable abstractions

2. **Create Templates** (~6 hours)
   - `widget/shared-types.template.ts` - Interfaces
   - `widget/browser.template.ts` - Widget implementation
   - `widget/styles.template.css` - CSS template
   - `widget/html.template.html` - HTML template
   - `widget/README.template.md` - Documentation

3. **Extend Generator** (~3 hours)
   - Add widget support to CommandGenerator
   - Handle props, state, lifecycle, render
   - CSS/HTML template integration

4. **Update Audit System** (~2 hours)
   - Widget-specific checks (styles, render, lifecycle)
   - Validate widget structure
   - Auto-fix capabilities

5. **Dogfooding Test** (~3 hours)
   - Generate LoRA dashboard widget from spec
   - Generate training monitor widget from spec
   - Verify CSS guru persona can use screenshot + iterate

**Total Estimate**: 17-20 hours

## Phase 4: Schema Validation 📋 FUTURE

**Timeline**: After daemon/widget generators

**What**: JSON Schema validation for specs before generation

```typescript
// Validate spec against schema BEFORE generation
const schema = loadSchema('command.schema.json');
const valid = validateSpec(spec, schema);

if (!valid) {
  console.error('Invalid spec:', validator.errors);
  process.exit(1);
}
```

**Benefits**:
- Catch spec errors early (before generation)
- Better error messages (point to exact issue)
- IDE integration (autocomplete in spec files)
- Self-documenting (schema IS the spec language)

## Success Metrics

The meta-language generator succeeds when:

1. **Correctness**: Generated code passes all audits automatically
2. **Consistency**: All modules follow same patterns
3. **Velocity**: New modules created in minutes, not hours
4. **Accessibility**: Junior developers/personas contribute safely
5. **Maintainability**: Changing templates updates all modules
6. **Self-Documentation**: Specs ARE the documentation
7. **AI-Learnability**: Lower-intelligence AIs can contribute

## Force Multiplier Effect

**Traditional Development**:
```
1 developer = 1x output (linear)
10 developers = 10x output
```

**With Meta-Language**:
```
1 developer + meta-system = 100x output (exponential)
1 developer + meta-system + AI personas = 1000x+ output
```

**How**:
- Meta-language encodes expertise (junior devs produce senior-level code)
- Self-policing eliminates manual reviews
- Generated code always up-to-date with best practices
- System improvements propagate automatically to all modules
- AI agents can safely contribute at ANY intelligence level

## Related Documentation

- [FORCE-MULTIPLIER-PRINCIPLE.md](architecture/FORCE-MULTIPLIER-PRINCIPLE.md) - Philosophy
- [META-LANGUAGE-DESIGN.md](META-LANGUAGE-DESIGN.md) - Architecture
- [AUDIT-SYSTEM-DESIGN.md](../generator/AUDIT-SYSTEM-DESIGN.md) - Self-policing
- [generate/audit/README.md](../commands/generate/audit/README.md) - Audit usage

---

**Last Updated**: December 2025
**Current Phase**: Phase 1 complete, Phase 2 starting
**Next Milestone**: Daemon generator enables LoRA infrastructure
