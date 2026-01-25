# IPC Event Bridge Design - The Last Mile

## The Problem

**User warning**: "Rust gets stuck in its own enclave and becomes useless"

The data daemon tried to emit events from Rust and failed (see commented-out code in `DataDaemonServer.ts:249-344`). Attempting the same for voice will fail.

## ❌ WRONG APPROACH: Rust Emits Events Directly

```rust
// ❌ This is what FAILED in data daemon work
for ai_id in responder_ids {
    // Try to emit event from Rust → TypeScript Events system
    rust_ipc_emit("voice:transcription:directed", event_data)?;
    // Result: "Rust gets stuck in its own enclave"
}
```

**Why this fails:**
- Rust worker is isolated process
- TypeScript Events.emit() is in-process pub/sub
- No good bridge between isolated Rust → TypeScript event bus
- Data daemon attempted this and it became "useless"

## ✅ CORRECT APPROACH: Follow CRUD Pattern

### The CRUD Pattern (Already Works)

```typescript
// commands/data/create/server/DataCreateServerCommand.ts
async execute(params: DataCreateParams): Promise<DataCreateResult> {
  // 1. Rust computes (via DataDaemon → Rust storage)
  const entity = await DataDaemon.store(collection, params.data);

  // 2. TypeScript emits (in-process, works perfectly)
  const eventName = BaseEntity.getEventName(collection, 'created');
  await Events.emit(eventName, entity, this.context, this.commander);

  return { success: true, data: entity };
}
```

**Pattern**:
1. Rust does computation (concurrent, fast)
2. Returns data to TypeScript
3. TypeScript emits events (in-process, no bridge needed)

### Apply to Voice (The Solution)

```typescript
// system/voice/server/VoiceWebSocketHandler.ts (MODIFY)

case 'Transcription':
  const utteranceEvent = { /* ... */ };

  // 1. Rust computes responder IDs (ALREADY WORKS - 2µs!)
  const responderIds = await getVoiceOrchestrator().onUtterance(utteranceEvent);
  //     ↑ This calls Rust via IPC, returns UUID[]

  // 2. TypeScript emits events (NEW CODE - follow CRUD pattern)
  for (const aiId of responderIds) {
    const eventName = 'voice:transcription:directed';
    const eventData = {
      sessionId: utteranceEvent.sessionId,
      speakerId: utteranceEvent.speakerId,
      speakerName: utteranceEvent.speakerName,
      transcript: utteranceEvent.transcript,
      confidence: utteranceEvent.confidence,
      targetPersonaId: aiId,  // Directed to this AI
      timestamp: utteranceEvent.timestamp,
    };

    // Emit to TypeScript event bus (PersonaUser subscribes to this)
    await Events.emit(eventName, eventData, this.context, this.commander);

    console.log(`[STEP 8] 📤 Emitted voice event to AI: ${aiId}`);
  }
  break;
```

## Implementation

### File: `system/voice/server/VoiceWebSocketHandler.ts`

**Location 1: Line ~256** (Audio path)
```typescript
// BEFORE (current):
await getVoiceOrchestrator().onUtterance(utteranceEvent);

// AFTER (add event emission):
const responderIds = await getVoiceOrchestrator().onUtterance(utteranceEvent);
for (const aiId of responderIds) {
  await Events.emit('voice:transcription:directed', {
    sessionId: utteranceEvent.sessionId,
    speakerId: utteranceEvent.speakerId,
    speakerName: utteranceEvent.speakerName,
    transcript: utteranceEvent.transcript,
    confidence: utteranceEvent.confidence,
    targetPersonaId: aiId,
    timestamp: utteranceEvent.timestamp,
  }, this.context, this.commander);
}
```

**Location 2: Line ~365** (Transcription event path)
```typescript
// BEFORE (current):
await getVoiceOrchestrator().onUtterance(utteranceEvent);
console.log(`[STEP 10] 🎙️ VoiceOrchestrator RECEIVED event`);

// AFTER (add event emission):
const responderIds = await getVoiceOrchestrator().onUtterance(utteranceEvent);
console.log(`[STEP 10] 🎙️ VoiceOrchestrator RECEIVED event → ${responderIds.length} AIs`);

for (const aiId of responderIds) {
  await Events.emit('voice:transcription:directed', {
    sessionId: utteranceEvent.sessionId,
    speakerId: utteranceEvent.speakerId,
    speakerName: utteranceEvent.speakerName,
    transcript: utteranceEvent.transcript,
    confidence: utteranceEvent.confidence,
    targetPersonaId: aiId,
    timestamp: utteranceEvent.timestamp,
  }, this.context, this.commander);

  console.log(`[STEP 11] 📤 Emitted voice event to AI: ${aiId.slice(0, 8)}`);
}
```

### Event Subscription (PersonaUser)

PersonaUser instances should subscribe to `voice:transcription:directed`:

```typescript
// system/user/server/PersonaUser.ts (or wherever PersonaUser subscribes)

Events.subscribe('voice:transcription:directed', async (eventData) => {
  // Only process if directed to this persona
  if (eventData.targetPersonaId === this.entity.id) {
    console.log(`🎙️ ${this.entity.displayName}: Received voice transcription from ${eventData.speakerName}`);

    // Add to inbox for processing
    await this.inbox.enqueue({
      type: 'voice-transcription',
      priority: 0.8,  // High priority for voice
      data: eventData,
    });
  }
});
```

## Why This Works

### 1. No Rust → TypeScript Event Bridge Needed ✅
- Rust just returns data (Vec<Uuid>)
- TypeScript receives data via IPC (already works)
- TypeScript emits events (in-process, proven pattern)

### 2. Follows Existing CRUD Pattern ✅
- Same pattern as data/create, data/update, data/delete
- Rust computes → TypeScript emits
- No "stuck in enclave" problem

### 3. Minimal Changes ✅
- Rust code: ALREADY COMPLETE (returns responder IDs)
- TypeScript: Add 10 lines in VoiceWebSocketHandler
- PersonaUser: Subscribe to event (standard pattern)

### 4. Testable ✅
- Can test Rust separately (already done - 76 tests pass)
- Can test TypeScript event emission (standard Events.emit test)
- Can test PersonaUser subscription (standard pattern)

## Performance Impact

**Rust computation**: 2µs (already measured)

**TypeScript event emission**: ~50µs per AI
- Events.emit() is in-process function call
- No IPC, no serialization, no socket
- Negligible overhead

**Total for 5 AIs**: 2µs + (5 × 50µs) = ~250µs

**Still well under 1ms target.**

## Testing Strategy

### 1. Unit Test: VoiceWebSocketHandler Event Emission
```typescript
// Test that responder IDs are emitted as events
it('should emit voice:transcription:directed for each responder', async () => {
  const mockOrchestrator = {
    onUtterance: vi.fn().mockResolvedValue([ai1Id, ai2Id])
  };

  const emitSpy = vi.spyOn(Events, 'emit');

  await handler.handleTranscription(utteranceEvent);

  expect(emitSpy).toHaveBeenCalledTimes(2);
  expect(emitSpy).toHaveBeenCalledWith('voice:transcription:directed',
    expect.objectContaining({ targetPersonaId: ai1Id }), ...);
});
```

### 2. Integration Test: PersonaUser Receives Event
```typescript
// Test that PersonaUser receives and processes voice event
it('should process voice transcription event', async () => {
  const persona = await PersonaUser.create({ displayName: 'Helper AI' });

  await Events.emit('voice:transcription:directed', {
    targetPersonaId: persona.entity.id,
    transcript: 'Test utterance',
    // ...
  });

  // Verify persona inbox has the task
  const tasks = await persona.inbox.peek(1);
  expect(tasks[0].type).toBe('voice-transcription');
});
```

### 3. End-to-End Test: Full Voice Flow
```typescript
// Test complete flow: audio → transcription → orchestrator → events → AI
it('should complete full voice response flow', async () => {
  // 1. Send audio to VoiceWebSocketHandler
  // 2. Wait for transcription
  // 3. Verify orchestrator called
  // 4. Verify events emitted
  // 5. Verify PersonaUser received event
  // 6. Verify AI generated response
});
```

## Deployment Strategy

### Phase 1: Add Event Emission (TypeScript only)
1. Modify VoiceWebSocketHandler to emit events
2. Write unit tests
3. Deploy (no Rust changes needed)
4. Verify events are emitted (check logs)

### Phase 2: PersonaUser Subscription
1. Add subscription to `voice:transcription:directed`
2. Write integration tests
3. Deploy
4. Verify PersonaUser receives events

### Phase 3: Full Integration
1. Test end-to-end: voice → AI response
2. Verify TTS playback works
3. Performance profiling
4. Production ready

## Summary

**The key insight**: Don't fight the architecture. Rust is great at computation, TypeScript is great at events. Let each do what it's good at.

**Rust**: Compute responder IDs (2µs, concurrent, tested) ✅
**TypeScript**: Emit events (in-process, proven pattern) ✅
**PersonaUser**: Subscribe and process (standard pattern) ✅

**No IPC event bridge needed. No "stuck in enclave" problem.**

This is the CRUD pattern applied to voice. It works.
