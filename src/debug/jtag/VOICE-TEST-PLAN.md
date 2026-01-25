# Voice AI Response System - Comprehensive Test Plan

## Test Coverage Goals
- **100% unit test coverage** for all new/modified code
- **100% integration test coverage** for all flows
- **Extreme attention to detail** - test edge cases, error conditions, boundary values
- **Improved modularity** - each component tested in isolation

---

## 1. Rust Unit Tests (continuum-core)

### 1.1 VoiceOrchestrator Unit Tests
**File**: `workers/continuum-core/src/voice/orchestrator.rs`

#### Test Cases:
- [x] `test_register_session` - Session registration
- [x] `test_broadcast_to_all_ais` - Broadcasts to all AI participants
- [ ] `test_no_ai_participants` - Returns empty vec when no AIs in session
- [ ] `test_speaker_excluded_from_broadcast` - Speaker not in responder list
- [ ] `test_unregistered_session` - Returns empty vec for unknown session
- [ ] `test_empty_transcript` - Handles empty transcript gracefully
- [ ] `test_multiple_sessions` - Multiple concurrent sessions isolated
- [ ] `test_session_unregister` - Cleanup after session ends
- [ ] `test_should_route_to_tts` - TTS routing logic (if still used)
- [ ] `test_clear_voice_responder` - Cleanup after response

**Coverage Target**: 100% of orchestrator.rs

### 1.2 IPC Layer Unit Tests
**File**: `workers/continuum-core/src/ipc/mod.rs`

#### Test Cases:
- [ ] `test_voice_on_utterance_request` - Deserializes request correctly
- [ ] `test_voice_on_utterance_response` - Response uses constant field name
- [ ] `test_voice_on_utterance_response_field_name` - Constant matches expected value
- [ ] `test_empty_responder_ids` - Returns empty array when no AIs
- [ ] `test_multiple_responder_ids` - Returns multiple UUIDs correctly
- [ ] `test_voice_register_session_request` - Session registration IPC
- [ ] `test_health_check` - Health check returns success
- [ ] `test_malformed_request` - Error handling for invalid JSON
- [ ] `test_lock_poisoning` - Error handling for mutex poisoning

**Coverage Target**: 100% of IPC voice-related code

### 1.3 CallServer Unit Tests
**File**: `workers/continuum-core/src/voice/call_server.rs`

#### Test Cases (after integration):
- [ ] `test_transcription_calls_orchestrator` - After STT, calls VoiceOrchestrator
- [ ] `test_orchestrator_result_emitted` - AI IDs emitted as events
- [ ] `test_empty_orchestrator_result` - Handles no AI participants
- [ ] `test_transcription_failure` - Graceful handling of STT failure
- [ ] `test_multiple_transcriptions_sequential` - Back-to-back transcriptions
- [ ] `test_concurrent_transcriptions` - Multiple participants talking simultaneously

**Coverage Target**: 100% of new orchestrator integration code

---

## 2. Rust Integration Tests

### 2.1 VoiceOrchestrator + IPC Integration
**File**: `workers/continuum-core/tests/voice_orchestrator_ipc.rs` (new file)

#### Test Cases:
- [ ] `test_ipc_voice_on_utterance_end_to_end` - Request → Orchestrator → Response
- [ ] `test_ipc_register_session_then_utterance` - Register, then process utterance
- [ ] `test_ipc_multiple_sessions_isolated` - Session isolation via IPC
- [ ] `test_ipc_responder_ids_field_constant` - Response field uses constant
- [ ] `test_ipc_broadcast_to_multiple_ais` - Multiple AIs via IPC

### 2.2 CallServer + VoiceOrchestrator Integration
**File**: `workers/continuum-core/tests/call_server_orchestrator.rs` (new file)

#### Test Cases:
- [ ] `test_transcription_to_orchestrator_flow` - STT → Orchestrator → Event emission
- [ ] `test_statement_broadcasts_to_all` - Non-questions broadcast
- [ ] `test_question_broadcasts_to_all` - Questions broadcast (no filtering)
- [ ] `test_no_ai_participants_no_events` - No events when no AIs
- [ ] `test_multiple_ai_participants` - All AIs receive events
- [ ] `test_speaker_not_in_responders` - Speaker excluded from broadcast

---

## 3. TypeScript Unit Tests

### 3.1 RustCoreIPC Bindings
**File**: `tests/unit/rust-core-ipc-voice.test.ts` (new file)

#### Test Cases:
- [ ] `test_voiceOnUtterance_returns_array` - Return type is string[]
- [ ] `test_voiceOnUtterance_uses_constant` - Uses VOICE_RESPONSE_FIELDS constant
- [ ] `test_voiceOnUtterance_empty_response` - Returns empty array on failure
- [ ] `test_voiceOnUtterance_multiple_ids` - Handles multiple responder IDs
- [ ] `test_ipc_field_names_match_rust` - TypeScript constants match Rust

### 3.2 VoiceOrchestratorRustBridge
**File**: `tests/unit/voice-orchestrator-rust-bridge.test.ts` (new file)

#### Test Cases:
- [ ] `test_onUtterance_returns_array` - Return type changed to UUID[]
- [ ] `test_onUtterance_not_connected` - Returns empty array when not connected
- [ ] `test_onUtterance_error_handling` - Returns empty array on error
- [ ] `test_onUtterance_performance_warning` - Logs warning if > 5ms
- [ ] `test_onUtterance_conversion_to_rust_format` - Event conversion correct

---

## 4. TypeScript Integration Tests

### 4.1 Voice Flow Integration (mocked Rust)
**File**: `tests/integration/voice-flow-mocked.test.ts` (new file)

#### Test Cases:
- [ ] `test_rust_bridge_to_typescript_flow` - Bridge → TypeScript event handling
- [ ] `test_multiple_ai_responders` - Multiple AIs receive events
- [ ] `test_broadcast_model_no_filtering` - All AIs get events (no arbiter)
- [ ] `test_empty_responder_array` - Handles empty array gracefully

### 4.2 Voice Flow Integration (real Rust - requires running server)
**File**: `tests/integration/voice-flow-e2e.test.ts` (new file)

#### Test Cases:
- [ ] `test_complete_voice_flow` - Audio → STT → Orchestrator → AI events → TTS
- [ ] `test_statement_response` - Statement triggers AI responses
- [ ] `test_question_response` - Question triggers AI responses
- [ ] `test_multiple_ais_respond` - Multiple AIs can respond
- [ ] `test_concurrent_utterances` - Multiple users talking

---

## 5. Test Implementation Priority

### Phase 1: Rust Unit Tests (Foundation)
1. Complete VoiceOrchestrator unit tests (100% coverage)
2. Complete IPC unit tests (100% coverage)
3. Verify all tests pass: `cargo test --package continuum-core`

### Phase 2: TypeScript Unit Tests (Bindings)
1. RustCoreIPC bindings unit tests
2. VoiceOrchestratorRustBridge unit tests
3. Verify all tests pass: `npx vitest tests/unit/`

### Phase 3: Rust Integration (CallServer)
1. Implement CallServer → VoiceOrchestrator integration
2. Write integration tests
3. Verify tests pass: `cargo test --package continuum-core --test call_server_orchestrator`

### Phase 4: TypeScript Integration (Mocked)
1. Write mocked integration tests
2. Verify tests pass without running server

### Phase 5: E2E Integration (Real System)
1. Deploy system
2. Run E2E tests with real Rust + TypeScript
3. Verify complete flow works

---

## 6. Test Data & Fixtures

### Standard Test UUIDs
```rust
// Rust
const TEST_SESSION_ID: &str = "00000000-0000-0000-0000-000000000001";
const TEST_SPEAKER_ID: &str = "00000000-0000-0000-0000-000000000002";
const TEST_AI_1_ID: &str = "00000000-0000-0000-0000-000000000003";
const TEST_AI_2_ID: &str = "00000000-0000-0000-0000-000000000004";
```

```typescript
// TypeScript
const TEST_IDS = {
  SESSION: '00000000-0000-0000-0000-000000000001' as UUID,
  SPEAKER: '00000000-0000-0000-0000-000000000002' as UUID,
  AI_1: '00000000-0000-0000-0000-000000000003' as UUID,
  AI_2: '00000000-0000-0000-0000-000000000004' as UUID,
};
```

### Standard Test Utterances
- **Statement**: "This is a statement, not a question"
- **Question**: "Can you hear me?"
- **Empty**: ""
- **Long**: "Lorem ipsum..." (500 chars)
- **Special chars**: "Hello @AI-Name, can you help?"

### Standard Test Participants
```rust
VoiceParticipant {
    user_id: TEST_AI_1_ID,
    display_name: "Helper AI",
    participant_type: SpeakerType::Persona,
    expertise: vec!["general".to_string()],
}
```

---

## 7. Success Criteria

### Unit Tests
- ✅ 100% code coverage for modified files
- ✅ All edge cases tested
- ✅ All error conditions tested
- ✅ All tests pass

### Integration Tests
- ✅ Complete flow tested end-to-end
- ✅ Multiple scenarios tested
- ✅ Concurrency tested
- ✅ All tests pass

### Code Quality
- ✅ No magic strings (all constants)
- ✅ No duplication
- ✅ Clear test names
- ✅ Well-documented test purposes

---

## 8. Running Tests

### Rust Tests
```bash
# All tests
cargo test --package continuum-core

# Specific module
cargo test --package continuum-core --lib voice::orchestrator

# Integration tests
cargo test --package continuum-core --test voice_orchestrator_ipc

# With output
cargo test --package continuum-core -- --nocapture

# Release mode (faster)
cargo test --package continuum-core --release
```

### TypeScript Tests
```bash
# All unit tests
npx vitest tests/unit/

# All integration tests
npx vitest tests/integration/

# Specific file
npx vitest tests/unit/rust-core-ipc-voice.test.ts

# With coverage
npx vitest --coverage

# Watch mode
npx vitest --watch
```

---

## 9. Test Metrics

Track these metrics for each test run:
- **Tests Passed**: X / Y
- **Code Coverage**: X%
- **Average Test Duration**: Xms
- **Slowest Tests**: List of tests > 100ms
- **Flaky Tests**: Tests that fail intermittently

---

## 10. Next Steps

1. ✅ Create this test plan
2. [ ] Implement Rust unit tests (Phase 1)
3. [ ] Implement TypeScript unit tests (Phase 2)
4. [ ] Implement CallServer integration (Phase 3)
5. [ ] Implement TypeScript integration tests (Phase 4)
6. [ ] Run E2E tests (Phase 5)
7. [ ] Verify 100% coverage
8. [ ] Deploy with confidence
