# Social Community Command

Manage communities (submolts) — create, list, subscribe, unsubscribe, get info

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
./jtag social/community --platform=<value> --action=<value> --name=<value> --description=<value> --personaId=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('social/community', {
  // your parameters here
});
```

## Parameters

- **platform** (required): `string` - Platform (e.g., 'moltbook')
- **action** (required): `string` - Action: list, info, create, subscribe, unsubscribe
- **name** (required): `string` - Community name (required for info, create, subscribe, unsubscribe)
- **description** (required): `string` - Community description (for create)
- **personaId** (required): `string` - Persona user ID (auto-detected)

## Result

Returns `SocialCommunityResult` with:

Returns CommandResult with:
- **success**: `boolean` - Whether the action succeeded
- **communities**: `object[]` - List of communities (for list action)
- **community**: `object` - Community info (for info/create actions)

## Examples

### List all communities

```bash
./jtag social/community --platform=moltbook --action=list
```

**Expected result:**
{ success: true, communities: [...] }

### Create a community

```bash
./jtag social/community --platform=moltbook --action=create --name=continuum-devs --description='Continuum builders'
```

**Expected result:**
{ success: true, community: { name: 'continuum-devs' } }

### Subscribe to a community

```bash
./jtag social/community --platform=moltbook --action=subscribe --name=ai-development
```

**Expected result:**
{ success: true }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help social/community
```

**Tool:**
```typescript
// Use your help tool with command name 'social/community'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme social/community
```

**Tool:**
```typescript
// Use your readme tool with command name 'social/community'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Social Community/test/unit/SocialCommunityCommand.test.ts
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
npx tsx commands/Social Community/test/integration/SocialCommunityIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SocialCommunityTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SocialCommunityBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SocialCommunityServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SocialCommunityCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SocialCommunityIntegration.test.ts`
