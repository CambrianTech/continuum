# Genome Train List Jobs Command

List all training jobs with status, progress, checkpoints, and node info. Shows running, completed, crashed, and resumable jobs. Use genome/train/resume to restart crashed jobs from their latest checkpoint.

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
./jtag genome/train/list-jobs [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('genome/train/list-jobs', {
  // your parameters here
});
```

## Parameters

- **status** (optional): `string` - Filter by job status: pending, running, checkpointed, completed, failed, crashed, cancelled
- **personaId** (optional): `string` - Filter by persona UUID
- **nodeId** (optional): `string` - Filter by grid node ID (e.g., '100.124.122.107' or 'local')
- **limit** (optional): `number` - Maximum number of jobs to return (default: 20)

## Result

Returns `GenomeTrainListJobsResult` with:

Returns CommandResult with:
- **jobs**: `object[]` - Array of training job summaries
- **totalCount**: `number` - Total number of matching jobs
- **activeCount**: `number` - Number of currently running/checkpointed jobs
- **resumableCount**: `number` - Number of crashed jobs that can be resumed

## Examples

### List all training jobs

```bash
./jtag genome/train/list-jobs
```

**Expected result:**
{ jobs: [...], totalCount: 5, activeCount: 1, resumableCount: 2 }

### Show only crashed/resumable jobs

```bash
./jtag genome/train/list-jobs --status=crashed
```

**Expected result:**
{ jobs: [...], totalCount: 2, resumableCount: 2 }

### Show jobs running on the 5090 tower

```bash
./jtag genome/train/list-jobs --nodeId=100.124.122.107
```

**Expected result:**
{ jobs: [...], totalCount: 3 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help genome/train/list-jobs
```

**Tool:**
```typescript
// Use your help tool with command name 'genome/train/list-jobs'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme genome/train/list-jobs
```

**Tool:**
```typescript
// Use your readme tool with command name 'genome/train/list-jobs'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Genome Train List Jobs/test/unit/GenomeTrainListJobsCommand.test.ts
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
npx tsx commands/Genome Train List Jobs/test/integration/GenomeTrainListJobsIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GenomeTrainListJobsTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GenomeTrainListJobsBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GenomeTrainListJobsServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GenomeTrainListJobsCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GenomeTrainListJobsIntegration.test.ts`
