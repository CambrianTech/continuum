# AI Logs Command

**Command:** `ai/logs`
**Environments:** Server-side only
**Purpose:** Read and analyze dedicated AI decision log file with rich filtering and statistics

## Overview

The `ai/logs` command provides a rich interface for viewing and analyzing all AI persona decision-making logs. Unlike `debug/logs` which shows general system logs, this command reads from a dedicated AI decision log file that contains:

- **Gating decisions** (RESPOND/SILENT)
- **Redundancy checks** (DISCARD/ALLOW)
- **Response posting** (POSTED)
- **Errors** during decision-making

## Log File Location

```
.continuum/jtag/sessions/system/00000000-0000-0000-0000-000000000000/logs/ai-decisions.log
```

## Usage Examples

### Basic Usage

```bash
# Show last 50 lines (default)
./jtag ai/logs

# Show last 20 lines
./jtag ai/logs --tailLines=20

# Show last 100 lines
./jtag ai/logs --tailLines=100
```

### Filtering by Persona

```bash
# Show decisions from specific AI persona
./jtag ai/logs --personaName="Helper AI"
./jtag ai/logs --personaName="Code Reviewer"
```

### Filtering by Decision Type

```bash
# Show only response decisions
./jtag ai/logs --decisionType=RESPOND

# Show only silent decisions (chose not to respond)
./jtag ai/logs --decisionType=SILENT

# Show only successful posts
./jtag ai/logs --decisionType=POSTED

# Show only redundancy checks
./jtag ai/logs --decisionType=REDUNDANCY-CHECK

# Show only errors
./jtag ai/logs --decisionType=ERROR

# Show all decision types (default)
./jtag ai/logs --decisionType=ALL
```

### Filtering by Room

```bash
# Show decisions for specific chat room (use short room ID)
./jtag ai/logs --roomId=5e71a0c8
```

### Statistics Analysis

```bash
# Show statistics summary with persona breakdown
./jtag ai/logs --includeStats=true

# Combine filters with statistics
./jtag ai/logs --personaName="Helper AI" --includeStats=true
./jtag ai/logs --roomId=5e71a0c8 --includeStats=true --tailLines=100
```

### Output Formats

```bash
# Human-readable text format (default)
./jtag ai/logs --format=text

# JSON format for programmatic analysis
./jtag ai/logs --format=json --includeStats=true
```

## Statistics Breakdown

When `--includeStats=true` is provided, the command returns:

```typescript
{
  totalDecisions: number;        // Total gating decisions (RESPOND + SILENT)
  responseCount: number;         // Number of RESPOND decisions
  silentCount: number;           // Number of SILENT decisions
  postedCount: number;           // Number of successful posts
  redundancyChecks: number;      // Total redundancy checks performed
  redundancyDiscards: number;    // Responses discarded as redundant
  errors: number;                // Errors during decision-making
  personaBreakdown: {
    [personaName: string]: {
      respond: number;           // RESPOND decisions by this persona
      silent: number;            // SILENT decisions by this persona
      posted: number;            // Successful posts by this persona
    }
  }
}
```

## Example Output

### Basic Text Output

```
[2025-10-13T07:45:23.123Z] Helper AI → RESPOND | Room: 5e71a0c8 | Reason: Mentioned by name | Message: "@Helper AI can you help with this?" | Sender: Joel | MENTIONED
[2025-10-13T07:45:23.456Z] Helper AI → REDUNDANCY-CHECK: ALLOW | Room: 5e71a0c8 | Reason: No similar recent responses | Draft: "I'd be happy to help! What specifically..."
[2025-10-13T07:45:24.789Z] Helper AI → POSTED | Room: 5e71a0c8 | Response: "I'd be happy to help! What specifically are you working on?"
```

### With Statistics

```json
{
  "success": true,
  "logPath": "/path/to/ai-decisions.log",
  "lines": [...],
  "totalLines": 145,
  "filteredLines": 145,
  "stats": {
    "totalDecisions": 48,
    "responseCount": 12,
    "silentCount": 36,
    "postedCount": 10,
    "redundancyChecks": 12,
    "redundancyDiscards": 2,
    "errors": 0,
    "personaBreakdown": {
      "Helper AI": {
        "respond": 8,
        "silent": 15,
        "posted": 7
      },
      "Code Reviewer": {
        "respond": 4,
        "silent": 21,
        "posted": 3
      }
    }
  }
}
```

## Command Parameters

| Parameter | Type | Description | Default |
|-----------|------|-------------|---------|
| `personaName` | string | Filter by persona name | (none) |
| `decisionType` | enum | Filter by type: RESPOND, SILENT, POSTED, REDUNDANCY-CHECK, ERROR, ALL | ALL |
| `roomId` | string | Filter by room (short ID like "5e71a0c8") | (none) |
| `tailLines` | number | Show last N lines | 50 |
| `includeStats` | boolean | Include statistics summary | false |
| `format` | enum | Output format: text, json | text |
| `since` | string | ISO timestamp or relative (e.g., "5m", "1h") | (none) |
| `until` | string | ISO timestamp | (none) |

## Use Cases

### Debugging AI Responsiveness

```bash
# Why isn't my AI responding?
./jtag ai/logs --personaName="Helper AI" --tailLines=30

# Check if AI is seeing messages at all
./jtag ai/logs --decisionType=RESPOND --includeStats=true

# Check for errors
./jtag ai/logs --decisionType=ERROR --tailLines=10
```

### Analyzing Response Patterns

```bash
# How often is redundancy filtering working?
./jtag ai/logs --decisionType=REDUNDANCY-CHECK --includeStats=true

# Which personas are most active?
./jtag ai/logs --includeStats=true

# Activity in specific room
./jtag ai/logs --roomId=5e71a0c8 --includeStats=true
```

### Performance Analysis

```bash
# How many decisions per minute?
./jtag ai/logs --tailLines=200 --includeStats=true

# Successful post rate (posted / respond decisions)
./jtag ai/logs --includeStats=true
```

## Related Commands

- **`debug/logs`** - General system logs (includes AI logs + all other system activity)
- **`debug/widget-events`** - Widget event debugging
- **`screenshot`** - Visual verification of UI state

## Architecture Notes

- **Server-side only**: AI decision logs are written server-side where PersonaUser evaluations happen
- **Dedicated log file**: Separate from general system logs to avoid pollution
- **Real-time logging**: All AI decisions are logged immediately (gating, redundancy, posting, errors)
- **Singleton logger**: `AIDecisionLogger` ensures consistent formatting across all personas

## Log Format

Each log entry follows this format:

```
[ISO_TIMESTAMP] PERSONA_NAME → DECISION_TYPE | Room: SHORT_ROOM_ID | Context...
```

Examples:
- `[2025-10-13T07:45:23.123Z] Helper AI → RESPOND | Room: 5e71a0c8 | Reason: Mentioned by name | Message: "..." | Sender: Joel | MENTIONED`
- `[2025-10-13T07:45:23.456Z] Helper AI → REDUNDANCY-CHECK: ALLOW | Room: 5e71a0c8 | Reason: No similar recent responses | Draft: "..."`
- `[2025-10-13T07:45:24.789Z] Helper AI → POSTED | Room: 5e71a0c8 | Response: "..."`
- `[2025-10-13T07:45:25.999Z] Helper AI → ERROR | Operation: generateResponse | Error: Model timeout`

## Implementation

- **Types**: `commands/ai/logs/shared/AILogsTypes.ts`
- **Server Command**: `commands/ai/logs/server/AILogsServerCommand.ts`
- **Browser Command**: `commands/ai/logs/browser/AILogsBrowserCommand.ts` (returns error - server-side only)
- **Logger**: `system/ai/server/AIDecisionLogger.ts`
