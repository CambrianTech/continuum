# Airc Bridge Command

Ingest one AIRC message into Continuum. Normal messages become chat; explicit !continuum directives become bounded development and test commands. This is the inbox-side companion to airc/send: it lets AIRC peers drive Continuum validation without shelling through jtag chat/send or chat/export by hand.

## Table of Contents

- [Usage](#usage)
  - [CLI Usage](#cli-usage)
  - [Tool Usage](#tool-usage)
- [Parameters](#parameters)
- [Result](#result)
- [Examples](#examples)
- [Testing](#testing)
  - [Unit Tests](#unit-tests)
  - [Live Validation](#live-validation)
- [Getting Help](#getting-help)
- [Access Level](#access-level)
- [Implementation Notes](#implementation-notes)

## Usage

### CLI Usage

From the command line using the jtag CLI:

```bash
./jtag airc/bridge --message=<value>
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { Commands } from '@system/core/shared/Commands';

const result = await Commands.execute('airc/bridge', {
  message: '!continuum ping',
  senderNick: 'mac-codex',
  channel: 'general',
  dryRun: true
});
```

## Parameters

- **message** (required): `string` - Raw AIRC message body. Plain text is bridged into Continuum chat; messages beginning with the command prefix are parsed as bridge directives.
- **senderNick** (optional): `string` - AIRC sender nick used for attribution in bridged chat text.
- **channel** (optional): `string` - AIRC channel name, with or without leading #. Defaults to general.
- **room** (optional): `string` - Continuum room name to target. Defaults to general; the AIRC channel is preserved separately for attribution and mirroring.
- **commandPrefix** (optional): `string` - Directive prefix for test and control messages. Defaults to !continuum.
- **dryRun** (optional): `boolean` - Parse and report intent without executing Continuum commands.
- **mirrorResponse** (optional): `boolean` - Send bridge command responses back to AIRC via the airc CLI.

## Result

Returns `AircBridgeResult` with:

Returns CommandResult with:
- **handled**: `boolean` - True when the bridge executed the parsed action. Dry runs return handled=false.
- **parsed**: `ParsedAircBridgeMessage` - Structured parser output for the incoming AIRC message.
- **responseText**: `string` - Short human and AI readable response for the action.
- **mirrored**: `boolean` - True when response mirroring to AIRC was requested and handed off successfully.
- **mirrorError**: `string` - AIRC mirror failure, surfaced loudly instead of swallowed.
- **commandResult**: `unknown` - Underlying Continuum command result for directives such as chat export or activity list.

## Examples

### Dry-run a normal chat message from AIRC

```bash
./jtag airc/bridge --message='hello from airc' --senderNick=mac-codex --channel=general --dryRun=true
```

### Check bridge health from AIRC

```bash
./jtag airc/bridge --message='!continuum ping' --senderNick=win-claude --channel=general --mirrorResponse=true
```

### Assert a marker landed in Continuum chat

```bash
./jtag airc/bridge --message='!continuum assert seen marker-123 --room general --last 100' --senderNick=mac-codex --channel=general
```

## Getting Help

### Using the Help Tool

Get detailed usage information for this command:

**CLI:**
```bash
./jtag help airc/bridge
```

**Tool:**
```typescript
// Use your help tool with command name 'airc/bridge'
```

### Using the README Tool

Access this README programmatically:

**CLI:**
```bash
./jtag readme airc/bridge
```

**Tool:**
```typescript
// Use your readme tool with command name 'airc/bridge'
```

## Testing

### Unit Tests

Test parser behavior and the server command boundary:

```bash
# Run unit tests (no server required)
npm --prefix commands/airc/bridge run test:unit
```

**What's tested:**
- AIRC text/directive parsing
- Room/channel normalization
- Dry-run command execution
- Missing-message rejection through the command boundary

**TDD Workflow:**
1. Write/modify unit test first (test-driven development)
2. Run test, see it fail
3. Implement feature
4. Run test, see it pass
5. Refactor if needed

### Live Validation

Test the command against a matching running server with the branch deployed:

```bash
./jtag airc/bridge --message='!continuum ping' --senderNick=mac-codex --channel=general --dryRun=true
./jtag airc/bridge --message='hello from airc' --senderNick=mac-codex --channel=general
./jtag airc/bridge --message='!continuum assert seen marker-123 --room general --last 100'
```

**What's tested:**
- `airc/bridge` is registered in the active server process
- Chat messages route into Continuum chat
- Export/assert directives can read back recent chat state
- Optional AIRC mirroring fails loudly if the local bus is unavailable

**Best Practice:**
Run unit tests during development. Run live validation before PR review because `./jtag` talks to the currently running server, not necessarily the branch you just edited.

## Access Level

**ai-safe** - Safe for AI personas to call autonomously

## Implementation Notes

- **Shared Logic**: Core business logic in `shared/AircBridgeTypes.ts`
- **Browser**: Browser-specific implementation in `browser/AircBridgeBrowserCommand.ts`
- **Server**: Server-specific implementation in `server/AircBridgeServerCommand.ts`
- **Protocol Tests**: Parser coverage in `test/unit/AircBridgeProtocolCheck.ts`
- **Server Tests**: Command boundary coverage in `test/unit/AircBridgeServerCommandCheck.ts`
