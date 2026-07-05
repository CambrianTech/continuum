# Gpu Stats Command

Query GPU memory manager stats including VRAM detection, per-subsystem budgets (inference, TTS, rendering), usage tracking, and memory pressure. Returns real hardware data from Metal (macOS) or CUDA APIs.

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
./jtag gpu/stats [options]
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('gpu/stats', {
  // your parameters here
});
```

## Parameters

- **subsystem** (optional): `string` - Filter to specific subsystem: 'inference', 'tts', or 'rendering'. Omit for full stats.

## Result

Returns `GpuStatsResult` with:

Returns CommandResult with:
- **gpuName**: `string` - GPU hardware name (e.g., 'Apple M3 Max', 'NVIDIA RTX 5090')
- **totalVramMb**: `number` - Total detected VRAM in MB
- **totalUsedMb**: `number` - Total VRAM used across all subsystems in MB
- **pressure**: `number` - Memory pressure 0.0-1.0 (0=idle, 0.6=warning, 0.8=high, 0.95=critical)
- **reserveMb**: `number` - Reserved headroom in MB (5% of total, prevents OOM)
- **rendering**: `SubsystemInfo` - Rendering subsystem budget and usage
- **inference**: `SubsystemInfo` - Inference subsystem budget and usage (models, LoRA adapters)
- **tts**: `SubsystemInfo` - TTS subsystem budget and usage

## Examples

### Get full GPU stats

```bash
./jtag gpu/stats
```

**Expected result:**
{ gpuName: 'Apple M3 Max', totalVramMb: 36864, pressure: 0.12, inference: { budgetMb: 25804, usedMb: 3200 }, ... }

### Get inference subsystem only

```bash
./jtag gpu/stats --subsystem=inference
```

**Expected result:**
{ gpuName: 'Apple M3 Max', totalVramMb: 36864, pressure: 0.12, inference: { budgetMb: 25804, usedMb: 3200 } }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help gpu/stats
```

**Tool:**
```typescript
// Use your help tool with command name 'gpu/stats'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme gpu/stats
```

**Tool:**
```typescript
// Use your readme tool with command name 'gpu/stats'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Gpu Stats/test/unit/GpuStatsCommand.test.ts
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
npx tsx commands/Gpu Stats/test/integration/GpuStatsIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/GpuStatsTypes.ts`
- **Browser**: Browser-specific implementation in `browser/GpuStatsBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/GpuStatsServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/GpuStatsCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/GpuStatsIntegration.test.ts`
