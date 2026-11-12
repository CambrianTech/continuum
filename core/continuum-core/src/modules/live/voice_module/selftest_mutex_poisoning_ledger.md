## Card a6cfe59d — "Investigate mutex poisoning in voice/selftest" Ledger

### Known Facts (file:range → what it showed)
- `voice_module.rs` / `voice_response_field_responder_ids.rs`: VoiceState uses `parking_lot::Mutex<HashSet<String>>` for active_sessions and `parking_lot::Mutex<HashMap<String, String>>` for legacy_call_aliases — no poisoning risk
- `selftest.rs`: No mutex locks or PoisonError usage; calls synthesize_speech_async, call_manager.join_call/push_audio/leave_call, transcribe_speech_async
- `call_server.rs` / `call_manager.rs`: CallManager uses tokio::sync RwLock throughout — no std::sync Mutex, no poisoning risk
- `tts_service.rs` / `stt_service.rs`: No mutex usage; delegates to tts:: and stt:: modules
- `tts/mod.rs`: TTS_REGISTRY is `OnceCell<Arc<RwLock<TTSRegistry>>>` (parking_lot) — no poisoning risk
- `stt/mod.rs`: STT_REGISTRY is `OnceCell<Arc<RwLock<STTRegistry>>>` (parking_lot) — no poisoning risk
- `avatar/render_loop.rs:28,30,40,54,68`: `SLOT_POOL` is static std::sync Mutex<VecDeque<u8>> with `.lock().unwrap()` in allocate_slot/release_slot/reset_slot_pool
- `avatar/render_loop.rs:263-267,385,416,455`: `ACTIVE_RENDERERS` is static std::sync Mutex<HashMap<String, ActiveRenderer>> with `.lock().unwrap()` in spawn_renderer_loop
- `bevy_renderer/api.rs:59,66,72,100,109,123`: BEVY_SYSTEM uses RwLock<Option<Arc<BevyAvatarSystem>>> accessed via `.read().unwrap()` / `.write().unwrap()`
- `bevy_renderer/api.rs:133,205,253,258,262,267`: BevyAvatarSystem.identity_to_slot is std::sync Mutex<HashMap<String,u8>> with `.lock().unwrap()` in register_identity/unregister_identity/identity_to_slot_map/send_by_identity
- `inference/llama_server/process.rs:214,234,248,290,325,329,414`: child process management uses std::sync Mutex with `.lock().unwrap()` — poison means prior panic mutating the owned child

### Hypotheses
1. **Hypothesis: Poisoning from inference layer (llama.cpp/Kokoro/Moonshine)**
   - Claim: The TTS/STT services call into C library backends (kokoro, moonshine) that use internal std::sync Mutex with unwrap(); a prior panic in those libraries poisoned the lock and selftest hits it later
   - Evidence for: Selftest calls synthesize_speech_async → kokoro TTS; transcribe_speech_async → moonshine STT. Both are C library bindings that may have global state mutexes
   - Evidence against: Haven't directly inspected Kokoro/Moonshine source code
   - Test: Search kokoro and moonshine modules for std::sync Mutex with .unwrap()

2. **Hypothesis: Poisoning from shared engine/inference state**
   - Claim: A prior operation (not in selftest) panics while holding a lock in llama_server/process.rs or similar; selftest later acquires that same lock and gets poisoned
   - Evidence for: llama_server has many `.lock().unwrap()` on child process management locks that are shared across modules via Arc
   - Evidence against: The poisoning error would likely mention "child" or "process" in the backtrace, not just generic "Mutex poisoned"
   - Test: Check if selftest acquires any llama_server locks (unlikely but worth verifying)

3. **Hypothesis: Issue was fixed in a later commit**
   - Claim: The poisoning was caused by code that existed at core 8d1282271 but has since been changed or removed
   - Evidence for: Card says "observed on BigMama at core 8d1282271" — platform-specific and version-specific; the live avatar render_loop std::sync Mutex usage may have been added after that commit
   - Evidence against: The card is still open, suggesting the issue persists or wasn't fully resolved
   - Test: Compare current code with 8d1282271 to see if any std::sync Mutex patterns changed

4. **Hypothesis: Poisoning from avatar render loop (unlikely path)**
   - Claim: A previous call/avatar session panicked while holding SLOT_POOL or ACTIVE_RENDERERS; selftest indirectly triggers a code path that acquires one of these locks
   - Evidence for: These ARE std::sync Mutex with .unwrap() in the live module; if any background task panics, they become poisoned
   - Evidence against: Selftest doesn't call spawn_renderer_loop, allocate_bevy_slot, create_renderer, or any bevy_renderer function
   - Test: Grep for any indirect path from selftest to avatar/bevy initialization

### Unknown
The exact source of the "Mutex poisoned" panic during voice selftest — whether it's in TTS/STT C library backends (kokoro/moonshine), inference layer shared state, or an indirect path through avatar rendering that I haven't traced yet.

### Next Test
Search kokoro and moonshine modules for std::sync Mutex with .unwrap() to check if the poisoning comes from TTS/STT C library backends.
