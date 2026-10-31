//! Cognition: the per-turn verbs of a persona's mind (admission, attention,
//! deliberation, tool execution, recall) and the faculties around them.

pub mod conversation;
pub mod extensions;
pub mod admission;
pub mod admission_persistence;
pub mod admission_state;
pub mod attention;
pub use continuum_engram::engram;
pub use continuum_engram::engram_graph;
pub use continuum_engram::home;
pub mod inbox_admission;
pub use continuum_rooms::mind_room;
pub use continuum_grounding::cached_source;
pub use continuum_engram::recall_metadata;
pub use continuum_engram::trace;
pub use continuum_rooms::card_facts;
pub use continuum_rooms::card_holder;
pub use continuum_grounding::grounding_invalidation;
pub use continuum_rooms::room_substrates;
pub use continuum_prompt::framing_echo;
pub use continuum_prompt::not_speech;
pub mod act_observe;
pub use continuum_vitals::directed_pending;
pub use continuum_vitals::grid_allocation;
pub mod channel_digest;
pub mod channel_digest_region;
pub mod channel_element;
pub mod channel_substrate;
pub use continuum_prompt::context_budget;
pub mod deferred_faculty;
pub mod deliberation_budget;
pub mod deliberation_parse;
pub mod deliberation_prompt;
pub mod dispatch_listener;
pub use continuum_vitals::activity_gate;
pub use continuum_vitals::turn_ingress;
pub mod faculty_pulse;
pub use continuum_vitals::vitals_pulse;
pub use continuum_engram::focus_policy;
pub mod generate_recipe;
pub mod generate_response;
pub(crate) mod generation_drop;
pub mod inference_session;
pub mod introspect_commands;
pub mod llm_deliberation_faculty;
pub(crate) mod context_streams;
pub mod tool_surface;
pub mod tool_head;
pub mod model_binding;
pub mod model_resolver;
/// Paging cursors over large data she is working through, kept in working memory.
pub mod open_views;
pub mod parroted_perception;
pub use continuum_vitals::pass_intent;
pub mod perception_facts;
pub mod persona_slot;
pub mod persona_tools;
pub mod persona_workspace;
pub use continuum_rooms::handoff;
pub use continuum_rooms::bounded_room_ledger;
pub use continuum_prompt::prompt_prefix;
pub use continuum_capture::prompt_capture;
pub mod rag_source_faculty;
pub mod rate_proposals;
pub mod recall_faculty;
pub mod recent_message;
pub use continuum_prompt::audience;
pub use continuum_engram::recall_ranker;
pub mod replay;
pub use continuum_rooms::room_roster;
pub use continuum_vitals::turn_ledger;
pub use continuum_prompt::response_validator;
pub mod self_repeat;
pub mod should_respond;
pub mod should_respond_module;
pub use continuum_prompt::token_budget;
pub mod tool_embedding;
pub mod tool_executor;
pub mod turn_batch;
pub mod validate_response;
pub mod vision_describe;
pub mod working_memory;
pub mod working_set;
pub mod workspace;
pub mod workspace_capture;
// Leaf modules peeled down from persona (they name no persona module): the capture
// fixture policy, the faculty ids persona's sources deliver under, the one budget
// allocator, text analysis, prompt assembly, redaction, the health ledger, presence
// glyphs, mind events, the room directory and the focus registry.
pub use continuum_capture::capture;
pub mod faculty_ids;
pub use continuum_vitals::health_ledger;
pub use continuum_vitals::mind_events;
pub use continuum_engram::focus;
pub use continuum_rooms::presence_glyph;
pub use continuum_prompt::prompt_assembly;
pub use continuum_grounding::rag_budget;
pub use continuum_engram::redaction;
pub mod resident_service;
pub use continuum_rooms::room_directory;

pub use model_resolver::*;
pub use persona_slot::PersonaSlot;
pub use response_validator::{clean_and_validate, is_hard_failure, ValidationOutcome};
pub use recent_message::RecentMessage;
pub use should_respond::*;
pub use tool_executor::{
    MediaItemLite, NativeBatchOutcome, ParsedToolBatch, PersonaMediaConfigLite,
    ToolExecutionContext, ToolExecutor, ToolInvocation, ToolOutcome,
};
pub use turn_batch::*;
