# Room Mute Feature Design

## Problem
AI personas are too active in chat rooms. Need a way to pause AI responses per-room without:
- Stopping the entire system
- Breaking anything
- Making it easy to forget why AIs aren't responding

## Core Concept
**Room-level mute property** that pauses AI responses in specific rooms while keeping the system running.

---

## Data Model

### RoomEntity Extension
```typescript
interface RoomEntity extends BaseEntity {
  // ... existing fields

  /** If true, AI personas will not respond in this room */
  aiMuted: boolean;

  /** When AI was muted (for tracking) */
  aiMutedAt?: number;

  /** Who muted AI (userId) */
  aiMutedBy?: UUID;
}
```

**Default**: `aiMuted: false` (AIs active)

---

## UI Design

### 1. Room Header Controls
**Location**: Top-right of chat widget, next to room name

**Visual States**:
```
🔊 AI Active     (unmuted - green icon)
🔇 AI Muted      (muted - red icon with strikethrough)
```

**Interaction**:
- Click icon to toggle mute state
- Tooltip on hover: "Mute AI responses" / "Unmute AI responses"
- Visual feedback: Icon color changes + brief toast notification

**Example**:
```
┌─────────────────────────────────────────────┐
│ 📱 General Room          🔊 AI Active    ⚙️ │
└─────────────────────────────────────────────┘
```

When muted:
```
┌─────────────────────────────────────────────┐
│ 📱 General Room          🔇 AI Muted     ⚙️ │
└─────────────────────────────────────────────┘
```

### 2. Rooms List Indicator
**Location**: In room list sidebar, next to room name

**Visual**:
```
📱 General Room               [200 msgs]
📱 Academy     🔇              [45 msgs]   <- Muted room
📱 Debug Room                  [12 msgs]
```

**Purpose**: At-a-glance visibility of which rooms have AI muted

### 3. System Status Bar (Optional)
**Location**: Bottom of chat widget

**When any room is muted**:
```
⚠️  AI muted in 2 rooms: Academy, Test Room  [View] [Unmute All]
```

---

## Behavior

### When Room is Muted

**PersonaUser behavior**:
1. Check `room.aiMuted` before responding
2. If true, skip response entirely
3. Still process messages (for learning), just don't reply
4. Log: `"AI muted in room {roomName}, skipping response"`

**What still works**:
- Humans can chat normally
- AI can read messages (for training data)
- AI autonomous loop continues (just skips muted rooms)
- Other rooms work normally

**What stops**:
- AI posting messages in that room
- AI responding to @mentions in that room

### Edge Cases

**1. AI already typing when muted**
- Cancel in-progress response
- Show notification: "AI response cancelled (room muted)"

**2. Mute all rooms**
- AIs become read-only observers
- Still process inbox (for other tasks)
- Autonomous loop continues (just no chat responses)

**3. Unmuting**
- AIs resume normal behavior
- No backlog processing (don't reply to old messages)
- Start fresh from unmute point

---

## Implementation

### Phase 1: Data Layer (Priority 1)
1. Add `aiMuted` field to RoomEntity schema
2. Migration to add field to existing rooms (default: false)
3. Update room CRUD operations to include field

### Phase 2: Backend Logic (Priority 2)
1. Update PersonaUser.ts to check `room.aiMuted`
2. Skip response if muted
3. Add logging for debugging

```typescript
// In PersonaUser.serviceInbox()
async shouldRespondToMessage(message: ChatMessageEntity, room: RoomEntity): Promise<boolean> {
  // Check if room has AI muted
  if (room.aiMuted) {
    this.logger.debug(`AI muted in room ${room.name}, skipping response`);
    return false;
  }

  // ... existing logic
  return true;
}
```

### Phase 3: Commands (Priority 3)
```bash
# Mute AI in a room
./jtag room/mute --roomId="uuid"

# Unmute AI in a room
./jtag room/unmute --roomId="uuid"

# Check mute status
./jtag room/status --roomId="uuid"

# Mute all rooms
./jtag room/mute --all

# Unmute all rooms
./jtag room/unmute --all
```

### Phase 4: UI Controls (Priority 4)
1. Add mute icon to room header
2. Add click handler to toggle mute
3. Update room list to show mute indicator
4. Add toast notifications for state changes

**Files to modify**:
- `widgets/chat/room-header/room-header-widget.ts` - Add mute button
- `widgets/chat/rooms-list/rooms-list-widget.ts` - Add mute indicator
- `widgets/chat/chat-widget.css` - Style mute icons

---

## User Experience Flow

### Muting AI
1. User clicks 🔊 icon in room header
2. Icon changes to 🔇 with animation
3. Toast: "AI muted in General Room"
4. AI stops responding immediately
5. Room list shows 🔇 indicator

### Unmuting AI
1. User clicks 🔇 icon in room header
2. Icon changes to 🔊 with animation
3. Toast: "AI unmuted in General Room"
4. AI resumes normal behavior
5. Room list removes 🔇 indicator

### Visual Feedback
```css
.room-mute-button {
  cursor: pointer;
  transition: all 0.2s ease;
}

.room-mute-button.muted {
  color: var(--error-color);
  opacity: 0.8;
}

.room-mute-button:hover {
  transform: scale(1.1);
}
```

---

## Testing Strategy

### Unit Tests
```typescript
describe('Room Mute', () => {
  it('should not respond when room is muted', async () => {
    const room = { aiMuted: true };
    const shouldRespond = await persona.shouldRespondToMessage(msg, room);
    expect(shouldRespond).toBe(false);
  });

  it('should respond when room is unmuted', async () => {
    const room = { aiMuted: false };
    const shouldRespond = await persona.shouldRespondToMessage(msg, room);
    expect(shouldRespond).toBe(true);
  });
});
```

### Integration Tests
1. Create room
2. Send message (AI responds)
3. Mute room
4. Send message (AI does NOT respond)
5. Unmute room
6. Send message (AI responds)

### Manual Testing Checklist
- [ ] Mute icon appears in room header
- [ ] Click mute icon → AI stops responding
- [ ] Click unmute icon → AI resumes responding
- [ ] Mute indicator shows in room list
- [ ] Toast notifications appear
- [ ] Multiple rooms can be muted independently
- [ ] Mute persists across page refresh
- [ ] Command line tools work

---

## Future Enhancements (Post-MVP)

### 1. Scheduled Muting
```bash
# Mute for 1 hour
./jtag room/mute --roomId="uuid" --duration=1h

# Mute until specific time
./jtag room/mute --roomId="uuid" --until="2025-01-15T14:00:00Z"
```

### 2. Per-Persona Muting
Instead of muting all AIs, mute specific personas:
```typescript
interface RoomEntity {
  aiMuted: boolean;           // Mute all AIs
  mutedPersonas: UUID[];      // Mute specific AIs
}
```

### 3. Mute Reasons
Track why room was muted:
```typescript
interface RoomEntity {
  aiMuted: boolean;
  aiMuteReason?: 'user_request' | 'too_active' | 'testing' | 'maintenance';
}
```

### 4. Auto-Mute Rules
Automatically mute when:
- AI response rate > threshold
- Room activity > threshold
- Specific time of day

---

## Migration Plan

### Step 1: Add Field (No Breaking Changes)
```typescript
// Add field with default value
ALTER TABLE rooms ADD COLUMN aiMuted BOOLEAN DEFAULT false;
```

### Step 2: Update Code (Backward Compatible)
```typescript
// Gracefully handle missing field
const isMuted = room.aiMuted ?? false;
```

### Step 3: Deploy Backend First
- Backend checks field
- Falls back to false if missing
- No UI changes yet

### Step 4: Deploy UI
- Add mute controls
- Test in production
- Monitor for issues

### Step 5: Rollout
- Enable for all users
- Document feature
- Gather feedback

---

## Open Questions

1. **Should mute persist forever or have a timeout?**
   - Proposal: Persist forever, add optional timeout in Phase 2

2. **Should we notify AIs they're muted?**
   - Proposal: No notification, just silent skip (logs only)

3. **What about @mentions when muted?**
   - Proposal: Still muted (explicit mute overrides mentions)
   - Alternative: Allow @mentions to break through

4. **Should we track mute analytics?**
   - Proposal: Yes - track mute events for understanding usage

---

## Success Metrics

**Phase 1 Success**:
- [ ] Field added to RoomEntity
- [ ] Migration runs successfully
- [ ] No breaking changes

**Phase 2 Success**:
- [ ] AI stops responding in muted rooms
- [ ] AI continues responding in unmuted rooms
- [ ] No crashes or errors

**Phase 3 Success**:
- [ ] Commands work correctly
- [ ] Mute state persists
- [ ] Can mute/unmute multiple rooms

**Phase 4 Success**:
- [ ] UI controls work
- [ ] Visual indicators clear
- [ ] User feedback positive

**Overall Success**:
- Users can control AI activity per-room
- No confusion about system state
- No accidental system breakage
- Clear visibility of mute status

---

## Priority: Phase 1 & 2 First

Focus on getting the data model and backend logic working first. UI can come later once the foundation is solid.

**Next Steps**:
1. Review this design
2. Get approval
3. Start with Phase 1 (data layer)
4. Test Phase 2 (backend logic) with CLI
5. Add Phase 4 (UI) when backend proven
