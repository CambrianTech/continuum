# Genome Academy Competition Command

Launches a multi-persona competition: 1 shared teacher sentinel generates a curriculum, N student sentinels compete on the same exam questions. Rankings computed from exam scores across all topics.

## Table of Contents

- [Usage](#usage)
  - [CLI Usage](#cli-usage)
  - [Tool Usage](#tool-usage)
- [Parameters](#parameters)
- [Result](#result)
- [Examples](#examples)
- [Architecture](#architecture)
- [Testing](#testing)
- [Getting Help](#getting-help)
- [Access Level](#access-level)
- [Implementation Notes](#implementation-notes)

## Usage

### CLI Usage

```bash
./jtag genome/academy-competition --skill="typescript-generics" --competitors='[{"personaId":"<uuid1>","personaName":"Helper AI"},{"personaId":"<uuid2>","personaName":"Code Tutor"}]'
```

### Tool Usage

```typescript
import { GenomeAcademyCompetition } from '@commands/genome/academy-competition/shared/GenomeAcademyCompetitionTypes';

const result = await GenomeAcademyCompetition.execute({
  skill: 'typescript-generics',
  competitors: [
    { personaId: '<uuid1>', personaName: 'Helper AI' },
    { personaId: '<uuid2>', personaName: 'Code Tutor' },
  ],
  baseModel: 'smollm2:135m',
  passingScore: 70,
});
```

## Parameters

- **skill** (required): `string` - Skill to compete on (e.g., "typescript-generics")
- **competitors** (required): `CompetitorDef[]` - Array of competitors (minimum 2), each with `personaId` and `personaName`
- **baseModel** (optional): `string` - Base model for training (default: "smollm2:135m")
- **maxTopicAttempts** (optional): `number` - Maximum attempts per topic before failure (default: 3)
- **passingScore** (optional): `number` - Score required to pass exams, 0-100 (default: 70)
- **epochs** (optional): `number` - Training epochs per round (default: 3)
- **rank** (optional): `number` - LoRA rank (default: 32)
- **tournamentRounds** (optional): `number` - Number of tournament rounds (default: 1)
- **model** (optional): `string` - Teacher LLM model
- **provider** (optional): `string` - Teacher LLM provider

## Result

Returns `GenomeAcademyCompetitionResult` with:

- **success**: `boolean` - Whether competition was created and sentinels spawned
- **competitionId**: `UUID` - The created competition entity ID
- **teacherHandle**: `string` - Sentinel handle for the shared teacher pipeline
- **competitorHandles**: `CompetitorHandle[]` - Per-competitor handles with `personaId`, `personaName`, `studentHandle`, `sessionId`
- **error**: `string` (optional) - Error message if failed

## Examples

### Two-persona competition

```bash
./jtag genome/academy-competition \
  --skill="typescript-generics" \
  --competitors='[{"personaId":"00000000-0000-0000-0000-000000000002","personaName":"Helper AI"},{"personaId":"00000000-0000-0000-0000-000000000003","personaName":"Code Tutor"}]'
```

### Track competition progress

```bash
# Check teacher
./jtag sentinel/status --handle="<teacherHandle>"

# Check each student
./jtag sentinel/status --handle="<studentHandle1>"
./jtag sentinel/status --handle="<studentHandle2>"

# View competition entity
./jtag data/read --collection="competitions" --id="<competitionId>"
```

## Architecture

Extends the Academy Dojo dual-sentinel pattern to N students:

```
1 Teacher Sentinel (shared)
   |
   +---> Student Sentinel 1 (persona A)
   +---> Student Sentinel 2 (persona B)
   +---> Student Sentinel N (persona N)
```

All students receive the same curriculum and exam questions from the shared teacher. Each trains independently. Rankings are computed from exam scores.

See `docs/personas/ACADEMY-DOJO-ARCHITECTURE.md` for full design.

## Testing

```bash
# Unit tests
npx vitest run tests/unit/semantic-cognition.test.ts

# Integration tests (requires running server + Rust sentinel engine)
npm start
npx vitest run tests/integration/sentinel-lora-training.test.ts
```

## Getting Help

```bash
./jtag help genome/academy-competition
./jtag readme genome/academy-competition
```

## Access Level

**ai-safe** - Safe for AI personas to call autonomously.

## Implementation Notes

- **Shared Logic**: `shared/GenomeAcademyCompetitionTypes.ts`
- **Browser**: `browser/GenomeAcademyCompetitionBrowserCommand.ts`
- **Server**: `server/GenomeAcademyCompetitionServerCommand.ts`
- Entities: `CompetitionEntity` (collection: `competitions`)
- Per-competitor `AcademySessionEntity` for independent training tracking
- Events share competition ID as session scope for teacher broadcasts
