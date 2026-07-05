# Adapter Publish Command

Publish a trained LoRA adapter to HuggingFace with auto-generated model card and continuum:* tags. The adapter manifest metadata (role, skill, scores, base model) becomes discoverable via adapter/search. Every published adapter is an advertisement for the Continuum ecosystem.

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
./jtag adapter/publish --adapterPath=<value> --repoId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('adapter/publish', {
  // your parameters here
});
```

## Parameters

- **adapterPath** (required): `string` - Path to the adapter directory containing adapter_model.safetensors and manifest.json
- **repoId** (required): `string` - HuggingFace repo ID to publish to (e.g., 'continuum-ai/sprite-artist-pixel-games-qwen14b')
- **projectType** (optional): `string` - Project type tag (e.g., 'game-development', 'web-app', 'music-production')
- **academySessionId** (optional): `string` - Academy session ID to pull exam scores and before/after data for the model card
- **teamProjectId** (optional): `string` - Team project ID to pull project context and role grades for the model card
- **private** (optional): `boolean` - Publish as private repo (default: false)
- **update** (optional): `boolean` - Update existing repo instead of creating new (default: false). Pushes new weights + regenerates model card with latest training data.

## Result

Returns `AdapterPublishResult` with:

Returns CommandResult with:
- **repoUrl**: `string` - Full HuggingFace URL to the published adapter
- **tags**: `object` - Array of continuum:* tags applied to the repo
- **modelCardGenerated**: `boolean` - Whether a model card with training data was auto-generated

## Examples

### Publish an adapter trained by academy

```bash
undefined
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help adapter/publish
```

**Tool:**
```typescript
// Use your help tool with command name 'adapter/publish'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme adapter/publish
```

**Tool:**
```typescript
// Use your readme tool with command name 'adapter/publish'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Adapter Publish/test/unit/AdapterPublishCommand.test.ts
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
npx tsx commands/Adapter Publish/test/integration/AdapterPublishIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AdapterPublishTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AdapterPublishBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AdapterPublishServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AdapterPublishCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AdapterPublishIntegration.test.ts`
