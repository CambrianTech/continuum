# System Docker Tier Stats Command

Snapshot of the Docker storage tier (capacity, used bytes, pressure ratio, detection state). Phase 1 of #1239 — exposes the data the existing `DockerTierPool` (`modules/docker_tier_pool.rs`) already computes, without depending on the not-yet-instantiated `PressureBroker` singleton. Wired so `bin/continuum status` can surface a `Docker disk: ...` row + warn at >90%, and so future scheduler hot paths can refuse before ENOSPC. Returns `detected: false` + zeros on hosts where Docker isn't installed.

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
./jtag system/docker-tier-stats 
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('system/docker-tier-stats', {
  // your parameters here
});
```

## Parameters

No parameters required.

## Result

Returns `SystemDockerTierStatsResult` with:

Returns CommandResult with:
- **stats**: `DockerTierStats` - { capacityBytes, usedBytes, pressure (0.0-1.0+), detected }. See shared/generated/resources/DockerTierStats.ts.

## Examples

### Print Docker tier usage from CLI

```bash
./jtag system/docker-tier-stats
```

**Expected result:**
{ capacityBytes: 64424509440, usedBytes: 12884901888, pressure: 0.20, detected: true }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help system/docker-tier-stats
```

**Tool:**
```typescript
// Use your help tool with command name 'system/docker-tier-stats'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme system/docker-tier-stats
```

**Tool:**
```typescript
// Use your readme tool with command name 'system/docker-tier-stats'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/System Docker Tier Stats/test/unit/SystemDockerTierStatsCommand.test.ts
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
npx tsx commands/System Docker Tier Stats/test/integration/SystemDockerTierStatsIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SystemDockerTierStatsTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SystemDockerTierStatsBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SystemDockerTierStatsServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SystemDockerTierStatsCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SystemDockerTierStatsIntegration.test.ts`
