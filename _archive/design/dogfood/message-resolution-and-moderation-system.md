# Message Resolution & Moderation System
**Implementation Date:** 2025-10-20
**Session:** Reply-to linking, message prioritization, and moderation foundation

---

## Problem Statement

User feedback from testing the AI coordination system:
> "they kind of need the ability to say which question they are responding to, if any... They're like just responding randomly now look."

Three issues identified:
1. **Context confusion**: AIs responding without clear indication of which question they're answering
2. **Dead horse beating**: AIs continuing to respond to old/resolved questions
3. **No quality control**: No mechanism for moderators to manage conversation flow or flag bad responses

---

## Phase 1: Implemented (2025-10-20)

### 1. Reply-To Database Linking ✅

**File Modified:** `system/user/server/PersonaUser.ts:1010`

```typescript
// Link AI response to trigger message
responseMessage.replyToId = originalMessage.id;
```

**Database Schema:** `system/data/entities/ChatMessageEntity.ts:144`
```typescript
@TextField({ nullable: true })
replyToId?: UUID;
```

**Verification:**
```bash
./jtag data/list --collection=chat_messages --limit=2 --orderBy='[{"field":"timestamp","direction":"desc"}]'
```

**Result:** AI responses now contain `replyToId` field pointing to the original question.

---

### 2. Message Resolution Mechanism ✅

**Purpose:** Prevent "beating a dead horse" - mark messages as resolved so AIs stop responding.

**Schema Changes:** `system/data/entities/ChatMessageEntity.ts:42-51`
```typescript
export interface MessageMetadata {
  source: 'user' | 'system' | 'bot' | 'webhook';
  deviceType?: string;
  clientVersion?: string;
  editHistory?: readonly EditHistoryEntry[];
  deliveryReceipts?: readonly DeliveryReceipt[];
  resolved?: boolean;      // NEW: Mark message as resolved
  resolvedBy?: UUID;       // NEW: Who marked it resolved (moderator)
  resolvedAt?: number;     // NEW: When it was marked resolved
}

@JsonField({ nullable: true })
metadata?: Partial<MessageMetadata>;
```

**Logic:** `system/user/server/PersonaUser.ts:365-369`
```typescript
// STEP 2: Skip resolved messages (moderator marked as no longer needing responses)
if (messageEntity.metadata?.resolved) {
  console.log(`⏭️ ${this.displayName}: Skipping resolved message from ${messageEntity.senderName}`);
  return;
}
```

**Usage:**
```bash
# Moderator marks message as resolved
./jtag data/update --collection=chat_messages --id=MESSAGE_ID --data='{"metadata":{"resolved":true,"resolvedBy":"MODERATOR_ID","resolvedAt":1729400000000}}'

# Moderator un-marks if still relevant
./jtag data/update --collection=chat_messages --id=MESSAGE_ID --data='{"metadata":{"resolved":false}}'
```

---

### 3. Age-Based Message Prioritization ✅

**Purpose:** Naturally prioritize newer messages over old ones.

**Implementation:** `system/user/server/PersonaUser.ts:1176-1190`

```typescript
// Apply age-based penalty (prioritize newer messages)
const messageAgeMinutes = (Date.now() - messageEntity.timestamp.getTime()) / (1000 * 60);
let agePenalty = 0;

if (messageAgeMinutes > 5) {
  // Messages 5-15 minutes old: Linear penalty from 0% to 30%
  // Messages 15+ minutes old: Capped at 30% penalty
  agePenalty = Math.min(0.30, (messageAgeMinutes - 5) / 10 * 0.30);
}

const adjustedConfidence = Math.max(0, result.confidence - agePenalty);
```

**Penalty Schedule:**
- 0-5 minutes old: No penalty (fresh messages)
- 5-15 minutes old: Linear 0% → 30% penalty
- 15+ minutes old: Capped at 30% penalty

**Log Output Example:**
```
🧵 Helper AI: Worker evaluated message abc123 -
  rawConfidence=0.85,
  agePenalty=0.15 (12.3min old),
  adjustedConfidence=0.70,
  threshold=0.50,
  shouldRespond=true
```

---

## Phase 2: Future Implementation

### 4. Recipe-Based Role Permissions

**Goal:** Define moderator roles and permissions in recipe files.

**Proposed Type Additions:** `system/recipes/shared/RecipeTypes.ts`

```typescript
export interface RolePermissions {
  // Message management
  canResolveMessages: boolean;
  canUnresolveMessages: boolean;
  canPinMessages: boolean;
  canDeleteMessages: boolean;

  // Participant management
  canMuteParticipants: boolean;
  canUnmuteParticipants: boolean;
  canRateResponses: boolean;

  // Room management
  canModifyRecipe: boolean;
  canInviteParticipants: boolean;
}

export interface RoleDefinition {
  roleName: string;
  displayName: string;
  permissions: RolePermissions;
  priority: number;  // For ThoughtStreamCoordinator (moderator > expert > participant)
}

// Add to RecipeDefinition
export interface RecipeDefinition {
  // ... existing fields ...
  roles?: RoleDefinition[];  // NEW: Define roles and their permissions
}
```

**Example Recipe with Roles:** `system/recipes/general-chat.json`
```json
{
  "uniqueId": "general-chat",
  "name": "General Chat (Human-Focused)",
  "roles": [
    {
      "roleName": "moderator",
      "displayName": "Moderator",
      "permissions": {
        "canResolveMessages": true,
        "canUnresolveMessages": true,
        "canMuteParticipants": true,
        "canRateResponses": true,
        "canPinMessages": true,
        "canDeleteMessages": false
      },
      "priority": 100
    },
    {
      "roleName": "expert",
      "displayName": "Expert",
      "permissions": {
        "canResolveMessages": false,
        "canRateResponses": true
      },
      "priority": 75
    },
    {
      "roleName": "participant",
      "displayName": "Participant",
      "permissions": {
        "canResolveMessages": false,
        "canRateResponses": true
      },
      "priority": 50
    }
  ]
}
```

**Benefits:**
- Multiple moderators coordinate via shared permissions
- Different rooms have different governance models
- Recipe defines who can mark messages as resolved
- Priority levels integrate with ThoughtStreamCoordinator

---

### 5. Moderator UI Controls

**Goal:** Widget controls for moderators to manage conversation quality.

**Proposed Commands:**

```bash
# Message resolution
./jtag chat/resolve-message --messageId=UUID --moderatorId=UUID
./jtag chat/unresolve-message --messageId=UUID --moderatorId=UUID

# AI moderation
./jtag chat/mute-participant --userId=UUID --roomId=UUID --durationMinutes=60
./jtag chat/unmute-participant --userId=UUID --roomId=UUID

# Response rating
./jtag chat/rate-response --messageId=UUID --rating=helpful|unhelpful|harmful
```

**Widget Features:**
- **Message actions menu** (right-click or long-press):
  - ✅ Mark as resolved
  - ❌ Mark as unresolved
  - ⭐ Rate response (helpful/unhelpful/harmful)
  - 🚫 Mute sender (temporary timeout)
  - 📌 Pin message

- **Moderator dashboard widget:**
  - Recent AI decisions (via `./jtag ai/report`)
  - Flagged messages (low confidence, reported)
  - AI performance metrics
  - Resolution queue (unresolved questions)

---

### 6. AI Quality Control & Safety

**Moderator Use Cases:**

**1. Detect Gibberish/Abuse**
- AI posts nonsense → Moderator marks `resolved: true`
- All AIs skip that message (prevents cascade)
- Moderator rates response as `harmful`

**2. Review AI Decision Logs**
```bash
./jtag ai/logs --filterPersona="Problematic AI"
# See: What triggered it, reasoning, confidence levels
```

**3. Check AI Performance**
```bash
./jtag ai/report --personaName="Problematic AI"
# See: Response rate, confidence distribution, error rate
```

**4. Temporary Muting**
```typescript
// Mute AI for 1 hour
messageEntity.metadata = {
  mutedUntil: Date.now() + 3600000,
  mutedBy: moderatorId,
  muteReason: "Low quality responses"
};
```

**5. Rating-Based Demotion**
- Track moderator ratings per AI
- AIs with low ratings → Lower priority in ThoughtStreamCoordinator
- Automatic feedback loop: Bad ratings → Genome adjustments → Better behavior

---

### 7. AI Moderators (Long-term)

**Vision:** AI moderators monitoring conversation quality 24/7.

**Capabilities:**
- Pattern detection (spam, abuse, low-quality responses)
- Flag suspicious messages for human review
- Auto-resolve obvious spam
- Learn from human moderator decisions

**Transparent Equality:**
- AI moderators have same tools as humans
- All moderation actions logged (accountability)
- Human oversight for controversial decisions
- Community can vote on moderator effectiveness

---

## Technical Architecture

### Data Flow: Message → Response → Resolution

```
1. Human posts question
   ↓
2. AI evaluates via Worker Thread
   ↓ (rawConfidence calculated)
3. Apply age-based penalty (if message > 5min old)
   ↓ (adjustedConfidence = rawConfidence - agePenalty)
4. ThoughtStreamCoordinator (priority + recency fairness)
   ↓ (if highest confidence & turn available)
5. AI generates response with replyToId
   ↓
6. Response stored in database
   ↓
7. Moderator reviews (optional)
   ↓ (if resolved)
8. Moderator marks message.metadata.resolved = true
   ↓
9. Future AIs skip this message (no more responses)
```

### Integration Points

**1. Reply-To Linking**
- `PersonaUser.respondToMessage()` → Sets `replyToId`
- `ChatMessageEntity.replyToId` → Database field
- **Future:** Chat widget displays thread relationships

**2. Message Resolution**
- `PersonaUser.handleChatMessage()` → Checks `metadata.resolved`
- `ChatMessageEntity.metadata` → JSON field with resolution data
- **Future:** Moderator UI commands to set/unset

**3. Age-Based Prioritization**
- `PersonaUser.shouldRespondFastGating()` → Applies age penalty
- Integrates with existing confidence threshold system
- **Future:** Configurable penalty curve per recipe

**4. Recipe System**
- Current: Defines conversation patterns and AI behavior
- **Future:** Defines roles, permissions, moderation rules
- **Integration:** RoomEntity.recipeId → RecipeEntity.roles → User.role → RolePermissions

---

## Testing Strategy

### Unit Tests
- ✅ Reply-to field populated correctly
- ✅ Resolved messages skipped by AIs
- ✅ Age penalty calculated correctly (0-5min, 5-15min, 15+min)
- ✅ Confidence adjustment respects threshold

### Integration Tests
```bash
# 1. Test reply-to linking
./jtag debug/chat-send --roomId=ROOM_ID --message="Test question 1"
# Wait for AI response
# Check replyToId field in database

# 2. Test message resolution
./jtag debug/chat-send --roomId=ROOM_ID --message="Test question 2"
# Wait for first AI response
# Mark message as resolved
./jtag data/update --collection=chat_messages --id=MESSAGE_ID --data='{"metadata":{"resolved":true}}'
# Send another message to trigger evaluation
# Verify AIs skip the resolved message

# 3. Test age-based prioritization
# Create message with old timestamp
# Verify age penalty applied in logs
# Verify lower confidence results in fewer responses

# 4. Test coordination still works
./jtag debug/chat-send --roomId=ROOM_ID --message="Multi-AI test"
./jtag ai/report
# Verify: High spam prevention rate (60-90%)
# Verify: Only 1-2 AIs respond (not all 13)
```

---

## Deployment Checklist

- [x] Schema changes (ChatMessageEntity.metadata, replyToId)
- [x] PersonaUser logic (skip resolved, age penalty)
- [x] TypeScript compilation
- [x] Database migration (nullable fields, no breaking changes)
- [ ] Integration tests passed
- [ ] Manual testing with real messages
- [ ] AI decision logs verified
- [ ] Documentation complete
- [ ] Git commit with descriptive message

---

## Future Enhancements (Phase 3+)

### Short-term (Next Sprint)
1. **Moderator commands** - Implement `chat/resolve-message`, `chat/mute-participant`
2. **Widget UI controls** - Add message action menu (resolve, rate, mute)
3. **AI decision dashboard** - Widget showing recent decisions, flags, metrics

### Medium-term (Next Month)
1. **Recipe role system** - Implement role-based permissions
2. **Rating system** - Track moderator ratings per AI, use for priority
3. **Auto-resolution** - Mark messages as resolved after X responses or Y time

### Long-term (Next Quarter)
1. **AI moderators** - Train specialized "Moderator AI" personas
2. **Genome feedback** - Use ratings to improve AI behavior via LoRA
3. **Community governance** - Vote on moderation policies, moderator effectiveness

---

## Transparent Equality Implications

**Problem:** Traditional systems have opaque AI moderation with no accountability.

**Our Solution:**
- ✅ All moderation actions logged (who, what, when, why)
- ✅ AI moderators use same tools as humans (no special backdoors)
- ✅ Decision logs visible to all participants (`./jtag ai/logs`)
- ✅ Community can review moderator effectiveness
- ✅ Recipe-based governance (rooms self-govern via transparent rules)

**Result:** Moderation is transparent, accountable, and community-driven.

---

## Performance Considerations

**Age-Based Penalty Overhead:**
- Calculation: `(Date.now() - timestamp) / 60000` → Negligible (microseconds)
- Impact: Runs once per AI evaluation, already in Worker Thread
- No database queries added

**Resolved Message Check:**
- Early return: Happens before RAG building (saves LLM call)
- Impact: Positive (fewer unnecessary evaluations)

**Reply-To Field:**
- Storage: UUID (36 bytes) per message
- Index: Not needed (only for display, not filtering)
- Impact: Negligible

---

## Related Documentation

- `CLAUDE.md` - Universal Cognition Equation (E=mc² for AI)
- `system/recipes/README.md` - Recipe System Overview
- `system/conversation/server/ThoughtStreamCoordinator.ts` - RTOS-inspired coordination
- `commands/ai/report/README.md` - AI performance metrics
- `design/dogfood/transparent-equality-css-debugging-2025-10-16.md` - Dogfood session example

---

## Version History

- **v1.0.3450** (2025-10-20): Initial implementation
  - Reply-to linking
  - Message resolution mechanism
  - Age-based prioritization
  - Documentation complete
