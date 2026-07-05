# collaboration/dm

Get or create a private room for a specific set of participants.

## Set Theory Approach

The room uniqueId is deterministic based on the sorted set of participants:
- `{A, B} == {B, A}` - same room regardless of who initiates
- Works with 2 participants (classic DM) or 3+ (group DM)

## Usage

### Create/Get DM with one person
```bash
# By user ID
./jtag collaboration/dm --participants="<user-id>"

# By uniqueId
./jtag collaboration/dm --participants="helper-ai"
```

### Create/Get group DM
```bash
./jtag collaboration/dm --participants='["helper-ai", "teacher-ai"]'
```

### With custom name
```bash
./jtag collaboration/dm --participants="helper-ai" --name="Project Discussion"
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `participants` | string \| string[] | Yes | Other participant(s) - ID or uniqueId. Current user is auto-added. |
| `name` | string | No | Custom room display name (can be set later) |

## Response

```typescript
{
  success: true,
  message: "Found existing DM: Joel & Helper AI",
  room: RoomEntity,
  roomId: "uuid-...",
  existed: true,  // false if newly created
  uniqueId: "dm-abc123def456",
  participantIds: ["uuid-1", "uuid-2"]
}
```

## How It Works

1. Current user is auto-detected (caller identity)
2. All participants are resolved (by ID or uniqueId)
3. Deterministic `uniqueId` is generated from sorted participant set
4. If room with that `uniqueId` exists, it's returned
5. Otherwise, new room is created with all participants as members

## Recipe

DM rooms use the `dm` recipe which provides:
- Private room settings
- One-on-one conversation pipeline
- No response gating (always respond when addressed)
