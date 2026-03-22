# Persona Allocate Command

Hardware-aware persona allocation via Rust PersonaAllocator. Returns optimal persona assignments based on GPU VRAM and available API keys. Single source of truth for which personas should exist on this machine.

## Table of Contents

- [Usage](#usage)
  - [CLI Usage](#cli-usage)
  - [Tool Usage](#tool-usage)
- [Parameters](#parameters)
- [Result](#result)
- [Examples](#examples)
- [Testing](#testing)
  - [Unit Tests](#unit-tests)
  - [Integration Tests](#integration-tests)
- [Getting Help](#getting-help)
- [Access Level](#access-level)
- [Implementation Notes](#implementation-notes)

## Usage

### CLI Usage

From the command line using the jtag CLI:

```bash
./jtag persona/allocate --availableApiKeys=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('persona/allocate', {
  // your parameters here
});
```

## Parameters

- **availableApiKeys** (required): `string[]` - List of API key env var names that are currently set (e.g., ['ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY'])

## Result

Returns `PersonaAllocateResult` with:

Returns CommandResult with:
- **allocations**: `object[]` - Array of persona allocations to create
- **skipped**: `object[]` - Array of personas skipped (with reasons)
- **summary**: `string[]` - Human-readable summary lines of the allocation decision
- **gpuName**: `string` - Detected GPU hardware name
- **totalVramGb**: `number` - Total detected VRAM in GB
- **gpuType**: `string` - GPU type: 'cuda', 'metal', or 'cpu'
- **localModel**: `string` - Recommended local model for this hardware

## Examples

### Allocate with no API keys (local only)

```bash
./jtag persona/allocate --availableApiKeys='[]'
```

**Expected result:**
{ allocations: [{uniqueId:'helper', provider:'candle', ...}], gpuName: 'Apple M1 Pro', localModel: 'coder' }

### Allocate with Anthropic key

```bash
./jtag persona/allocate --availableApiKeys='["ANTHROPIC_API_KEY"]'
```

**Expected result:**
{ allocations: [...candle personas..., {uniqueId:'claude', provider:'anthropic'}], ... }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help persona/allocate
```

**Tool:**
```typescript
// Use your help tool with command name 'persona/allocate'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme persona/allocate
```

**Tool:**
```typescript
// Use your readme tool with command name 'persona/allocate'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Persona Allocate/test/unit/PersonaAllocateCommand.test.ts
```

**What's tested:**
- Command structure and parameter validation
- Mock command execution patterns
- Required parameter validation (throws ValidationError)
- Optional parameter handling (sensible defaults)
- Performance requirements
- Assertion utility helpers

**TDD Workflow:**
1. Write/modify unit test first (test-driven development)
2. Run test, see it fail
3. Implement feature
4. Run test, see it pass
5. Refactor if needed

### Integration Tests

Test command with real client connections and system integration:

```bash
# Prerequisites: Server must be running
npm start  # Wait 90+ seconds for deployment

# Run integration tests
npx tsx commands/Persona Allocate/test/integration/PersonaAllocateIntegration.test.ts
```

**What's tested:**
- Client connection to live system
- Real command execution via WebSocket
- ValidationError handling for missing params
- Optional parameter defaults
- Performance under load
- Various parameter combinations

**Best Practice:**
Run unit tests frequently during development (fast feedback). Run integration tests before committing (verify system integration).

## Access Level

**ai-safe** - Safe for AI personas to call autonomously

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/PersonaAllocateTypes.ts`
- **Browser**: Browser-specific implementation in `browser/PersonaAllocateBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/PersonaAllocateServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/PersonaAllocateCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/PersonaAllocateIntegration.test.ts`
