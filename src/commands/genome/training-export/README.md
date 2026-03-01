# genome/training-export

Export accumulated training data from a PersonaUser's in-memory buffer to a JSONL file.

## Purpose

Bridges the gap between **interactive training capture** (via `persona/learning/capture-interaction`) and **LoRA training** (via `genome/train`). The capture system stores examples in RAM; this command writes them to disk as JSONL.

## Usage

```bash
./jtag genome/training-export \
  --personaId="550e8400-e29b-41d4-a716-446655440000" \
  --personaName="Helper AI" \
  --domain="coding"
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `personaId` | Yes | — | UUID of the persona whose buffer to export |
| `personaName` | Yes | — | Display name (used in output filename) |
| `domain` | Yes | — | Training domain (must match capture domain, e.g. `"coding"`) |
| `outputPath` | No | auto | Explicit output path; auto-generates under `.continuum/genome/datasets/` if omitted |

## Result

```json
{
  "success": true,
  "datasetPath": ".continuum/genome/datasets/helper-ai-coding-1709123456789.jsonl",
  "exampleCount": 42,
  "domain": "coding"
}
```

## Pipeline Usage

Used between CodingAgent capture and `genome/train` in sentinel pipelines:

```
CodingAgent (captureTraining=true) → genome/training-export → genome/train
```

The `datasetPath` from this command's result feeds directly into `genome/train`'s `datasetPath` parameter via interpolation: `{{steps.N.data.datasetPath}}`.

## JSONL Format

Each line is a training example:
```jsonl
{"messages":[{"role":"user","content":"..."},{"role":"assistant","content":"..."}]}
```

## Notes

- **Destructive read**: `consumeTrainingData()` clears the buffer after export. Call once per training cycle.
- **In-memory only**: If the server restarts, accumulated data is lost. Export promptly after capture.
- **Domain matching**: The `domain` parameter must exactly match what was used during capture (e.g. `"coding"` from sentinel CodingAgent steps).
