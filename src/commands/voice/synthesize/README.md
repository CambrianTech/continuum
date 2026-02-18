# Voice Synthesize Command

Synthesize text to speech using Rust TTS (Kokoro primary). Wraps the streaming-core TTS adapters for text-to-speech conversion.

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
./jtag voice/synthesize --text=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('voice/synthesize', {
  // your parameters here
});
```

## Parameters

- **text** (required): `string` - Text to synthesize
- **voice** (optional): `string` - Voice ID or name. Defaults to model's default voice.
- **adapter** (optional): `string` - TTS adapter: 'kokoro', 'fish-speech', 'f5-tts', 'styletts2', 'xtts-v2'. Defaults to 'kokoro'.
- **speed** (optional): `number` - Speech rate multiplier (0.5-2.0). Defaults to 1.0.
- **sampleRate** (optional): `number` - Output sample rate in Hz. Defaults to adapter's native rate (usually 24000).
- **format** (optional): `string` - Output format: 'pcm16', 'wav', 'opus'. Defaults to 'pcm16'.
- **stream** (optional): `boolean` - Return streaming handle instead of complete audio. Defaults to false.

## Result

Returns `VoiceSynthesizeResult` with:

Returns CommandResult with:
- **audio**: `string` - Base64-encoded audio data (if stream=false)
- **handle**: `string` - Streaming handle for WebSocket subscription (if stream=true)
- **sampleRate**: `number` - Actual sample rate of returned audio
- **duration**: `number` - Audio duration in seconds
- **adapter**: `string` - TTS adapter that was used

## Examples

### Synthesize text with default voice

```bash
./jtag voice/synthesize --text="Hello, how can I help you today?"
```

**Expected result:**
{ audio: "<base64-pcm>", sampleRate: 24000, duration: 1.8, adapter: "kokoro" }

### Synthesize with specific voice and adapter

```bash
./jtag voice/synthesize --text="Welcome!" --voice="amy" --adapter="kokoro" --speed=1.1
```

**Expected result:**
{ audio: "<base64>", sampleRate: 24000, duration: 0.6 }

### Request streaming audio

```bash
./jtag voice/synthesize --text="Long response..." --stream=true
```

**Expected result:**
{ handle: "tts-abc123", sampleRate: 24000 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help voice/synthesize
```

**Tool:**
```typescript
// Use your help tool with command name 'voice/synthesize'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme voice/synthesize
```

**Tool:**
```typescript
// Use your readme tool with command name 'voice/synthesize'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Voice Synthesize/test/unit/VoiceSynthesizeCommand.test.ts
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
npx tsx commands/Voice Synthesize/test/integration/VoiceSynthesizeIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/VoiceSynthesizeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/VoiceSynthesizeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/VoiceSynthesizeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/VoiceSynthesizeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/VoiceSynthesizeIntegration.test.ts`
