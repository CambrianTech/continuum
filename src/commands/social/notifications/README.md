# Social Notifications Command

Check for unread notifications (replies, mentions, followers) on a social media platform. Key data source for SocialMediaRAGSource.

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
./jtag social/notifications --platform=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('social/notifications', {
  // your parameters here
});
```

## Parameters

- **platform** (required): `string` - Platform to check (e.g., 'moltbook')
- **since** (optional): `string` - ISO timestamp to fetch notifications since
- **limit** (optional): `number` - Maximum number of notifications to return
- **personaId** (optional): `UUID` - Persona user ID (auto-detected if not provided)

## Result

Returns `SocialNotificationsResult` with:

Returns CommandResult with:
- **message**: `string` - Human-readable result message
- **notifications**: `SocialNotification[]` - Array of notifications
- **unreadCount**: `number` - Count of unread notifications

## Examples

### Check recent notifications

```bash
./jtag social/notifications --platform=moltbook
```

**Expected result:**
{ success: true, notifications: [...], unreadCount: 3 }

### Check notifications since a specific time

```bash
./jtag social/notifications --platform=moltbook --since=2026-01-30T00:00:00Z
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help social/notifications
```

**Tool:**
```typescript
// Use your help tool with command name 'social/notifications'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme social/notifications
```

**Tool:**
```typescript
// Use your readme tool with command name 'social/notifications'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/social/notifications/test/unit/SocialNotificationsCommand.test.ts
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
npx tsx commands/social/notifications/test/integration/SocialNotificationsIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SocialNotificationsTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SocialNotificationsBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SocialNotificationsServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SocialNotificationsCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SocialNotificationsIntegration.test.ts`
