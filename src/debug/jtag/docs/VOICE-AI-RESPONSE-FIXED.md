# Voice AI Response - What Was Fixed

## The Problem

AIs were NOT responding to voice transcriptions because:

1. **VoiceOrchestrator existed** and was receiving transcriptions ✅
2. **Arbiter was selecting responders** (but only for questions/direct mentions) ✅
3. **🚨 CRITICAL BUG: After selecting a responder, nothing sent them the message!** ❌

Line 262 in VoiceOrchestrator.ts literally said:
```typescript
// TODO: Implement proper voice inbox routing through event system
```

## The Architecture (How It Was Supposed to Work)

```
1. Browser captures speech → Whisper STT (Rust)
2. Rust broadcasts transcription to WebSocket clients
3. Browser relays to server via collaboration/live/transcription command
4. Server emits voice:transcription event
5. VoiceOrchestrator receives event
6. Arbiter selects ONE responder based on:
   - Direct mention ("Helper AI, what do you think?")
   - Topic relevance (expertise match)
   - Round-robin for questions
7. 🚨 MISSING: Send inbox message to selected persona
8. PersonaUser processes from inbox
9. Generates response
10. Routes to TTS (via VoiceOrchestrator)
```

## What I Fixed

### 1. Added Directed Event Emission
**File**: `system/voice/server/VoiceOrchestrator.ts:260-272`

**BEFORE** (broken):
```typescript
console.log(`🎙️ VoiceOrchestrator: ${responder.displayName} selected to respond via voice`);

// TODO: Implement proper voice inbox routing through event system
// (nothing happens here!)

this.trackVoiceResponder(sessionId, responder.userId);
```

**AFTER** (fixed):
```typescript
console.log(`🎙️ VoiceOrchestrator: ${responder.displayName} selected to respond via voice`);

// Emit directed event FOR THE SELECTED RESPONDER ONLY
Events.emit('voice:transcription:directed', {
  sessionId: event.sessionId,
  speakerId: event.speakerId,
  speakerName: event.speakerName,
  transcript: event.transcript,
  confidence: event.confidence,
  language: 'en',
  timestamp: event.timestamp,
  targetPersonaId: responder.userId  // ONLY this persona responds
});

this.trackVoiceResponder(sessionId, responder.userId);
```

### 2. PersonaUser Subscribes to Directed Events
**File**: `system/user/server/PersonaUser.ts:578-590`

**BEFORE** (wrong - subscribed to ALL transcriptions):
```typescript
// Was subscribing to voice:transcription (broadcasts to everyone)
Events.subscribe('voice:transcription', async (data) => {
  // All personas received all transcriptions (spam!)
});
```

**AFTER** (correct - only receives when selected):
```typescript
// Subscribe to DIRECTED events (only when arbiter selects this persona)
Events.subscribe('voice:transcription:directed', async (data) => {
  // Only process if directed at THIS persona
  if (data.targetPersonaId === this.id) {
    await this.handleVoiceTranscription(data);
  }
});
```

### 3. Added Voice Transcription Handler
**File**: `system/user/server/PersonaUser.ts:935-1015`

NEW method that:
1. Ignores own transcriptions
2. Deduplicates
3. Calculates priority (boosted for voice)
4. Enqueues to inbox with `sourceModality: 'voice'` and `voiceSessionId`
5. Records in consciousness timeline

### 4. Removed Debug Spam
**Files**: `widgets/live/LiveWidget.ts`, `widgets/live/AudioStreamClient.ts`

Removed all the debug logs:
- ❌ `[STEP 8]`, `[STEP 9]` logs
- ❌ `🔍 DEBUG:` logs
- ❌ `[CAPTION]` logs
- ❌ `🌐 BROWSER:` logs

## How to Test

### Test 1: Direct Mention (Should Work Now)
```
1. npm start (wait 90s)
2. Open browser, join voice call
3. Speak: "Helper AI, what do you think about TypeScript?"
4. Expected: Helper AI responds via TTS
```

### Test 2: Question (Should Work - Arbiter Selects Round-Robin)
```
1. Speak: "What's the best way to handle errors?"
2. Expected: One AI responds (round-robin selection)
```

### Test 3: Statement (Won't Respond - By Design)
```
1. Speak: "The weather is nice today"
2. Expected: No AI response (arbiter rejects statements to prevent spam)
```

## Arbiter Logic (When AIs Respond)

**Composite Arbiter Priority**:
1. **Direct mention** - highest priority
   - "Helper AI, ..."
   - "@helper-ai ..."

2. **Topic relevance** - matches expertise
   - Looks for keywords in AI's expertise field

3. **Round-robin for questions** - takes turns
   - Only if utterance has '?' or starts with what/how/why/can/could

4. **Statements ignored** - prevents spam
   - No response to casual conversation

## What Still Needs Work

### Phase 1: Response Routing to TTS ❌
PersonaUser generates response but needs to route to TTS:
- Check `sourceModality === 'voice'`
- Call `VoiceOrchestrator.onPersonaResponse()`
- Route through AIAudioBridge to call server

**File to modify**: `system/user/server/modules/PersonaResponseGenerator.ts`

### Phase 2: LiveWidget Participant List ❌
Show AI participants in call UI:
- Add AI avatars
- Show "speaking" indicator when TTS active
- Show "listening" state

**File to modify**: `widgets/live/LiveWidget.ts`

### Phase 3: Arbiter Tuning ⚠️
Current arbiter is very conservative (only questions/mentions).
May want to add:
- Sentiment detection (respond to frustration)
- Context awareness (respond after long silence)
- Personality modes (some AIs more chatty than others)

## Logs to Watch

**Browser console**:
```
🎙️ Helper AI: Subscribed to voice:transcription:directed events
🎙️ Helper AI: Received DIRECTED voice transcription
📨 Helper AI: Enqueued voice transcription (priority=0.75, ...)
```

**Server logs** (npm-start.log):
```
[STEP 10] 🎙️ VoiceOrchestrator RECEIVED event: "Helper AI, what..."
🎙️ Arbiter: Selected Helper AI (directed)
🎙️ VoiceOrchestrator: Helper AI selected to respond via voice
```

## Key Architectural Insights

1. **Voice is a modality, not a domain**
   - Inbox already handles multi-domain (chat, code, games, etc.)
   - Voice just adds `sourceModality: 'voice'` metadata

2. **Arbitration prevents spam**
   - Without arbiter, ALL AIs would respond to EVERY utterance
   - Arbiter selects ONE responder per utterance

3. **Event-driven routing**
   - No direct PersonaInbox access
   - VoiceOrchestrator emits events
   - PersonaUser subscribes and enqueues
   - Clean separation of concerns

## Testing Checklist

- [ ] Deploy completes without errors
- [ ] Join voice call in browser
- [ ] Speak direct mention: "Helper AI, hello"
- [ ] Check browser logs for "Received DIRECTED voice transcription"
- [ ] Check server logs for arbiter selection
- [ ] Verify inbox enqueue happens
- [ ] (Phase 2) Verify AI responds via TTS
- [ ] (Phase 2) Verify AI appears in participant list
