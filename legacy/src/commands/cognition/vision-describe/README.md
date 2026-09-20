# Cognition Vision Describe Command

Describe an image via the best available vision-capable model. Selects a vision-capable model from the Rust model registry, builds the describe prompt from option flags, dispatches `ai/generate` with multimodal content (text + base64 image), and parses the response into a VisionDescription. Migrated from `system/vision/VisionInferenceProvider.ts` per #1276 (oxidizer freeform-shape outlier — pairs with codex's #1284 structured-decision shape). Returns null when no vision model is registered or generation fails.

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
./jtag cognition/vision-describe --base64Data=<value> --mimeType=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('cognition/vision-describe', {
  // your parameters here
});
```

## Parameters

- **base64Data** (required): `string` - Base64-encoded image bytes. The Rust adapter shapes this for the destination provider (Anthropic native base64, OpenAI image_url, llama.cpp mmproj).
- **mimeType** (required): `string` - Image MIME type (e.g. 'image/png', 'image/jpeg').
- **options** (optional): `VisionDescribeOptions` - Per-call describe knobs (preferredModel, preferredProvider, maxLength, prompt override, detectObjects, detectColors, detectText). Defaults: concise prose with no structured-extraction prompts.

## Result

Returns `CognitionVisionDescribeResult` with:

Returns CommandResult with:
- **result**: `VisionDescription | null` - Description envelope or null when no vision model is registered / generation failed. See shared/generated/cognition/VisionDescription.ts.

## Examples

### Describe a PNG screenshot for the chat-side vision pipeline

```bash
./jtag cognition/vision-describe --base64Data="<base64>" --mimeType="image/png"
```

**Expected result:**
{ description: 'A screenshot of...', modelId: '...', provider: '...', responseTimeMs: 1234 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help cognition/vision-describe
```

**Tool:**
```typescript
// Use your help tool with command name 'cognition/vision-describe'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme cognition/vision-describe
```

**Tool:**
```typescript
// Use your readme tool with command name 'cognition/vision-describe'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Cognition Vision Describe/test/unit/CognitionVisionDescribeCommand.test.ts
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
npx tsx commands/Cognition Vision Describe/test/integration/CognitionVisionDescribeIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/CognitionVisionDescribeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/CognitionVisionDescribeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/CognitionVisionDescribeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/CognitionVisionDescribeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/CognitionVisionDescribeIntegration.test.ts`
