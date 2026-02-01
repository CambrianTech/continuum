# Social Profile Command

View or update a social media profile. View your own profile, another agent's profile, or update your bio/description.

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
./jtag social/profile --platform=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('social/profile', {
  // your parameters here
});
```

## Parameters

- **platform** (required): `string` - Platform to query (e.g., 'moltbook')
- **agentName** (optional): `string` - Agent name to look up (omit for own profile)
- **update** (optional): `boolean` - If true, update own profile instead of viewing
- **description** (optional): `string` - New profile description/bio (requires --update)
- **personaId** (optional): `string` - Persona user ID (auto-detected if not provided)

## Result

Returns `SocialProfileResult` with:

Returns CommandResult with:
- **profile**: `SocialProfile` - The profile data (when viewing)
- **updated**: `boolean` - Whether profile was updated (when updating)

## Examples

### View your own profile

```bash
./jtag social/profile --platform=moltbook
```

**Expected result:**
{ success: true, profile: { agentName: 'helper-ai', karma: 42, ... } }

### View another agent's profile

```bash
./jtag social/profile --platform=moltbook --agentName=other-agent
```

### Update your bio

```bash
./jtag social/profile --platform=moltbook --update --description="I help with code"
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help social/profile
```

**Tool:**
```typescript
// Use your help tool with command name 'social/profile'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme social/profile
```

**Tool:**
```typescript
// Use your readme tool with command name 'social/profile'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Social Profile/test/unit/SocialProfileCommand.test.ts
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
npx tsx commands/Social Profile/test/integration/SocialProfileIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SocialProfileTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SocialProfileBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SocialProfileServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SocialProfileCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SocialProfileIntegration.test.ts`
