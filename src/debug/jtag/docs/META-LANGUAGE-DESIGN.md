# Meta-Language Design: Declarative System Architecture

## Origin Story

This design is informed by real-world experience building enterprise-scale meta-language systems:

**H&R Block Tax System (2010-2011)**
- XML-based meta-language for tax logic
- Generated iOS and Android code from single spec
- Handled OCR integration, multi-state forms
- Enabled inexperienced developers to create complex tax forms
- One architect + meta-language = Enterprise-scale system

**Key Insight:** A well-designed meta-language democratizes expertise by encoding best practices into constraints.

## The Problem We're Solving

### Without Meta-Language
- Every developer must understand full system architecture
- Copy-paste leads to inconsistency
- Architectural violations are easy
- Junior developers (or lower-quality AI personas) produce fragile code
- Technical debt accumulates exponentially

### With Meta-Language
- Developers describe WHAT they want, not HOW to build it
- Generator enforces architectural constraints
- Best practices are encoded in templates
- Junior developers/personas produce consistent, correct code
- System polices itself through validation

## Architecture Overview

```
Spec (JSON) → Validator → Generator → Code + Tests + Docs
                ↓             ↓           ↓
            Schema      Templates    Audit System
                                         ↓
                                   Self-Policing
```

### The Self-Policing Loop

1. **Schema defines rules**: What makes a valid module
2. **Validator enforces rules**: Rejects invalid specs before generation
3. **Generator applies patterns**: Produces code from validated specs
4. **Auditor verifies correctness**: Checks generated code meets standards
5. **Feedback improves system**: Audit failures improve schemas/templates

## Core Principles

### 1. Declarative Over Imperative
```json
// User writes THIS (declarative):
{
  "name": "hello",
  "params": [{ "name": "greeting", "type": "string", "required": true }]
}

// Generator produces THIS (imperative):
export interface HelloParams extends CommandParams {
  greeting: string;
}
// + validation logic
// + error handling
// + tests
// + documentation
```

### 2. Constraints as Features
- **Type safety enforced**: Schema defines allowed types
- **Required patterns**: Can't skip validation, error handling, tests
- **Naming conventions**: Generator applies consistent naming
- **File structure**: Enforced by generator, verified by audit

### 3. Progressive Enhancement
```
Level 1: Basic spec → Working module
Level 2: Add validation rules → Custom validation
Level 3: Add hooks → Lifecycle customization
Level 4: Add extensions → Domain-specific features
```

### 4. Fail Fast, Fail Clear
- Validate specs BEFORE generation
- Clear error messages pointing to fix
- Examples of correct usage
- No silent failures

## The Persona Democratization Effect

**Traditional Development:**
```
Senior Dev → Writes perfect code
Junior Dev → Copies/pastes, introduces bugs
Code Review → Catches some bugs
Production → Rest of bugs discovered
```

**Meta-Language Development:**
```
Senior Dev → Writes schemas + templates (once)
Junior Dev → Writes specs (validated)
Generator → Produces perfect code (always)
Audit → Catches any edge cases
Production → Code works (by construction)
```

**For AI Personas:**
```
Claude Opus (Smart) → Creates schemas, templates, patterns
Claude Sonnet (Mid)  → Uses specs to create modules, learns patterns
Claude Haiku (Fast)  → Fills in business logic within constraints
Local Ollama (Basic) → Can safely contribute with full guardrails
```

## Module Type Spec Format

### Current Implementation (Commands)
```json
{
  "specVersion": "1.0",
  "name": "hello",
  "description": "Greets the user",
  "params": [
    {
      "name": "name",
      "type": "string",
      "required": true,
      "description": "Name to greet",
      "validation": {
        "minLength": 1,
        "maxLength": 100,
        "pattern": "^[a-zA-Z\\s]+$"
      }
    }
  ],
  "results": [
    {
      "name": "message",
      "type": "string",
      "description": "Greeting message"
    },
    {
      "name": "timestamp",
      "type": "number",
      "description": "When greeting was generated"
    }
  ],
  "examples": [
    {
      "description": "Basic greeting",
      "command": "./jtag hello --name=\"World\"",
      "expectedOutput": { "message": "Hello, World!" }
    }
  ],
  "accessLevel": "ai-safe",
  "rateLimit": {
    "maxRequests": 100,
    "windowMs": 60000
  }
}
```

### Planned Extensions (Daemons, Widgets)
```json
{
  "moduleType": "daemon",
  "name": "module-audit-daemon",
  "description": "Long-running audit operations",
  "lifecycle": {
    "onStart": "initializeChecks",
    "onStop": "cleanupResources"
  },
  "jobs": [
    {
      "name": "auditModule",
      "params": ["modulePath", "moduleType", "fix"],
      "returns": "AuditReport",
      "async": true,
      "progressEvents": true
    }
  ],
  "events": [
    {
      "name": "audit:progress",
      "payload": { "completed": "number", "total": "number" }
    },
    {
      "name": "audit:complete",
      "payload": "AuditReport"
    }
  ]
}
```

## Token System

Templates use `{{TOKENS}}` replaced by generator:

### Universal Tokens (All Module Types)
- `{{MODULE_NAME}}` - snake-case module name
- `{{MODULE_NAME_PASCAL}}` - PascalCase module name
- `{{MODULE_NAME_CAMEL}}` - camelCase module name
- `{{MODULE_DESCRIPTION}}` - User-provided description
- `{{PARAMS_INTERFACE}}` - Generated TypeScript interface
- `{{RESULT_INTERFACE}}` - Generated TypeScript interface
- `{{VALIDATION_CODE}}` - Generated validation logic
- `{{ERROR_HANDLING}}` - Generated try/catch blocks
- `{{EXAMPLES}}` - Generated usage examples

### Module-Specific Tokens
- `{{COMMAND_EXECUTE}}` - Command execution logic
- `{{DAEMON_LIFECYCLE}}` - Daemon start/stop logic
- `{{WIDGET_RENDER}}` - Widget render method

## Schema Validation

### JSON Schema for Module Specs
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["name", "description"],
  "properties": {
    "specVersion": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+$",
      "description": "Version of spec format"
    },
    "name": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9-]*$",
      "description": "Module name (kebab-case)"
    },
    "description": {
      "type": "string",
      "minLength": 10,
      "maxLength": 500,
      "description": "Clear description of module purpose"
    },
    "params": {
      "type": "array",
      "items": { "$ref": "#/definitions/parameter" }
    }
  },
  "definitions": {
    "parameter": {
      "type": "object",
      "required": ["name", "type"],
      "properties": {
        "name": { "type": "string" },
        "type": {
          "enum": ["string", "number", "boolean", "UUID", "string[]", "object"]
        },
        "required": { "type": "boolean", "default": false },
        "description": { "type": "string" }
      }
    }
  }
}
```

## Lessons from Production Systems

### 1. Versioning Strategy
**Problem:** Specs evolve, old modules need migration

**Solution:**
```json
{
  "specVersion": "1.0",
  "migrations": {
    "1.0": "No changes needed",
    "2.0": "Add accessLevel field"
  }
}
```

Generator checks spec version and applies migrations automatically.

### 2. Validation Timing
**Problem:** Invalid specs crash generator mid-process

**Solution:**
- Validate BEFORE any file creation
- Validate AFTER generation (audit)
- Validate continuously (CI/CD)

```typescript
// 3-stage validation
1. Schema validation (structure)
2. Semantic validation (logic)
3. Generated code validation (compilation + audit)
```

### 3. Edge Case Handling
**Problem:** Not everything fits the meta-language

**Solution: Escape Hatches**
```json
{
  "name": "complex-command",
  "customCode": {
    "beforeExecute": "./hooks/custom-validation.ts",
    "afterExecute": "./hooks/custom-logging.ts"
  }
}
```

But make escape hatches:
- Explicit (not hidden)
- Audited (special checks)
- Rare (if common, improve meta-language)

### 4. Training Materials
**Problem:** Developers don't understand meta-language

**Solution:**
- Auto-generated documentation from schemas
- Interactive examples (try in browser)
- Error messages include examples of correct usage
- IDE integration (autocomplete, validation)

```bash
# Good error message:
❌ Invalid type 'str' for parameter 'name'
✅ Use one of: string, number, boolean, UUID, string[]

Example:
{
  "params": [
    { "name": "greeting", "type": "string", "required": true }
  ]
}
```

### 5. Modern Improvements (LLMs)
**If rebuilding H&R Block system today:**

1. **LLM-Assisted Spec Writing**
   ```
   User: "I need a command that greets users"
   LLM: Generates valid spec from natural language
   ```

2. **Natural Language Examples**
   ```json
   {
     "examples": [
       {
         "natural": "Greet a user named Alice",
         "generates": "./jtag hello --name=\"Alice\""
       }
     ]
   }
   ```

3. **Self-Improving Templates**
   - LLM analyzes generated code
   - Suggests template improvements
   - System learns from usage patterns

4. **Semantic Validation**
   - LLM checks if spec makes logical sense
   - "This command takes a UUID but returns a greeting?"
   - Catches logic errors humans miss

## Implementation Phases

### Phase 1: Extract Universal Patterns ✅
- [x] ModuleAuditor works with any module type
- [x] Template system with token replacement
- [x] Recursive module discovery
- [ ] Extract common generation logic to `generator/core/`

### Phase 2: Create Schemas (Daemon Work - NEXT)
- [ ] Write `command.schema.json` from existing patterns
- [ ] Write `daemon.schema.json` for daemon structure
- [ ] Implement schema validator
- [ ] Use schemas to validate specs before generation

### Phase 3: Meta-Generator (Future)
- [ ] Write `meta.schema.json` for defining module types
- [ ] Schema-driven template engine
- [ ] Self-hosting: generate new module types from specs

### Phase 4: Persona Integration (Future)
- [ ] Different personas can contribute at different levels
- [ ] Lower-quality personas work within guardrails
- [ ] System learns from all contributions
- [ ] Continuous improvement loop

## Success Metrics

The meta-language succeeds when:

1. **Correctness**: Generated code passes all audits automatically
2. **Consistency**: All modules follow same patterns
3. **Velocity**: New modules created in minutes, not hours
4. **Accessibility**: Junior developers/personas contribute safely
5. **Maintainability**: Changing templates updates all modules
6. **Self-Documentation**: Specs ARE the documentation

## See Also

- [ARCHITECTURE-RULES.md](../ARCHITECTURE-RULES.md) - System-wide patterns
- [AUDIT-SYSTEM-DESIGN.md](../../generator/AUDIT-SYSTEM-DESIGN.md) - Self-policing implementation
- [UNIVERSAL-PRIMITIVES.md](../UNIVERSAL-PRIMITIVES.md) - Command and Event patterns
- [SPEC-LANGUAGE.md](./SPEC-LANGUAGE.md) - Full spec language reference (TODO)
