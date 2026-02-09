//! FFI bindings for Node.js and Swift
//!
//! C-compatible functions for cross-language interop.
//! All functions include performance timing and logging.
//!
//! Architecture:
//! - Rust core owns all data (VoiceOrchestrator, PersonaInbox)
//! - FFI returns opaque pointers that Node.js/Swift holds
//! - Caller must free pointers via continuum_free()
//!
//! Performance:
//! - All FFI calls are timed and logged
//! - Timing thresholds: >10ms = warn, >1ms = info, <1ms = debug
use crate::voice::{VoiceOrchestrator, UtteranceEvent, VoiceParticipant};
use crate::persona::PersonaInbox;
use crate::logging::{init_logger, logger, TimingGuard};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use uuid::Uuid;
use std::ptr;

// ============================================================================
// Initialization
// ============================================================================

/// Initialize continuum-core and logger
///
/// @param logger_socket_path Path to logger worker Unix socket
/// @return 0 on success, -1 on error
/// # Safety
/// Caller must ensure logger_socket_path is a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn continuum_init(logger_socket_path: *const c_char) -> i32 {
    let _timer = TimingGuard::new("ffi", "continuum_init");

    if logger_socket_path.is_null() {
        eprintln!("❌ continuum_init: null logger_socket_path");
        return -1;
    }

    let socket_path = unsafe {
        match CStr::from_ptr(logger_socket_path).to_str() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("❌ continuum_init: invalid UTF-8: {e}");
                return -1;
            }
        }
    };

    match init_logger(socket_path) {
        Ok(_) => {
            logger().info("ffi", "init", "✅ Continuum core initialized");
            0
        }
        Err(e) => {
            eprintln!("❌ continuum_init: {e}");
            -1
        }
    }
}

// ============================================================================
// VoiceOrchestrator FFI
// ============================================================================

/// Create a new VoiceOrchestrator
///
/// @return Opaque pointer to VoiceOrchestrator (must call continuum_voice_free())
#[no_mangle]
pub extern "C" fn continuum_voice_create() -> *mut VoiceOrchestrator {
    let _timer = TimingGuard::new("ffi", "voice_create");

    let orchestrator = VoiceOrchestrator::new();
    let ptr = Box::into_raw(Box::new(orchestrator));

    logger().info("ffi", "voice", &format!("Created VoiceOrchestrator at {ptr:?}"));

    ptr
}

/// Free a VoiceOrchestrator
///
/// @param ptr Pointer returned from continuum_voice_create()
/// # Safety
/// Caller must ensure ptr was returned from continuum_voice_create().
#[no_mangle]
pub unsafe extern "C" fn continuum_voice_free(ptr: *mut VoiceOrchestrator) {
    let _timer = TimingGuard::new("ffi", "voice_free");

    if !ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(ptr);
        }
        logger().debug("ffi", "voice", &format!("Freed VoiceOrchestrator at {ptr:?}"));
    }
}

/// Register a voice session with participants
///
/// @param ptr VoiceOrchestrator pointer
/// @param session_id UUID string (hex format)
/// @param room_id UUID string (hex format)
/// @param participants_json JSON array of VoiceParticipant objects
/// @return 0 on success, -1 on error
/// # Safety
/// Caller must ensure all pointers are valid: ptr from continuum_voice_create(),
/// session_id/room_id/participants_json are null-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn continuum_voice_register_session(
    ptr: *mut VoiceOrchestrator,
    session_id: *const c_char,
    room_id: *const c_char,
    participants_json: *const c_char,
) -> i32 {
    let _timer = TimingGuard::new("ffi", "voice_register_session");

    if ptr.is_null() || session_id.is_null() || room_id.is_null() || participants_json.is_null() {
        logger().error("ffi", "voice", "voice_register_session: null pointer");
        return -1;
    }

    let orchestrator = unsafe { &mut *ptr };

    // Parse session_id
    let session_id_str = unsafe {
        match CStr::from_ptr(session_id).to_str() {
            Ok(s) => s,
            Err(e) => {
                logger().error("ffi", "voice", &format!("Invalid session_id UTF-8: {e}"));
                return -1;
            }
        }
    };

    let session_uuid = match Uuid::parse_str(session_id_str) {
        Ok(u) => u,
        Err(e) => {
            logger().error("ffi", "voice", &format!("Invalid session_id UUID: {e}"));
            return -1;
        }
    };

    // Parse room_id
    let room_id_str = unsafe {
        match CStr::from_ptr(room_id).to_str() {
            Ok(s) => s,
            Err(e) => {
                logger().error("ffi", "voice", &format!("Invalid room_id UTF-8: {e}"));
                return -1;
            }
        }
    };

    let room_uuid = match Uuid::parse_str(room_id_str) {
        Ok(u) => u,
        Err(e) => {
            logger().error("ffi", "voice", &format!("Invalid room_id UUID: {e}"));
            return -1;
        }
    };

    // Parse participants JSON
    let participants_str = unsafe {
        match CStr::from_ptr(participants_json).to_str() {
            Ok(s) => s,
            Err(e) => {
                logger().error("ffi", "voice", &format!("Invalid participants UTF-8: {e}"));
                return -1;
            }
        }
    };

    let participants: Vec<VoiceParticipant> = match serde_json::from_str(participants_str) {
        Ok(p) => p,
        Err(e) => {
            logger().error("ffi", "voice", &format!("Invalid participants JSON: {e}"));
            return -1;
        }
    };

    let participant_count = participants.len();
    orchestrator.register_session(session_uuid, room_uuid, participants);

    logger().info(
        "ffi",
        "voice",
        &format!("Registered session {session_uuid} with {participant_count} participants")
    );

    0
}

/// Process an utterance event
///
/// @param ptr VoiceOrchestrator pointer
/// @param event_json JSON UtteranceEvent object
/// @param out_responder_id Output buffer for responder UUID (37 bytes: 36 + null terminator)
/// @return 0 if responder selected, 1 if no responder, -1 on error
/// # Safety
/// Caller must ensure ptr is valid, event_json is null-terminated,
/// and out_responder_id has at least 1024 bytes allocated.
#[no_mangle]
pub unsafe extern "C" fn continuum_voice_on_utterance(
    ptr: *mut VoiceOrchestrator,
    event_json: *const c_char,
    out_responder_id: *mut c_char,
) -> i32 {
    let _timer = TimingGuard::new("ffi", "voice_on_utterance").with_threshold(10);

    if ptr.is_null() || event_json.is_null() || out_responder_id.is_null() {
        logger().error("ffi", "voice", "voice_on_utterance: null pointer");
        return -1;
    }

    let orchestrator = unsafe { &mut *ptr };

    // Parse event JSON
    let event_str = unsafe {
        match CStr::from_ptr(event_json).to_str() {
            Ok(s) => s,
            Err(e) => {
                logger().error("ffi", "voice", &format!("Invalid event UTF-8: {e}"));
                return -1;
            }
        }
    };

    let event: UtteranceEvent = match serde_json::from_str(event_str) {
        Ok(e) => e,
        Err(e) => {
            logger().error("ffi", "voice", &format!("Invalid event JSON: {e}"));
            return -1;
        }
    };

    // Process utterance - returns Vec of ALL AI participant IDs
    let responder_ids = orchestrator.on_utterance(event.clone());

    if responder_ids.is_empty() {
        logger().debug(
            "ffi",
            "voice",
            &format!("Utterance from {} → no AI participants", event.speaker_name)
        );
        return 1;
    }

    // Serialize Vec<Uuid> to JSON array
    let json_array = serde_json::to_string(&responder_ids).unwrap();
    let c_string = CString::new(json_array).unwrap();
    let bytes = c_string.as_bytes_with_nul();

    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out_responder_id as *mut u8, bytes.len());
    }

    logger().info(
        "ffi",
        "voice",
        &format!(
            "Utterance from {} → {} AI participants",
            event.speaker_name, responder_ids.len()
        )
    );

    0
}

/// Check if TTS should be routed to a session
///
/// @param ptr VoiceOrchestrator pointer
/// @param session_id UUID string
/// @param persona_id UUID string
/// @return 1 if should route, 0 if not, -1 on error
/// # Safety
/// Caller must ensure ptr is valid and session_id/persona_id are null-terminated.
#[no_mangle]
pub unsafe extern "C" fn continuum_voice_should_route_to_tts(
    ptr: *mut VoiceOrchestrator,
    session_id: *const c_char,
    persona_id: *const c_char,
) -> i32 {
    let _timer = TimingGuard::new("ffi", "voice_should_route_to_tts");

    if ptr.is_null() || session_id.is_null() || persona_id.is_null() {
        logger().error("ffi", "voice", "voice_should_route_to_tts: null pointer");
        return -1;
    }

    let orchestrator = unsafe { &*ptr };

    // Parse session_id
    let session_id_str = unsafe {
        match CStr::from_ptr(session_id).to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        }
    };

    let session_uuid = match Uuid::parse_str(session_id_str) {
        Ok(u) => u,
        Err(_) => return -1,
    };

    // Parse persona_id
    let persona_id_str = unsafe {
        match CStr::from_ptr(persona_id).to_str() {
            Ok(s) => s,
            Err(_) => return -1,
        }
    };

    let persona_uuid = match Uuid::parse_str(persona_id_str) {
        Ok(u) => u,
        Err(_) => return -1,
    };

    if orchestrator.should_route_to_tts(session_uuid, persona_uuid) {
        1
    } else {
        0
    }
}

// ============================================================================
// PersonaInbox FFI
// ============================================================================

/// Create a new PersonaInbox
///
/// @param persona_id UUID string
/// @return Opaque pointer to PersonaInbox (must call continuum_inbox_free())
/// # Safety
/// Caller must ensure persona_id is a null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn continuum_inbox_create(persona_id: *const c_char) -> *mut PersonaInbox {
    let _timer = TimingGuard::new("ffi", "inbox_create");

    if persona_id.is_null() {
        logger().error("ffi", "inbox", "inbox_create: null persona_id");
        return ptr::null_mut();
    }

    let persona_id_str = unsafe {
        match CStr::from_ptr(persona_id).to_str() {
            Ok(s) => s,
            Err(e) => {
                logger().error("ffi", "inbox", &format!("Invalid persona_id UTF-8: {e}"));
                return ptr::null_mut();
            }
        }
    };

    let persona_uuid = match Uuid::parse_str(persona_id_str) {
        Ok(u) => u,
        Err(e) => {
            logger().error("ffi", "inbox", &format!("Invalid persona_id UUID: {e}"));
            return ptr::null_mut();
        }
    };

    let inbox = PersonaInbox::new(persona_uuid);
    let ptr = Box::into_raw(Box::new(inbox));

    logger().info("ffi", "inbox", &format!("Created PersonaInbox for {persona_uuid}"));

    ptr
}

/// Free a PersonaInbox
///
/// @param ptr Pointer returned from continuum_inbox_create()
/// # Safety
/// Caller must ensure ptr was returned from continuum_inbox_create().
#[no_mangle]
pub unsafe extern "C" fn continuum_inbox_free(ptr: *mut PersonaInbox) {
    let _timer = TimingGuard::new("ffi", "inbox_free");

    if !ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(ptr);
        }
        logger().debug("ffi", "inbox", "Freed PersonaInbox");
    }
}

// ============================================================================
// Memory Management
// ============================================================================

/// Generic free function for opaque pointers
/// # Safety
/// Caller must ensure ptr was allocated by this library.
#[no_mangle]
pub unsafe extern "C" fn continuum_free(ptr: *mut ()) {
    if !ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(ptr);
        }
    }
}

// ============================================================================
// Health Check
// ============================================================================

/// Health check - verifies FFI is working
///
/// @return 1 if healthy, 0 if not
#[no_mangle]
pub extern "C" fn continuum_health_check() -> i32 {
    logger().debug("ffi", "health", "Health check called");
    1
}

/// Get performance statistics as JSON
///
/// @param category Category to get stats for (or null for all)
/// @return JSON string (caller must free with continuum_free_string())
/// # Safety
/// If category is not null, it must be a valid null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn continuum_get_stats(category: *const c_char) -> *mut c_char {
    let _timer = TimingGuard::new("ffi", "get_stats");

    let category_str = if category.is_null() {
        "all"
    } else {
        unsafe {
            CStr::from_ptr(category).to_str().unwrap_or("all")
        }
    };

    let stats = serde_json::json!({
        "category": category_str,
        "note": "Performance stats tracking not yet implemented"
    });

    let json = serde_json::to_string(&stats).unwrap();
    let c_string = CString::new(json).unwrap();

    c_string.into_raw()
}

/// Free a string returned from continuum_get_stats()
/// # Safety
/// Caller must ensure ptr was returned from continuum_get_stats().
#[no_mangle]
pub unsafe extern "C" fn continuum_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}
