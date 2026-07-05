# genome/dataset-import

Import training datasets from external sources into `.continuum/datasets/`.

## Usage

```bash
# Import RealClassEval benchmark (400 Python classes)
./jtag genome/dataset-import \
  --source="realclasseval" \
  --csvPath="/path/to/RealClassEval.csv" \
  --testsDir="/path/to/tests/" \
  --splitRatio=0.8

# Import generic CSV
./jtag genome/dataset-import \
  --source="csv" \
  --csvPath="/path/to/data.csv" \
  --name="my-dataset" \
  --userColumn="question" \
  --assistantColumn="answer"

# List available datasets
./jtag genome/dataset-import --list
```

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source` | `'csv' \| 'realclasseval'` | Yes | - | Dataset source type |
| `csvPath` | string | Yes | - | Path to CSV file |
| `testsDir` | string | realclasseval only | - | Path to test files directory |
| `outputDir` | string | No | `.continuum/datasets/<source>/` | Output directory |
| `splitRatio` | number | No | 0.8 | Train/eval split ratio |
| `userColumn` | string | csv only | `'input'` | CSV column for user content |
| `assistantColumn` | string | csv only | `'output'` | CSV column for assistant content |
| `name` | string | csv only | `'imported'` | Dataset name for manifest |
| `list` | boolean | No | false | List datasets instead of importing |

## Output

Returns a manifest with dataset metadata:
- `name`: Dataset name
- `totalExamples`: Total training examples
- `trainExamples` / `evalExamples`: Split counts
- `trainPath` / `evalPath`: Paths to JSONL files
- `source`: Source attribution (e.g., `"arxiv:2510.26130"`)

## RealClassEval

[RealClassEval](https://arxiv.org/abs/2510.26130) contains 400 real Python classes
(200 pre-cutoff, 200 post-cutoff) with PYNGUIN-generated tests.

Download the dataset first:
```bash
bash scripts/download-realclasseval.sh
```

## Architecture

All heavy I/O (CSV parsing, JSONL conversion, file writes) happens in the Rust
`DatasetModule`. This command is a thin TypeScript wrapper for CLI discoverability.
