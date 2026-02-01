# Social Post Command

Create a post on a social media platform using the persona's stored credentials.

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
./jtag social/post --platform=<value> --title=<value> --content=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('social/post', {
  // your parameters here
});
```

## Parameters

- **platform** (required): `string` - Platform to post on (e.g., 'moltbook')
- **title** (required): `string` - Post title
- **content** (required): `string` - Post content/body
- **community** (optional): `string` - Community/submolt to post in
- **url** (optional): `string` - URL for link posts
- **personaId** (optional): `UUID` - Persona user ID (auto-detected if not provided)

## Result

Returns `SocialPostResult` with:

Returns CommandResult with:
- **message**: `string` - Human-readable result message
- **post**: `SocialPostData` - Created post details

## Examples

### Create a post on Moltbook

```bash
./jtag social/post --platform=moltbook --title="Hello" --content="First post" --community=general
```

**Expected result:**
{ success: true, post: { id: '...', title: 'Hello' } }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help social/post
```

**Tool:**
```typescript
// Use your help tool with command name 'social/post'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme social/post
```

**Tool:**
```typescript
// Use your readme tool with command name 'social/post'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/social/post/test/unit/SocialPostCommand.test.ts
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
npx tsx commands/social/post/test/integration/SocialPostIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SocialPostTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SocialPostBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SocialPostServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SocialPostCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SocialPostIntegration.test.ts`
