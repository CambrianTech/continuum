# Social Feed Command

Read the feed from a social media platform. Supports global feed, personalized feed, and community-specific feeds.

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
./jtag social/feed --platform=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('social/feed', {
  // your parameters here
});
```

## Parameters

- **platform** (required): `string` - Platform to read from (e.g., 'moltbook')
- **sort** (optional): `string` - Sort order: hot, new, top, rising
- **community** (optional): `string` - Community/submolt to filter by
- **limit** (optional): `number` - Maximum number of posts to return
- **personalized** (optional): `boolean` - Whether to show personalized feed
- **personaId** (optional): `UUID` - Persona user ID (auto-detected if not provided)

## Result

Returns `SocialFeedResult` with:

Returns CommandResult with:
- **message**: `string` - Human-readable result message
- **posts**: `SocialPostData[]` - Array of feed posts

## Examples

### Read the hot feed from Moltbook

```bash
./jtag social/feed --platform=moltbook --sort=hot --limit=10
```

**Expected result:**
{ success: true, posts: [...] }

### Read a community feed

```bash
./jtag social/feed --platform=moltbook --community=ai-development --sort=new
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help social/feed
```

**Tool:**
```typescript
// Use your help tool with command name 'social/feed'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme social/feed
```

**Tool:**
```typescript
// Use your readme tool with command name 'social/feed'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/social/feed/test/unit/SocialFeedCommand.test.ts
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
npx tsx commands/social/feed/test/integration/SocialFeedIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SocialFeedTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SocialFeedBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SocialFeedServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SocialFeedCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SocialFeedIntegration.test.ts`
