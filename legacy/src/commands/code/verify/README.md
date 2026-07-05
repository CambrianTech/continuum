# code/verify

Run TypeScript compilation checks and optionally execute tests against a persona workspace. Returns structured errors with file, line, column, and message.

## Usage

```bash
# Check TypeScript compilation in persona workspace
./jtag code/verify --userId="persona-uuid"

# Check with explicit working directory
./jtag code/verify --userId="persona-uuid" --cwd="/path/to/workspace"

# Skip type checking, only run tests
./jtag code/verify --userId="persona-uuid" --typeCheck=false --testFiles='["tests/unit/foo.test.ts"]'

# Type check + run specific tests
./jtag code/verify --userId="persona-uuid" --testFiles='["tests/unit/foo.test.ts"]'
```

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `userId` | string | *required* | Persona ID or workspace handle |
| `typeCheck` | boolean | `true` | Run TypeScript compilation check |
| `testFiles` | string[] | `[]` | Test files to run via vitest |
| `cwd` | string | *auto* | Working directory override |

## Result

```typescript
{
  success: boolean;
  typeCheck?: {
    passed: boolean;
    errorCount: number;
    errors: TypeScriptError[];
  };
  tests?: {
    passed: boolean;
    total: number;
    passedCount: number;
    failedCount: number;
    failures: string[];
  };
  durationMs: number;
  output: string;
}
```

## TypeScript Error Format

```typescript
{
  file: "src/utils.ts",
  line: 42,
  column: 5,
  code: "TS2345",
  message: "Argument of type 'string' is not assignable to parameter of type 'number'."
}
```

## Security

- Uses `ExecutionSandbox` for process isolation (restricted PATH, timeout enforcement)
- Allowed commands: `npx tsc`, `npx vitest` (via sandbox allowlist)
- No file modification — verification is read-only
- Available at **write** security tier (same tier as code/edit)
