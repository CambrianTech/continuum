# sentinel/status

Check the status of a running sentinel by its handle.

## Usage

```bash
./jtag sentinel/status --handle=abc12345
```

## Response

```json
{
  "success": true,
  "handle": "abc12345",
  "type": "orchestrate",
  "status": "running",
  "progress": 75,
  "duration": 5432,
  "data": {
    "success": true,
    "filesCreated": ["output/index.html"],
    "summary": "Created hello world page"
  }
}
```

## Status Values

- `running` - Sentinel is still executing
- `completed` - Sentinel finished successfully
- `failed` - Sentinel encountered an error
- `not_found` - Handle doesn't exist
