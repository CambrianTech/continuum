# sentinel/save

Save sentinel definitions to database for persistence and sharing.

## Usage

### Save a definition directly

```bash
./jtag sentinel/save --definition='{"type":"build","name":"my-build","version":"1.0","command":"npm run build"}'
```

### Save from a running/completed handle

```bash
# First, run a sentinel
./jtag sentinel/run --type=build --command="npm run build"
# Returns: { "handle": "abc12345", ... }

# Then save its definition
./jtag sentinel/save --handle=abc12345 --name="production-build"
```

### Save as template

```bash
./jtag sentinel/save --definition='...' --isTemplate=true --name="build-template"
```

## Response

```json
{
  "success": true,
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "shortId": "550e8400",
  "entity": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "definition": { ... },
    "executions": [],
    "createdAt": "2024-01-01T00:00:00.000Z",
    "updatedAt": "2024-01-01T00:00:00.000Z",
    "createdBy": "user-id",
    "isTemplate": false
  }
}
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| definition | object/string | No* | Complete sentinel definition |
| handle | string | No* | Handle from sentinel/run to capture |
| name | string | No | Override the sentinel name |
| description | string | No | Add description |
| tags | string[] | No | Tags for organization |
| isTemplate | boolean | No | Mark as clonable template |

*Either `definition` or `handle` is required.
