# Voice AI Response Architecture Plan

## Current State (What Works)
1. ✅ Rust WebSocket broadcasts transcriptions to browser
2. ✅ Browser relays transcriptions to server
3. ✅ Server emits `voice:transcription` events
4. ✅ PersonaUser subscribes to events and enqueues to inbox
5. ✅ Autonomous loop processes inbox (works for chat already)

## The Missing Piece: Response Routing

**Problem**: PersonaUser generates response, but WHERE does it go?
- Chat messages → ChatWidget via Commands.execute('collaboration/chat/send')
- Voice transcriptions → Should go to TTS → Voice call (NOT chat)

**Current Response Flow** (broken for voice):
```
PersonaUser.processInboxMessage()
  → evaluateAndRespond()
    → postResponse(roomId, text)
      → Commands.execute('collaboration/chat/send')  # WRONG for voice!
        → Message appears in ChatWidget, NOT in voice call
```

**Correct Response Flow** (needed):
```
PersonaUser.processInboxMessage()
  → Check sourceModality
    → If 'voice': Route to TTS → voice call
    → If 'text': Route to chat widget
```

## Solution Architecture

### 1. Response Router (NEW)
**File**: `system/user/server/modules/PersonaResponseRouter.ts`

```typescript
class PersonaResponseRouter {
  async routeResponse(message: InboxMessage, responseText: string): Promise<void> {
    if (message.sourceModality === 'voice') {
      // Route to voice call via TTS
      await this.sendVoiceResponse(message.voiceSessionId!, responseText);
    } else {
      // Route to chat widget
      await this.sendChatResponse(message.roomId, responseText);
    }
  }

  private async sendVoiceResponse(callSessionId: UUID, text: string): Promise<void> {
    // Call TTS to generate audio
    // Send audio to call server
  }

  private async sendChatResponse(roomId: UUID, text: string): Promise<void> {
    await Commands.execute('collaboration/chat/send', { roomId, message: text });
  }
}
```

### 2. TTS Integration
**File**: `commands/voice/tts/generate/`

New command to generate TTS audio and send to call:

```typescript
Commands.execute('voice/tts/generate', {
  callSessionId: UUID,
  text: string,
  speakerId: UUID,
  speakerName: string
});
```

This command:
1. Calls continuum-core TTS (Piper/Kokoro)
2. Gets audio samples
3. Sends to call server (via IPC or WebSocket)
4. Call server mixes audio into call

### 3. LiveWidget Participant List
**Problem**: Only human speaker shows as active participant

**Fix**: When AI responds via voice, they should appear in participant list:
- Add AI avatar/icon
- Show "speaking" indicator when TTS active
- Show when AI is listening (joined but not speaking)

### 4. AI Call Lifecycle

**When transcription arrives**:
```
PersonaUser.handleVoiceTranscription()
  1. Check if already in call (track activeCallSessions)
  2. If not, mark as "listening" to this call
  3. Enqueue transcription to inbox
  4. Autonomous loop processes
  5. If decides to respond:
     - Generate response text
     - Route via PersonaResponseRouter (checks sourceModality)
     - TTS generates audio
     - Audio sent to call
     - LiveWidget shows AI as speaking
```

**When to leave call**:
- After N minutes of silence
- When human leaves
- When explicitly dismissed

## Implementation Steps

### Phase 1: Response Routing (30min)
1. Create `PersonaResponseRouter.ts`
2. Update `PersonaUser.postResponse()` to use router
3. Add check for `sourceModality === 'voice'`
4. Log instead of sending (stub for now)

### Phase 2: TTS Command (1h)
1. Generate `voice/tts/generate` command
2. Implement server: call continuum-core TTS via IPC
3. Return audio samples
4. Test with simple phrase

### Phase 3: Call Audio Integration (1h)
1. Send TTS audio to call server (via continuum-core)
2. Mix into call (mixer already handles this)
3. Test end-to-end: speak → AI responds via voice

### Phase 4: LiveWidget UI (30min)
1. Add AI participants to call participant list
2. Show speaking indicator
3. Test UI updates

## Files to Modify

| File | Change |
|------|--------|
| `system/user/server/modules/PersonaResponseRouter.ts` | NEW - Route responses |
| `system/user/server/PersonaUser.ts` | Use router in postResponse() |
| `commands/voice/tts/generate/` | NEW - TTS command |
| `workers/continuum-core/src/ipc/mod.rs` | Add TTS IPC endpoint |
| `widgets/live/LiveWidget.ts` | Show AI participants |

## Testing Plan

1. **Manual Test**:
   ```bash
   npm start
   # Join call in browser
   # Speak: "Helper AI, what do you think?"
   # Expect: Helper AI responds via voice (TTS)
   # Verify: Audio plays in call
   # Verify: Helper AI shown in participant list
   ```

2. **Integration Test**:
   ```typescript
   // Test response routing
   const voiceMessage: InboxMessage = {
     sourceModality: 'voice',
     voiceSessionId: 'test-call-123',
     content: 'Hello AI'
   };
   await responseRouter.routeResponse(voiceMessage, 'Hi there!');
   // Should call TTS, not chat send
   ```

## Critical Insight

**The inbox already handles multi-modal input** (chat, code, games, sensors).
**Voice is just another input modality**.
**The ONLY difference is response routing** - where the output goes.

This is why `sourceModality` and `voiceSessionId` exist in `InboxMessage` - they tell PersonaUser HOW to respond.

## Why This Failed Before

I focused on:
- ❌ Getting transcriptions INTO inbox (this was easy, already done)
- ❌ Event subscriptions (also easy, already done)

I IGNORED:
- ❌ Getting responses OUT via correct channel (the hard part!)
- ❌ UI showing AI presence in call
- ❌ TTS integration with call server

**Root cause**: Treating voice as special case instead of just another response route.
