# Ai Key Diff Command

Compare redacted AI key status entries and produce a value-free merge plan for trusted grid reconciliation.

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
./jtag ai/key/diff --localEntries='[...]' --remoteEntries='[...]' --targetNode=windows-rtx
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('ai/key/diff', {
  localEntries,
  remoteEntries,
  targetNode: 'windows-rtx',
});
```

## Parameters

- **localEntries** (required): `array` - Local redacted ai/key/status entries.
- **remoteEntries** (required): `array` - Remote redacted ai/key/status entries from a trusted target node.
- **targetNode** (optional): `string` - Optional target node id or name for merge-plan labels.

## Result

Returns `AiKeyDiffResult` with:

Returns CommandResult with:
- **mergePlanId**: `string` - Stable id for this value-free merge plan.
- **actions**: `array` - Merge actions containing provider/key/action/reason/fingerprint metadata only.
- **conflictCount**: `number` - Number of conflicts requiring owner approval.
- **actionCount**: `number` - Number of generated actions.

## Examples

### Compare local and remote redacted key states

```bash
./jtag ai/key/diff --localEntries='[...]' --remoteEntries='[...]' --targetNode=windows-rtx
```

**Expected result:**
{ success: true, actionCount: 1, conflictCount: 0 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help ai/key/diff
```

**Tool:**
```typescript
// Use your help tool with command name 'ai/key/diff'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme ai/key/diff
```

**Tool:**
```typescript
// Use your readme tool with command name 'ai/key/diff'
```

## Testing

### Unit Tests

Test value-free merge-plan behavior without server dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/ai/key/diff/test/unit/AiKeyDiffCommand.test.ts
```

**What's tested:**
- Same redacted fingerprints produce no-op actions
- Missing remote/local keys produce explicit copy-plan actions
- Different configured fingerprints produce conflicts
- Missing keys on both sides are omitted
- Merge plan ids are deterministic across input ordering
- Results never serialize raw secret values

### Integration Tests

Smoke-test the shared params/result factories:

```bash
npx tsx commands/ai/key/diff/test/integration/AiKeyDiffIntegration.test.ts
```

**What's tested:**
- Factory preservation of local/remote status arrays
- Default empty merge-plan fields

## Access Level

**owner-only** - This command compares redacted key metadata for trusted grid reconciliation.

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/AiKeyDiffPlanner.ts`
- **Browser**: Browser-specific implementation in `browser/AiKeyDiffBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AiKeyDiffServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AiKeyDiffCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AiKeyDiffIntegration.test.ts`
