# Continuum REST API Documentation

<!-- ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking -->
<!-- ✅ CLEANED UP: Added modern session management documentation (2025-07-13) -->
<!-- ✅ CLEANED UP: Integrated X-Session-ID header standards (2025-07-13) -->

## Overview

The Continuum REST API provides HTTP endpoints for all system commands with proper session management. This API is designed for:

- **AI agents** and automation tools
- **curl** and direct HTTP clients  
- **Third-party integrations**
- **CLI tools** (ultra-thin clients)

## Base URL

```
http://localhost:9000/api
```

## Authentication & Session Management

### Session Flow

1. **Connect** to establish a session
2. **Use session** for subsequent commands
3. **Session persists** until system restart

### Automated Session Management

The API provides multiple session management approaches:

#### Option 1: Session Headers (Recommended for APIs)
```bash
# 1. Connect to get session ID
curl -X POST http://localhost:9000/api/commands/connect \
  -H "Content-Type: application/json" \
  -d '{"args":[]}'

# Response includes sessionId
{
  "sessionId": "development-shared-abc123",
  "version": "0.2.2745",
  "action": "joined_existing",
  ...
}

# 2. Use session ID in X-Session-ID header
curl -X POST http://localhost:9000/api/commands/screenshot \
  -H "Content-Type: application/json" \
  -H "X-Session-ID: development-shared-abc123" \
  -d '{"args":["--filename=test.png"]}'
```

#### Option 2: Authorization Bearer Token
```bash
# Use sessionId as Bearer token
curl -X POST http://localhost:9000/api/commands/screenshot \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer development-shared-abc123" \
  -d '{"args":["--filename=test.png"]}'
```

#### Option 3: Session Cookies (Future Implementation)
```bash
# Will automatically set session cookies on connect
curl -c cookies.txt -X POST http://localhost:9000/api/commands/connect \
  -H "Content-Type: application/json" \
  -d '{"args":[]}'

# Use cookies for subsequent requests
curl -b cookies.txt -X POST http://localhost:9000/api/commands/screenshot \
  -H "Content-Type: application/json" \
  -d '{"args":["--filename=test.png"]}'
```

## API Endpoints

### Core System

#### Health Check
```bash
GET /api/health
```

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2025-07-13T17:28:19.553Z",
  "daemons": ["renderer", "command-processor", "session-manager", ...],
  "connections": 1
}
```

#### Connect (Session Management)
```bash
POST /api/commands/connect
```

**Parameters:**
- `sessionType`: Type of session (development, persona, etc.) - Default: "development"
- `owner`: Session owner (user, system, etc.) - Default: "shared"
- `forceNew`: Force create new session - Default: false
- `focus`: Focus browser window - Default: false
- `killZombies`: Close zombie browser tabs - Default: false

**Example:**
```bash
curl -X POST http://localhost:9000/api/commands/connect \
  -H "Content-Type: application/json" \
  -d '{
    "args": [],
    "sessionType": "development",
    "owner": "shared",
    "forceNew": false
  }'
```

### Commands

All commands follow the same pattern:

```bash
POST /api/commands/{command-name}
```

**Request Format:**
```json
{
  "args": ["--param1=value1", "--param2=value2"],
  "param1": "value1",
  "param2": "value2"
}
```

**Session Required:** Most commands require a valid session (except `connect`)

#### Screenshot Command
```bash
POST /api/commands/screenshot
```

**Parameters:**
- `filename`: Output filename 
- `selector`: CSS selector to target - Default: "body"
- `format`: Image format (png, jpg, jpeg, webp) - Default: "png"
- `destination`: Where to save (file, bytes, both) - Default: "file"
- `animation`: Animation type (none, visible, animated) - Default: "none"

**Example:**
```bash
curl -X POST http://localhost:9000/api/commands/screenshot \
  -H "Content-Type: application/json" \
  -H "X-Session-ID: development-shared-abc123" \
  -d '{
    "args": ["--filename=homepage.png", "--selector=.main-content"],
    "filename": "homepage.png",
    "selector": ".main-content",
    "format": "png",
    "destination": "file"
  }'
```

#### Help Command
```bash
POST /api/commands/help
```

**Example:**
```bash
curl -X POST http://localhost:9000/api/commands/help \
  -H "Content-Type: application/json" \
  -H "X-Session-ID: development-shared-abc123" \
  -d '{"args":["screenshot"]}'
```

## Response Format

### Success Response
```json
{
  "success": true,
  "data": {
    // Command-specific data
  },
  "timestamp": "2025-07-13T17:28:19.553Z"
}
```

### Error Response
```json
{
  "success": false,
  "error": "Description of error",
  "timestamp": "2025-07-13T17:28:19.553Z"
}
```

## Error Handling

### Session Required Error
```bash
HTTP 500 Internal Server Error
```
```json
{
  "success": false,
  "error": "Screenshot capture failed: WebSocket communication failed: No session ID available for WebSocket communication",
  "timestamp": "2025-07-13T17:29:22.992Z"
}
```

**Solution:** Include session ID in request headers or connect first.

### Command Not Found Error
```bash
HTTP 404 Not Found
```
```json
{
  "success": false,
  "error": "API endpoint not found",
  "timestamp": "2025-07-13T17:29:22.992Z"
}
```

## Best Practices

### For AI Agents
1. **Connect once** at startup: `POST /api/commands/connect`
2. **Store session ID** from connect response  
3. **Use X-Session-ID header** for all subsequent requests
4. **Handle session expiration** by reconnecting

### For CLI Tools
1. **Ultra-thin design** - delegate all logic to API
2. **Session persistence** - store session ID locally
3. **Error handling** - clear messages for session issues
4. **Fail-fast validation** - validate parameters before API calls

### For Automation
1. **Bearer token approach** for stateless operations
2. **Health checks** before command execution
3. **Proper error handling** for network issues
4. **Retry logic** for transient failures

## Session Management Details

### What's Automated
- ✅ **Session extraction** from headers/cookies
- ✅ **Session validation** for protected endpoints
- ✅ **Session context** injection into commands
- ✅ **Multiple session formats** (header, bearer, cookie)

### What's Manual
- ❌ **Session ID storage** - clients must store and send
- ❌ **Session expiration handling** - clients must reconnect
- ❌ **Session cleanup** - handled by system restart

### Future Enhancements
- 🔮 **Session cookies** - automatic browser-style session management
- 🔮 **Session tokens** - JWT-based authentication
- 🔮 **Session persistence** - survive system restarts
- 🔮 **Multi-user sessions** - user-specific session isolation

## Command Discovery

Get list of available commands:
```bash
curl -X POST http://localhost:9000/api/commands/help \
  -H "Content-Type: application/json" \
  -H "X-Session-ID: development-shared-abc123" \
  -d '{"args":[]}'
```

Get command-specific help:
```bash
curl -X POST http://localhost:9000/api/commands/help \
  -H "Content-Type: application/json" \
  -H "X-Session-ID: development-shared-abc123" \
  -d '{"args":["screenshot"]}'
```

## Integration Examples

### Python Client
```python
import requests

class ContinuumClient:
    def __init__(self, base_url="http://localhost:9000"):
        self.base_url = base_url
        self.session_id = None
        
    def connect(self):
        response = requests.post(f"{self.base_url}/api/commands/connect", 
                               json={"args": []})
        data = response.json()
        self.session_id = data["sessionId"]
        return data
        
    def screenshot(self, filename="screenshot.png", selector="body"):
        headers = {"X-Session-ID": self.session_id}
        response = requests.post(f"{self.base_url}/api/commands/screenshot",
                               headers=headers,
                               json={"args": [f"--filename={filename}", f"--selector={selector}"]})
        return response.json()

# Usage
client = ContinuumClient()
client.connect()
result = client.screenshot("homepage.png", ".main-content")
```

### Node.js Client
```javascript
const axios = require('axios');

class ContinuumClient {
    constructor(baseURL = 'http://localhost:9000') {
        this.baseURL = baseURL;
        this.sessionId = null;
    }
    
    async connect() {
        const response = await axios.post(`${this.baseURL}/api/commands/connect`, {
            args: []
        });
        this.sessionId = response.data.sessionId;
        return response.data;
    }
    
    async screenshot(filename = 'screenshot.png', selector = 'body') {
        const response = await axios.post(`${this.baseURL}/api/commands/screenshot`, {
            args: [`--filename=${filename}`, `--selector=${selector}`]
        }, {
            headers: {
                'X-Session-ID': this.sessionId
            }
        });
        return response.data;
    }
}

// Usage
const client = new ContinuumClient();
await client.connect();
const result = await client.screenshot('homepage.png', '.main-content');
```

---

*This API is designed to be intuitive for both human developers and AI agents. Session management is automated where possible while maintaining flexibility for different client architectures.*