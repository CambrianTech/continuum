# Genome Train Resume Command

Resume a crashed or failed training job from its latest checkpoint. Looks up the TrainingJobEntity, verifies checkpoint exists on disk, and restarts genome/train with resumeFromCheckpoint pointing to the latest checkpoint directory.

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
./jtag genome/train/resume --jobId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/train/resume', {
  // your parameters here
});
```

## Parameters

- **jobId** (required): `string` - TrainingJobEntity UUID of the job to resume
- **checkpoint** (optional): `string` - Specific checkpoint path to resume from (default: latest checkpoint)

## Result

Returns `GenomeTrainResumeResult` with:

Returns CommandResult with:
- **resumed**: `boolean` - Whether the job was successfully resumed
- **jobId**: `string` - The training job UUID
- **checkpointStep**: `number` - Step number of the checkpoint being resumed from
- **checkpointPath**: `string` - Path to the checkpoint directory
- **crashCount**: `number` - Number of times this job has been resumed (including this one)
- **sentinelHandle**: `string` - New sentinel handle for the resumed training process

## Examples

### Resume a crashed training job

```bash
./jtag genome/train/resume --jobId=abc123-def456
```

**Expected result:**
{ resumed: true, checkpointStep: 4700, crashCount: 1 }

### Resume from a specific checkpoint

```bash
./jtag genome/train/resume --jobId=abc123 --checkpoint=/path/to/checkpoint-2000
```

**Expected result:**
{ resumed: true, checkpointStep: 2000, crashCount: 2 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/train/resume
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/train/resume'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/train/resume
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/train/resume'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Train Resume/test/unit/GenomeTrainResumeCommand.test.ts
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
npx tsx commands/Genome Train Resume/test/integration/GenomeTrainResumeIntegration.test.ts
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

**human-only** - Unknown access level

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/GenomeTrainResumeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeTrainResumeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeTrainResumeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeTrainResumeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeTrainResumeIntegration.test.ts`
