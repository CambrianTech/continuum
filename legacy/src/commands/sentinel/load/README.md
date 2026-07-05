# sentinel/load

Load and optionally run saved sentinel definitions from database.

## Usage

### Load a sentinel definition

```bash
./jtag sentinel/load --id=550e8400
```

### Load and run immediately

```bash
# Run async (returns handle)
./jtag sentinel/load --id=550e8400 --run=true

# Run sync (waits for completion)
./jtag sentinel/load --id=550e8400 --run=true --async=false
```

### Override working directory

```bash
./jtag sentinel/load --id=550e8400 --run=true --workingDir=/path/to/project
```

## Response

### Load only

```json
{
  "success": true,
  "entity": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "definition": {
      "type": "build",
      "name": "production-build",
      "version": "1.0",
      "command": "npm run build"
    },
    "executions": [...],
    "createdAt": "2024-01-01T00:00:00.000Z",
    "updatedAt": "2024-01-01T00:00:00.000Z"
  }
}
```

### Load and run

```json
{
  "success": true,
  "entity": { ... },
  "handle": "abc12345",
  "result": {
    "handle": "abc12345",
    "success": true,
    "startedAt": "2024-01-01T00:00:00.000Z",
    "completedAt": "2024-01-01T00:00:01.000Z",
    "data": { ... }
  }
}
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| id | string | Yes | Sentinel entity ID or shortId |
| run | boolean | No | Run immediately after loading |
| async | boolean | No | Run asynchronously (default: true) |
| workingDir | string | No | Override working directory |
