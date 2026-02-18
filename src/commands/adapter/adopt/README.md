# Adapter Adopt Command

Add an adapter to a persona's genome, making it a permanent trait

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
./jtag adapter/adopt --adapterId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('adapter/adopt', {
  // your parameters here
});
```

## Parameters

- **adapterId** (required): `string` - Adapter ID (HuggingFace repo ID or local adapter name)
- **scale** (optional): `number` - Adapter weight/scale (0-1, default: 1.0)
- **traitType** (optional): `string` - Domain/trait type (e.g., 'code', 'tone', 'domain_expertise')
- **personaId** (optional): `string` - Target persona ID (default: calling persona)

## Result

Returns `AdapterAdoptResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether adapter was adopted successfully
- **adapterId**: `string` - Adopted adapter ID
- **layerId**: `string` - Genome layer ID created
- **personaId**: `string` - Persona that adopted the adapter
- **genomeName**: `string` - Name of persona's genome
- **layerCount**: `number` - Number of layers in genome
- **metadata**: `object` - Adapter metadata (rank, base model, etc.)

## Examples

```bash
./jtag command-name
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help adapter/adopt
```

**Tool:**
```typescript
// Use your help tool with command name 'adapter/adopt'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme adapter/adopt
```

**Tool:**
```typescript
// Use your readme tool with command name 'adapter/adopt'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Adapter Adopt/test/unit/AdapterAdoptCommand.test.ts
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
npx tsx commands/Adapter Adopt/test/integration/AdapterAdoptIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AdapterAdoptTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AdapterAdoptBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AdapterAdoptServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AdapterAdoptCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AdapterAdoptIntegration.test.ts`
