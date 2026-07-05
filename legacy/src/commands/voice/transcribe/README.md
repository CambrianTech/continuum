# Voice Transcribe Command

Transcribe audio to text using Rust Whisper (STT). Wraps the streaming-core Whisper adapter for speech-to-text conversion.

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
./jtag voice/transcribe --audio=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('voice/transcribe', {
  // your parameters here
});
```

## Parameters

- **audio** (required): `string` - Base64-encoded audio data (PCM 16-bit, 16kHz mono)
- **format** (optional): `string` - Audio format hint: 'pcm16', 'wav', 'opus' (defaults to 'pcm16')
- **language** (optional): `string` - Language code hint (e.g., 'en', 'es', 'auto'). Defaults to auto-detect.
- **model** (optional): `string` - Whisper model size: 'tiny', 'base', 'small', 'medium', 'large'. Defaults to 'base'.

## Result

Returns `VoiceTranscribeResult` with:

Returns CommandResult with:
- **text**: `string` - Transcribed text
- **language**: `string` - Detected language code
- **confidence**: `number` - Confidence score (0-1)
- **segments**: `object[]` - Word-level timestamps if available: [{word, start, end}]

## Examples

### Transcribe audio buffer

```bash
./jtag voice/transcribe --audio="<base64-pcm-data>"
```

**Expected result:**
{ text: "Hello world", language: "en", confidence: 0.95 }

### Transcribe with language hint

```bash
./jtag voice/transcribe --audio="<base64>" --language="es" --model="small"
```

**Expected result:**
{ text: "Hola mundo", language: "es", confidence: 0.92 }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help voice/transcribe
```

**Tool:**
```typescript
// Use your help tool with command name 'voice/transcribe'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme voice/transcribe
```

**Tool:**
```typescript
// Use your readme tool with command name 'voice/transcribe'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Voice Transcribe/test/unit/VoiceTranscribeCommand.test.ts
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
npx tsx commands/Voice Transcribe/test/integration/VoiceTranscribeIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/VoiceTranscribeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/VoiceTranscribeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/VoiceTranscribeServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/VoiceTranscribeCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/VoiceTranscribeIntegration.test.ts`
