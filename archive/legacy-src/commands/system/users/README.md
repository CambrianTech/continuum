# Users Command

System user management command that provides real user data from database/shared storage.

## Purpose

The Users Command serves as the backend for the UniversalUserSystem, providing a centralized API for user management across the Continuum platform. It supports humans, personas, AI models, and AI systems as first-class user types.

## Usage

```bash
# Get all users
continuum users

# Get only online users
continuum users --status=online

# Get all personas
continuum users --type=persona

# Get online users only (exclude offline)
continuum users --includeOffline=false
```

## User Types

- **human**: Real human users
- **persona**: AI personas with specific roles/capabilities
- **ai-model**: AI models (Claude, GPT, etc.)
- **ai-system**: System AI components (monitoring, diagnostics, etc.)

## Response Format

```json
{
  "success": true,
  "message": "Found 6 users",
  "data": {
    "users": [
      {
        "id": "current-user",
        "name": "User",
        "type": "human",
        "avatar": "👤",
        "status": "online",
        "capabilities": ["all"],
        "isClickable": false
      }
    ],
    "total": 6,
    "filters": {
      "type": "all",
      "status": "all",
      "includeOffline": true
    }
  }
}
```

## Integration

Used by:
- UniversalUserSystem.ts for UI population
- Chat widgets for user selection
- Academy system for persona management
- System monitoring for user activity tracking

## Future Enhancements

- Database integration for persistent user storage
- Real-time user status updates
- User permissions and role management
- User activity logging and analytics