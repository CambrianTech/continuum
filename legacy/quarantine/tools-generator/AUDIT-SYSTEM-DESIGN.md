# Generator Audit System Design

## Vision

The generator produces modules (commands, widgets, daemons) from templates. An audit system validates these modules and automatically fixes common issues.

## Three-Tier Architecture

```
1. Template Sets (generator/templates/)
   └── command/    - Command module templates
   └── widget/     - Widget module templates (future)
   └── daemon/     - Daemon module templates (future)

2. Generator (CommandGenerator.ts → ModuleGenerator.ts)
   └── Type-agnostic generator that works with any template set
   └── Takes: spec.json + template-type flag
   └── Produces: Module with all files from template

3. Audit System (NEW)
   └── Scans existing modules
   └── Detects issues (lint errors, missing files, outdated patterns)
   └── Fixes issues automatically with --fix flag
```

## Audit Command Design

```bash
# Audit specific command
./jtag generate/audit --module="commands/hello"

# Audit all commands
./jtag generate/audit --type="command"

# Audit and fix issues
./jtag generate/audit --type="command" --fix

# Audit and hibernate unfixable modules
./jtag generate/audit --type="command" --fix --hibernate-failures

# Audit widgets (future)
./jtag generate/audit --type="widget" --fix

# Audit hibernated modules (check if restorable)
./jtag generate/audit --hibernated
```

## Audit Checks

### 1. Linting Errors
**Check**: Run eslint on module files
**Fix**: Apply eslint --fix automatically

### 2. Missing Files
**Check**: Compare against template manifest
**Fix**: Generate missing files from templates

### 3. Outdated Patterns
**Check**: Detect deprecated patterns (empty interfaces, `any` types, etc.)
**Fix**: Apply current best practices from templates

### 4. Package.json Validation
**Check**: Verify package.json matches spec
**Fix**: Regenerate package.json from template

### 5. README Completeness
**Check**: Verify README has all required sections
**Fix**: Regenerate README from spec

### 6. Test Coverage
**Check**: Verify unit and integration tests exist
**Fix**: Generate missing test files

### 7. Hibernation Pollution
**Check**: Detect backup/hibernation directories in active module locations
**Fix**: Move to proper hibernation location (`/tmp/jtag-hibernation/`)

### 8. Hibernated Module Health
**Check**: Verify hibernated modules are restorable (package integrity)
**Fix**: Re-package corrupted hibernations, report unrestorable modules

## Implementation Phases

### Phase 1: Audit Infrastructure (NEXT)
```typescript
// generator/audit/ModuleAuditor.ts
class ModuleAuditor {
  async audit(modulePath: string): Promise<AuditReport> {
    // Run all checks, return issues
  }

  async fix(modulePath: string, issues: Issue[]): Promise<void> {
    // Apply automatic fixes
  }
}

// generator/audit/checks/
//   - LintCheck.ts
//   - MissingFilesCheck.ts
//   - OutdatedPatternsCheck.ts
//   - PackageJsonCheck.ts
//   - ReadmeCheck.ts
//   - TestCoverageCheck.ts
```

### Phase 2: Command Module Auditor
- Implement all checks for command modules
- Test with hello command
- Validate fixes don't break functionality

### Phase 3: Widget/Daemon Templates
- Create widget template set
- Create daemon template set
- Extend auditor to handle multiple module types

### Phase 4: Integration
- Add `./jtag generate/audit` command
- Integrate into precommit hook (optional warning)
- Add to CI/CD pipeline

## Audit Report Format

```typescript
interface AuditReport {
  modulePath: string;
  moduleType: 'command' | 'widget' | 'daemon';
  timestamp: number;
  issues: Issue[];
  summary: {
    errors: number;
    warnings: number;
    fixable: number;
  };
}

interface Issue {
  severity: 'error' | 'warning';
  category: 'lint' | 'missing-file' | 'outdated-pattern' | 'package-json' | 'readme' | 'test-coverage';
  message: string;
  filePath?: string;
  lineNumber?: number;
  fixable: boolean;
  suggestedFix?: string;
}
```

## Example Output

```bash
$ ./jtag generate/audit --module="commands/hello"

🔍 Auditing module: commands/hello (type: command)

✅ Linting: 0 errors
✅ Files: All required files present
✅ Patterns: Using current best practices
✅ Package.json: Valid and up-to-date
⚠️  README: Missing "Performance Considerations" section (fixable)
✅ Tests: Unit and integration tests present
✅ Hibernation: No backup pollution detected

📊 Summary:
   0 errors, 1 warning, 1 fixable

Run with --fix to automatically fix issues:
   ./jtag generate/audit --module="commands/hello" --fix
```

### Example: Hibernation Pollution Detected

```bash
$ ./jtag generate/audit --type="command"

🔍 Auditing all command modules...

✅ chat/send: All checks passed
✅ screenshot: All checks passed
❌ hello.backup.1765067828264: Hibernation pollution detected
   Location: commands/hello.backup.1765067828264/
   Issue: Backup directory in active module location
   Impact: Causes duplicate identifiers in structure generation
   Fix: Move to /tmp/jtag-hibernation/

📊 Summary:
   1 error, 0 warnings, 1 fixable

Run with --fix to automatically fix issues:
   ./jtag generate/audit --type="command" --fix

Run with --hibernate-failures to hibernate unfixable modules:
   ./jtag generate/audit --type="command" --fix --hibernate-failures
```

## Template Sets Structure

```
generator/templates/
├── command/
│   ├── manifest.json              # Required files list
│   ├── shared-types.template.ts
│   ├── browser.template.ts
│   ├── server.template.ts
│   ├── README.template.md
│   ├── package.json.template
│   ├── .npmignore.template
│   ├── unit-test.template.ts
│   └── integration-test.template.ts
│
├── widget/
│   ├── manifest.json
│   ├── shared-types.template.ts
│   ├── widget.template.ts          # Web component
│   ├── styles.template.css
│   ├── README.template.md
│   ├── package.json.template
│   └── widget-test.template.ts
│
└── daemon/
    ├── manifest.json
    ├── shared-types.template.ts
    ├── server.template.ts           # Daemon logic
    ├── client.template.ts           # Daemon client
    ├── README.template.md
    ├── package.json.template
    └── daemon-test.template.ts
```

## Benefits

1. **Consistency**: All modules follow current best practices
2. **Maintainability**: Easy to update all modules when patterns change
3. **Quality**: Catch issues before they reach production
4. **Onboarding**: New developers see correct patterns
5. **Confidence**: Know generated code is lint-free and complete
6. **Extensibility**: Easy to add new module types (widgets, daemons)

## Migration Strategy

1. **Start with commands**: Audit and fix existing commands first
2. **Add widgets**: Create widget templates, migrate existing widgets
3. **Add daemons**: Create daemon templates, migrate existing daemons
4. **CI/CD integration**: Run audit in precommit hook
5. **Documentation**: Update CLAUDE.md with audit workflow

## Next Steps

1. ✅ Fix empty params linting (DONE)
2. Create ModuleAuditor base class
3. Implement LintCheck (first check)
4. Test with hello command
5. Add remaining checks incrementally
6. Create generate/audit command
7. Document usage in CLAUDE.md
