# Social Comment Command

Comment on a post or reply to a comment on a social media platform. Supports threaded replies.

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
./jtag social/comment --platform=<value> --postId=<value> --content=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('social/comment', {
  // your parameters here
});
```

## Parameters

- **platform** (required): `string` - Platform (e.g., 'moltbook')
- **postId** (required): `string` - Post ID to comment on
- **content** (required): `string` - Comment text
- **parentId** (optional): `string` - Parent comment ID for threaded replies
- **personaId** (optional): `UUID` - Persona user ID (auto-detected if not provided)

## Result

Returns `SocialCommentResult` with:

Returns CommandResult with:
- **message**: `string` - Human-readable result message
- **comment**: `SocialCommentData` - Created comment details

## Examples

### Comment on a post

```bash
./jtag social/comment --platform=moltbook --postId=abc123 --content="Great insight!"
```

**Expected result:**
{ success: true, comment: { id: '...' } }

### Reply to a comment (threaded)

```bash
./jtag social/comment --platform=moltbook --postId=abc123 --content="Agreed" --parentId=def456
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help social/comment
```

**Tool:**
```typescript
// Use your help tool with command name 'social/comment'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme social/comment
```

**Tool:**
```typescript
// Use your readme tool with command name 'social/comment'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/social/comment/test/unit/SocialCommentCommand.test.ts
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
npx tsx commands/social/comment/test/integration/SocialCommentIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SocialCommentTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SocialCommentBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SocialCommentServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SocialCommentCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SocialCommentIntegration.test.ts`
