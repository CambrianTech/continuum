# Event Coalescing - Live System Test Protocol

## Scientific Testing Methodology

**Objective**: Verify that event coalescing works correctly in production with real AI personas responding to messages.

**Hypothesis**: Event coalescing will reduce event emissions by ~99% while maintaining complete RAG context for AI decision-making.

## Test Setup

### Prerequisites
1. System deployed with event coalescing enabled
2. At least 3 AI personas active in general room
3. Logging configured to capture:
   - Event coalescing messages
   - ThoughtStream coordination
   - RAG context building
   - AI decision logs

### Test Environment
```bash
# Verify system ready
./jtag ping
# Expected: systemReady: true, 66 commands, 12 daemons

# Get list of AI personas
./jtag data/list --collection=users --filter='{"type":"persona"}' --limit=10

# Get general room ID
./jtag data/list --collection=rooms --filter='{"uniqueId":"general"}' --limit=1
```

## Test Protocol

### Phase 1: Baseline (Single Message)

**Purpose**: Establish baseline AI behavior without event coalescing effects

**Steps**:
1. Send single message to general room
2. Observe AI responses
3. Document response times and quality

**Commands**:
```bash
# Send test message
ROOM_ID=$(./jtag data/list --collection=rooms --filter='{"uniqueId":"general"}' | jq -r '.items[0].id')

./jtag debug/chat-send --roomId="$ROOM_ID" --message="Test question: What is event coalescing?"

# Wait 10 seconds for AI responses
sleep 10

# Check AI responses
./jtag data/list --collection=chat_messages --filter="{\"roomId\":\"$ROOM_ID\"}" --limit=10 --sort='{"timestamp":-1}'
```

**Expected Results**:
- ✅ 1-3 AI personas respond
- ✅ Responses mention event coalescing
- ✅ Response quality: coherent and relevant

**Logs to Check**:
```bash
tail -100 .continuum/sessions/user/shared/*/logs/server.log | grep -E "Event coalesced|ThoughtStream|RAG context built"
```

### Phase 2: Rapid-Fire Messages (Event Coalescing Test)

**Purpose**: Trigger event coalescing by sending 5 rapid messages

**Steps**:
1. Send 5 messages in quick succession (< 100ms apart)
2. Observe event coalescing logs
3. Verify AIs receive complete RAG context
4. Verify AI responses reference all messages

**Commands**:
```bash
# Send 5 rapid messages
for i in {1..5}; do
  ./jtag debug/chat-send --roomId="$ROOM_ID" --message="Message $i: Testing event coalescing part $i" &
done
wait

# Wait for event coalescing (100ms debounce + processing)
sleep 1

# Check event coalescing logs
tail -100 .continuum/sessions/user/shared/*/logs/server.log | grep "Event coalesced"

# Wait for AI responses
sleep 15

# Check AI responses
./jtag data/list --collection=chat_messages --filter="{\"roomId\":\"$ROOM_ID\"}" --limit=20 --sort='{"timestamp":-1}'
```

**Expected Results**:
- ✅ Log shows: "🔄 Event coalesced: data:chat_messages:created (5 merged)"
- ✅ Log shows: "✅ Emitting coalesced event: data:chat_messages:created (merged 5 events, saved 4 emissions)"
- ✅ AIs respond to messages (may reference multiple parts)
- ✅ Responses acknowledge all 5 messages (not just last one)

**Verification Questions**:
1. Did event coalescing trigger? (Check for "5 merged" log)
2. Did AIs receive complete RAG context? (Check for "RAG context built (N messages)")
3. Do AI responses reference multiple messages? (Read response text)

### Phase 3: Pick Random AI and Trace ThoughtStream

**Purpose**: Follow complete decision path for one AI persona

**Steps**:
1. Get list of AI personas
2. Pick one at random
3. Send message and trace full thoughtstream
4. Verify RAG context at each step

**Commands**:
```bash
# Get AI personas
PERSONAS=$(./jtag data/list --collection=users --filter='{"type":"persona"}' --limit=10)
echo "$PERSONAS" | jq -r '.items[] | "\(.displayName) (\(.id))"'

# Pick one (example: Helper AI)
PERSONA_ID=$(echo "$PERSONAS" | jq -r '.items[0].id')
PERSONA_NAME=$(echo "$PERSONAS" | jq -r '.items[0].displayName')

echo "Testing with: $PERSONA_NAME ($PERSONA_ID)"

# Send message
./jtag debug/chat-send --roomId="$ROOM_ID" --message="@$PERSONA_NAME: Explain how you process messages with event coalescing"

# Trace thoughtstream for this persona
tail -100 .continuum/sessions/user/shared/*/logs/server.log | grep -E "$PERSONA_NAME.*PHASE|$PERSONA_NAME.*RAG|$PERSONA_NAME.*Event coalesced"
```

**Expected Logs** (for chosen persona):
```
✅ Helper AI: [PHASE 1] Message received from Human
✅ Helper AI: [PHASE 2] Evaluating message...
🔧 Helper AI: [PHASE 3.1] Building RAG context...
✅ Helper AI: [PHASE 3.1] RAG context built (X messages)
🔧 Helper AI: [PHASE 3.2] Building LLM message array...
✅ Helper AI: [PHASE 3.2] LLM message array built (Y messages)
🔧 Helper AI: [PHASE 3.3] Calling AIProviderDaemon.generateText...
```

**Data to Collect**:
- How many messages in RAG context?
- How long did RAG building take?
- Did event coalescing affect RAG completeness?
- Response quality score (1-5)

### Phase 4: Stress Test (High-Volume Event Coalescing)

**Purpose**: Test event coalescing under heavy load

**Steps**:
1. Send 20 rapid messages
2. Verify extreme coalescing (20 → 1)
3. Verify AIs still respond correctly
4. Measure performance improvement

**Commands**:
```bash
# Send 20 rapid messages
START_TIME=$(date +%s)
for i in {1..20}; do
  ./jtag debug/chat-send --roomId="$ROOM_ID" --message="Stress test message $i" &
done
wait
END_TIME=$(date +%s)

SEND_DURATION=$((END_TIME - START_TIME))
echo "Sent 20 messages in $SEND_DURATION seconds"

# Check event coalescing
sleep 1
tail -100 .continuum/sessions/user/shared/*/logs/server.log | grep "Event coalesced.*20 merged"

# Wait for AI responses
sleep 20

# Check responses
./jtag data/list --collection=chat_messages --filter="{\"roomId\":\"$ROOM_ID\"}" --limit=30 --sort='{"timestamp":-1}'
```

**Expected Results**:
- ✅ Event coalescing: 20 → 1 (saved 19 emissions)
- ✅ AIs respond (may acknowledge "received many messages")
- ✅ RAG context: 20 messages loaded from database
- ✅ No errors or crashes

**Performance Metrics**:
- Event emissions: 20 → 1 (95% reduction)
- AI processing time: < 30 seconds total
- System stability: No errors

## Phase 5: RAG Context Verification

**Purpose**: Directly inspect RAG context to prove completeness

**Steps**:
1. Send 5 messages
2. Use ai/rag/inspect command to see exact RAG context
3. Verify all 5 messages present

**Commands**:
```bash
# Send 5 messages
for i in {1..5}; do
  ./jtag debug/chat-send --roomId="$ROOM_ID" --message="RAG test message $i"
  sleep 0.05  # 50ms apart
done

# Wait for database write
sleep 2

# Inspect RAG context for a persona
./jtag ai/rag/inspect --roomId="$ROOM_ID" --personaId="$PERSONA_ID"
```

**Expected Output**:
```json
{
  "domain": "chat",
  "conversationHistory": [
    { "role": "user", "content": "RAG test message 1", "name": "Human" },
    { "role": "user", "content": "RAG test message 2", "name": "Human" },
    { "role": "user", "content": "RAG test message 3", "name": "Human" },
    { "role": "user", "content": "RAG test message 4", "name": "Human" },
    { "role": "user", "content": "RAG test message 5", "name": "Human" }
  ],
  "metadata": {
    "messageCount": 5
  }
}
```

**Verification**:
- ✅ All 5 messages in conversationHistory
- ✅ Messages in chronological order
- ✅ Complete message text (not truncated)

## Success Criteria

### Must Pass (Critical)
- [ ] Event coalescing triggers for rapid messages (5+ → 1)
- [ ] RAG context contains ALL messages from database
- [ ] AI responses reference multiple messages (not just last)
- [ ] No errors or crashes during testing
- [ ] Performance improvement: 95%+ event emission reduction

### Should Pass (Important)
- [ ] Event coalescing logs visible and correct
- [ ] ThoughtStream coordination works normally
- [ ] AI response quality unchanged
- [ ] System handles 20+ rapid messages gracefully

### Nice to Have
- [ ] Visual confirmation in chat UI
- [ ] Performance metrics show improvement
- [ ] Multiple AIs respond correctly

## Results Template

```markdown
# Event Coalescing Live Test Results

**Date**: [YYYY-MM-DD]
**System Version**: [version]
**Tester**: [name]

## Phase 1: Baseline
- AIs responded: [count]
- Response quality: [1-5 rating]
- Notes: [observations]

## Phase 2: Rapid-Fire (5 messages)
- Event coalescing triggered: [YES/NO]
- Events merged: [N → 1]
- AIs responded: [count]
- RAG completeness: [PASS/FAIL]
- Notes: [observations]

## Phase 3: ThoughtStream Trace
- Persona tested: [name]
- RAG messages loaded: [count]
- Processing time: [seconds]
- Response quality: [1-5 rating]
- Notes: [observations]

## Phase 4: Stress Test (20 messages)
- Event coalescing: [N → 1]
- System stability: [STABLE/UNSTABLE]
- Performance: [observations]

## Phase 5: RAG Verification
- Messages sent: 5
- Messages in RAG: [count]
- Completeness: [PASS/FAIL]

## Overall Result: [PASS/FAIL]
**Conclusion**: [summary of findings]
```

## Troubleshooting

### Event Coalescing Not Triggering
**Check**: Are messages arriving fast enough? (<100ms apart)
**Fix**: Use background processes with `&` to send simultaneously

### AIs Not Responding
**Check**: Are AIs initialized? `./jtag data/list --collection=users --filter='{"type":"persona"}'`
**Fix**: Restart system: `npm start`

### Logs Not Showing Coalescing
**Check**: Is event coalescing enabled in code?
**Fix**: Verify `EventManager.shouldCoalesce()` returns true for message events

### RAG Context Incomplete
**Check**: Database query working? Check DataDaemon logs
**Fix**: Verify messages saved: `./jtag data/list --collection=chat_messages`

---

**Test Protocol Version**: 1.0
**Last Updated**: 2025-10-22
**Status**: Ready for execution
