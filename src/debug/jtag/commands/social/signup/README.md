# Social Signup Command

Register a persona on a social media platform (e.g., Moltbook). Creates an account with a chosen username and stores credentials for future use.

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
./jtag social/signup --platform=<value> --agentName=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('social/signup', {
  // your parameters here
});
```

## Parameters

- **platform** (required): `string` - Platform to register on (e.g., 'moltbook')
- **agentName** (required): `string` - Desired username on the platform
- **description** (optional): `string` - Profile description/bio
- **personaId** (optional): `UUID` - Persona user ID (auto-detected if not provided)
- **metadata** (optional): `Record<string, unknown>` - Additional platform-specific metadata

## Result

Returns `SocialSignupResult` with:

Returns CommandResult with:
- **message**: `string` - Human-readable result message
- **apiKey**: `string` - API key for future authenticated requests
- **agentName**: `string` - Assigned username on the platform
- **claimUrl**: `string` - URL to claim/verify the account
- **profileUrl**: `string` - URL to the agent's profile page
- **verificationCode**: `string` - Verification code if applicable

## Examples

### Register a persona on Moltbook

```bash
./jtag social/signup --platform=moltbook --agentName="helper-ai" --description="I help with code"
```

**Expected result:**
{ success: true, agentName: 'helper-ai', profileUrl: '...' }

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help social/signup
```

**Tool:**
```typescript
// Use your help tool with command name 'social/signup'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme social/signup
```

**Tool:**
```typescript
// Use your readme tool with command name 'social/signup'
```

## Testing

### Unit Tests

Test command logic in isolation using mock dependencies:

```bash
# Run unit tests (no server required)
npx tsx commands/Social Signup/test/unit/SocialSignupCommand.test.ts
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
npx tsx commands/Social Signup/test/integration/SocialSignupIntegration.test.ts
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

- **Shared Logic**: Core business logic in `shared/SocialSignupTypes.ts`
- **Browser**: Browser-specific implementation in `browser/SocialSignupBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/SocialSignupServerCommand.ts`
- **Unit Tests**: Isolated testing in `test/unit/SocialSignupCommand.test.ts`
- **Integration Tests**: System testing in `test/integration/SocialSignupIntegration.test.ts`
