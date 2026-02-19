# sentinel/list

List saved sentinel definitions from database.

## Usage

### List all sentinels

```bash
./jtag sentinel/list
```

### Filter by type

```bash
./jtag sentinel/list --type=build
./jtag sentinel/list --type=orchestrate
```

### Search by name

```bash
./jtag sentinel/list --search="production"
```

### Filter by tags

```bash
./jtag sentinel/list --tags='["deployment","ci"]'
```

### List templates only

```bash
./jtag sentinel/list --templatesOnly=true
```

## Response

```json
{
  "success": true,
  "sentinels": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "shortId": "550e8400",
      "name": "production-build",
      "type": "build",
      "description": "Production build with type checking",
      "tags": ["ci", "production"],
      "isTemplate": false,
      "executionCount": 15,
      "lastRun": "2024-01-15T10:30:00.000Z",
      "lastSuccess": true,
      "createdAt": "2024-01-01T00:00:00.000Z",
      "createdBy": "user-id"
    }
  ],
  "total": 1
}
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| type | string | No | Filter by sentinel type |
| tags | string[] | No | Filter by tags (any match) |
| templatesOnly | boolean | No | Only show templates |
| search | string | No | Search by name |
| limit | number | No | Limit results (default: 20) |
