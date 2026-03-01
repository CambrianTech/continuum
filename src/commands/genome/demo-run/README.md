# genome/demo-run

Run a sentinel demo pipeline where Claude Code builds real software from a project spec, capturing all interactions for LoRA training.

## Usage

```bash
# Run task-tracker demo (3 milestones, ~$3-7, ~20 minutes)
./jtag genome/demo-run --project=task-tracker --personaId=<uuid>

# With custom settings
./jtag genome/demo-run \
  --project=task-tracker \
  --personaId=<uuid> \
  --personaName="Helper AI" \
  --maxRetries=3 \
  --maxBudget=10 \
  --maxTurns=40
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| project | Yes | - | Project name (maps to `src/projects/<name>/`) |
| personaId | Yes | - | Target persona UUID for training |
| personaName | No | 'demo-persona' | Persona display name |
| baseModel | No | LOCAL_MODELS.DEFAULT | Base model for LoRA training |
| maxRetries | No | 2 | Max CodingAgent retries per milestone |
| maxBudget | No | 5.0 | Max USD per milestone |
| maxTurns | No | 30 | Max CodingAgent turns per milestone |
| provider | No | 'claude-code' | CodingAgent provider |
| epochs | No | 3 | LoRA training epochs |
| rank | No | 32 | LoRA rank |

## Pipeline Flow

```
Step 0: Shell — create temp dir, copy scaffold, npm install
Step 1: Loop (milestones):
  loop.0: Shell — read milestone spec
  loop.1: CodingAgent — Claude Code builds it (captureTraining=true)
  loop.2: Shell — run deterministic tests
  loop.3: Condition — passed?
    then: emit milestone:passed
    else: CodingAgent retry → re-run tests (up to maxRetries)
  loop.4: Emit milestone:complete
Step 2: Command — genome/train (captured interactions → LoRA adapter)
Step 3: Command — genome/phenotype-validate (before vs after)
Step 4: Emit — demo:complete
```

## Available Projects

| Project | Milestones | Difficulty | Est. Cost |
|---------|-----------|------------|-----------|
| task-tracker | 3 | Intermediate | $3-7 |
| ecommerce-api | 6 | Advanced | $10-20 |
| url-shortener | 3 | Beginner | $2-5 |

## Monitoring

```bash
# Check pipeline status
./jtag sentinel/status --handle=<returned-handle>

# Tail pipeline logs
./jtag sentinel/logs/tail --handle=<returned-handle>

# Check training data
./jtag data/list --collection=genome_layers --limit=5
```
