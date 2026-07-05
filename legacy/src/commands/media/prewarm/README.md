# Media Prewarm Command

Pre-warm vision description cache for image media. Fires VisionDescriptionService.describeBase64() so that by the time personas build RAG context, descriptions are cached. Called fire-and-forget by chat/send when images are attached.

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
./jtag media/prewarm --images=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('media/prewarm', {
  // your parameters here
});
```

## Parameters

- **images** (required): `object[]` - Array of image objects with base64 and mimeType fields

## Result

Returns `MediaPrewarmResult` with:

Returns CommandResult with:
- **queued**: `number` - Number of images queued for description generation

## Examples

### Pre-warm descriptions for uploaded images

```bash
./jtag media/prewarm --images='[{"base64":"...","mimeType":"image/jpeg"}]'
```

**Expected result:**
{ queued: 1 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help media/prewarm
```

**Tool:**
```typescript
// Use your help tool with command name 'media/prewarm'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme media/prewarm
```

**Tool:**
```typescript
// Use your readme tool with command name 'media/prewarm'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Media Prewarm/test/unit/MediaPrewarmCommand.test.ts
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
npx tsx commands/Media Prewarm/test/integration/MediaPrewarmIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/MediaPrewarmTypes.ts`
- **Browser**: Browser-specific implementation in `browser/MediaPrewarmBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/MediaPrewarmServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/MediaPrewarmCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/MediaPrewarmIntegration.test.ts`
