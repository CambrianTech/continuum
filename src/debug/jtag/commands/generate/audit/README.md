# generate/audit Command

Audit generated modules for issues and optionally fix them automatically.

## Usage

```bash
# Audit specific module
./jtag generate/audit --module="commands/hello"

# Audit all commands
./jtag generate/audit --type="command"

# Audit and fix issues
./jtag generate/audit --type="command" --fix

# Audit widgets (future)
./jtag generate/audit --type="widget" --fix
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `module` | string | No | Specific module path to audit (e.g., "commands/hello") |
| `type` | string | No | Module type to audit all of ("command", "widget", "daemon") |
| `fix` | boolean | No | Apply automatic fixes to fixable issues |
| `hibernateFailures` | boolean | No | Hibernate modules that can't be fixed (future) |

**Note**: Must specify either `module` or `type` (or both).

## Audit Checks

### 1. Linting
- **Check**: Runs eslint on module files
- **Fix**: Applies `eslint --fix` automatically

### 2. Missing Files (Future)
- **Check**: Verifies all required files exist
- **Fix**: Generates missing files from templates

### 3. Outdated Patterns (Future)
- **Check**: Detects deprecated patterns
- **Fix**: Applies current best practices

### 4. Package.json Validation (Future)
- **Check**: Verifies package.json matches spec
- **Fix**: Regenerates package.json

### 5. README Completeness (Future)
- **Check**: Verifies README has required sections
- **Fix**: Regenerates README sections

### 6. Test Coverage (Future)
- **Check**: Verifies unit and integration tests exist
- **Fix**: Generates missing test files

### 7. Hibernation Pollution (Future)
- **Check**: Detects backup directories in active locations
- **Fix**: Moves to /tmp/jtag-hibernation/

## Example Output

```bash
$ ./jtag generate/audit --module="commands/hello"

🔍 Auditing commands/hello...

✅ Linting: No issues
✅ Files: All required files present
✅ Patterns: Using current best practices
✅ Package.json: Valid and up-to-date
✅ README: Complete
✅ Tests: Unit and integration tests present
✅ Hibernation: No backup pollution detected

📊 Summary:
   0 errors, 0 warnings, 0 fixable

$ ./jtag generate/audit --type="command"

🔍 Auditing all command modules...

🔍 Auditing commands/hello...
✅ All checks passed

🔍 Auditing commands/screenshot...
❌ Linting: 3 errors (fixable)
  ❌ no-unused-vars: 'context' is defined but never used
     Location: commands/screenshot/server/ScreenshotServerCommand.ts:45

📊 Summary:
   3 errors, 0 warnings, 3 fixable

Run with --fix to automatically fix issues:
   ./jtag generate/audit --type="command" --fix
```

## Integration

### With Generator
After generating a command:
```bash
./jtag generate commands/my-command.spec.json
./jtag generate/audit --module="commands/my-command"
```

### With Hibernation (Future)
Before hibernating:
```bash
./jtag generate/audit --module="commands/old-feature" --fix
./jtag module/hibernate --name="old-feature"
```

### In Precommit Hook (Future)
```bash
# Run audit on changed modules
./jtag generate/audit --type="command" --fix
```

## Return Value

```typescript
{
  success: boolean;
  reports: AuditReport[];  // One per module audited
  summary: {
    modulesAudited: number;
    totalErrors: number;
    totalWarnings: number;
    totalFixed: number;
  };
  error?: string;
}
```

## Related Documentation

- [AUDIT-SYSTEM-DESIGN.md](../../../generator/AUDIT-SYSTEM-DESIGN.md) - Full audit system design
- [MODULE-HIBERNATION-SYSTEM.md](../../../generator/MODULE-HIBERNATION-SYSTEM.md) - Hibernation integration
