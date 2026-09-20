# Avatar Snapshot Command

Capture a Bevy 3D avatar snapshot as PNG for profile pictures. Allocates a temporary render slot, loads the persona's VRM model, waits for a clean frame, encodes as PNG, and saves to ~/.continuum/avatars/. Cached on disk — subsequent calls return immediately unless force=true.

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
./jtag avatar/snapshot --identity=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('avatar/snapshot', {
  // your parameters here
});
```

## Parameters

- **identity** (required): `string` - Persona identity string (e.g., 'helper', 'teacher'). Used to select avatar model and name the output file.
- **width** (optional): `number` - Render width in pixels (default: 480)
- **height** (optional): `number` - Render height in pixels (default: 480)
- **force** (optional): `boolean` - Force re-capture even if cached snapshot exists (default: false)

## Result

Returns `AvatarSnapshotResult` with:

Returns CommandResult with:
- **path**: `string` - Relative URL path to the avatar PNG (e.g., '/avatars/helper.png')
- **cached**: `boolean` - Whether the result came from disk cache (true) or was freshly captured (false)

## Examples

### Capture avatar for helper persona

```bash
./jtag avatar/snapshot --identity=helper
```

**Expected result:**
{ path: '/avatars/helper.png', cached: false }

### Force re-capture at higher resolution

```bash
./jtag avatar/snapshot --identity=helper --width=640 --height=640 --force=true
```

**Expected result:**
{ path: '/avatars/helper.png', cached: false }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help avatar/snapshot
```

**Tool:**
```typescript
// Use your help tool with command name 'avatar/snapshot'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme avatar/snapshot
```

**Tool:**
```typescript
// Use your readme tool with command name 'avatar/snapshot'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Avatar Snapshot/test/unit/AvatarSnapshotCommand.test.ts
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
npx tsx commands/Avatar Snapshot/test/integration/AvatarSnapshotIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/AvatarSnapshotTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AvatarSnapshotBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AvatarSnapshotServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/AvatarSnapshotCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/AvatarSnapshotIntegration.test.ts`
