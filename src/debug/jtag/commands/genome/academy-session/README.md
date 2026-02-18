# Genome Academy Session Command

Entry point for the Academy Dojo system. Creates an AcademySessionEntity and spawns dual sentinels (teacher + student) for autonomous skill training.

## Table of Contents

- [Usage](#usage)
  - [CLI Usage](#cli-usage)
  - [Tool Usage](#tool-usage)
- [Parameters](#parameters)
- [Result](#result)
- [Examples](#examples)
- [Architecture](#architecture)
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
./jtag genome/academy-session --personaId="<uuid>" --personaName="Helper AI" --skill="typescript-generics"
```

### Tool Usage

From Persona tools or programmatic access using `Commands.execute()`:

```typescript
import { GenomeAcademySession } from '@commands/genome/academy-session/shared/GenomeAcademySessionTypes';

const result = await GenomeAcademySession.execute({
  personaId: '<persona-uuid>',
  personaName: 'Helper AI',
  skill: 'typescript-generics',
  baseModel: 'smollm2:135m',
  passingScore: 70,
});
```

## Parameters

- **personaId** (required): `UUID` - The student persona ID
- **personaName** (required): `string` - Student persona display name
- **skill** (required): `string` - Skill to teach (e.g., "typescript-generics", "ethical-reasoning")
- **baseModel** (optional): `string` - Base model for training (default: "smollm2:135m")
- **maxTopicAttempts** (optional): `number` - Maximum attempts per topic before failure (default: 3)
- **passingScore** (optional): `number` - Score required to pass exams, 0-100 (default: 70)
- **epochs** (optional): `number` - Training epochs per round (default: 3)
- **rank** (optional): `number` - LoRA rank (default: 32)
- **model** (optional): `string` - Teacher LLM model
- **provider** (optional): `string` - Teacher LLM provider

## Result

Returns `GenomeAcademySessionResult` with:

- **success**: `boolean` - Whether session was created and sentinels spawned
- **academySessionId**: `UUID` - The created Academy session ID
- **teacherHandle**: `string` - Sentinel handle for the teacher pipeline
- **studentHandle**: `string` - Sentinel handle for the student pipeline
- **error**: `string` (optional) - Error message if failed

## Examples

### Basic session

```bash
./jtag genome/academy-session --personaId="00000000-0000-0000-0000-000000000002" --personaName="Helper AI" --skill="typescript-generics"
```

**Expected result:**
```json
{ "success": true, "academySessionId": "<uuid>", "teacherHandle": "abc123", "studentHandle": "def456" }
```

### Track session progress

```bash
# Check teacher sentinel status
./jtag sentinel/status --handle="abc123"

# Check student sentinel status
./jtag sentinel/status --handle="def456"

# View session entity
./jtag data/read --collection="academy_sessions" --id="<session-uuid>"
```

### Custom training parameters

```bash
./jtag genome/academy-session --personaId="<uuid>" --personaName="Code Tutor" --skill="react-hooks" --baseModel="smollm2:135m" --passingScore=80 --maxTopicAttempts=5 --epochs=5 --rank=64
```

## Architecture

The Academy Dojo spawns two autonomous sentinels that communicate via emit/watch events:

```
Teacher Sentinel                    Student Sentinel
  1. Design curriculum                1. Watch: curriculum:ready
  2. Loop per topic:                  2. Loop:
     a. Synthesize training data         a. Watch: dataset:ready
     b. Emit: dataset:ready              b. Train (genome/train)
     c. Watch: training:complete         c. Emit: training:complete
     d. Generate exam                    d. Watch: exam:ready
     e. Emit: exam:ready                 e. Take exam (LLM)
     f. Watch: exam:responses            f. Emit: exam:responses
     g. Grade & emit: exam:graded        g. Watch: exam:graded
     h. Remediate if failed              h. Register adapter if passed
  3. Emit: session:complete           3. Compose final genome
```

See `docs/personas/ACADEMY-DOJO-ARCHITECTURE.md` for full design.

## Getting Help

### Using the Help Tool

```bash
./jtag help genome/academy-session
```

### Using the README Tool

```bash
./jtag readme genome/academy-session
```

## Testing

### Unit Tests

```bash
npx vitest run tests/unit/semantic-cognition.test.ts
```

### Integration Tests

```bash
# Prerequisites: Server must be running + Rust sentinel engine
npm start  # Wait 90+ seconds for deployment

npx vitest run tests/integration/sentinel-lora-training.test.ts
```

## Access Level

**ai-safe** - Safe for AI personas to call autonomously. Intended for self-directed learning where a persona initiates its own training.

## Implementation Notes

- **Shared Logic**: Types and factories in `shared/GenomeAcademySessionTypes.ts`
- **Browser**: Delegates to server in `browser/GenomeAcademySessionBrowserCommand.ts`
- **Server**: Orchestration in `server/GenomeAcademySessionServerCommand.ts`
- Entities: `AcademySessionEntity`, `AcademyCurriculumEntity`, `AcademyExaminationEntity`
- Pipelines: `TeacherPipeline.ts`, `StudentPipeline.ts` in `system/sentinel/pipelines/`
- Events scoped by session: `academy:{sessionId}:{action}`
