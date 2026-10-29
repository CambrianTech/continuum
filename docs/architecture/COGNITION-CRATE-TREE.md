# continuum-cognition as a tree of crates

Status: plan, 2026-10-29 (Joel: "Plus the cognition is divisible further. Crates are better as trees"). Owner: Fable; each cut is one PR, a pure move plus import paths; cognition re-exports each moved module under its old path, so callers change nothing. Survey on origin/canary b5a1527f6.

95,596 lines, 105 modules. One SCC of 26 modules (~55.5k lines, 58%): llm_deliberation_faculty, act_observe, working_memory, conversation, persona_workspace, workspace, tool_executor, admission_state, admission_persistence, inbox_admission, recall_faculty, rag_source_faculty, deliberation_*, context_streams, open_views, dispatch_listener, extensions, faculty_pulse, generation_drop, deferred_faculty, parroted_perception, perception_facts, resident_service, workspace_capture. The other ~40k lines are acyclic (45 leaf modules, 14 depending only on leaves).

## Back-edges and their fixes
| back-edge | evidence | fix |
|---|---|---|
| workspace -> llm_deliberation_faculty | workspace.rs:31 GenomeHandle, DecodingHandle, empty_genome, relaxed_decoding | move into model_binding (leaf) |
| workspace -> act_observe | EffectGate in cause.rs:130, edit.rs:732 | move effect_mark.rs (55 lines) into the workspace crate |
| workspace -> working_memory, tool_executor, admission_state, admission_persistence | ActingBody/EvalIsolation held by WorkspaceCycle | move WorkspaceCycle (+ those structs, ~975 lines) up into persona-workspace |
| workspace -> resident_service | capacity.rs:51 unpark | trait ParkListener owned by continuum-workspace |
| workspace(edit) <-> parroted_perception | | parroted_perception + self_repeat into the workspace crate |
| conversation -> workspace, deliberation_budget, rag_budget | conversation/workspace_turns.rs (668) | move that file up into turn-framing |
| working_memory <-> act_observe, faculty <-> act_observe | vocabulary types only | split act_observe: vocabulary (types, observation, disk_change, recency; ~1.57k) vs the settle loop |
| tool_executor -> persona_workspace | spill.rs:233 persisted_result_handles; command_executor.rs:114 forget_acting_context | trait ActingCustody owned by continuum-acts, installed by persona_workspace |
| act_observe(apply) -> persona_workspace | acting_root_in | same ActingCustody |

## The tree, bottom first
| # | crate | modules | ~lines | depends on |
|---|---|---|---|---|
| 1 | continuum-conversation | conversation minus workspace_turns; attention; channel_element/digest/digest_region/substrate | 5.5k | airc, base |
| 2 | continuum-rooms | card_holder, card_facts, handoff, room_directory/substrates/roster, mind_room, presence_glyph, bounded_room_ledger | 2.2k | airc, positron |
| 3 | continuum-vitals | health_ledger, turn_ledger, turn_ingress, grid_allocation, vitals_pulse, activity_gate, directed_pending, mind_events, pass_intent | 3.7k | engine, experience |
| 4 | continuum-prompt | prompt_assembly, framing_echo, not_speech, response_validator, token_budget, prompt_prefix, audience, context_budget | 4.5k | engine, text-analysis |
| 5 | continuum-tools | working_set, tool_head, tool_embedding, tool_surface, persona_tools | 4.0k | 4, tool_working_set, genome |
| 6 | continuum-grounding | rag_budget, cached_source, grounding_invalidation | 2.1k | airc |
| 7 | continuum-engram | engram, engram_graph, recall_metadata, recall_ranker, focus, focus_policy, trace, redaction, home | 4.4k | kernel |
| 8 | continuum-admission | admission, inbox_admission, admission_state, admission_persistence | 5.8k | 1,2,5,7 |
| 9 | continuum-capture | capture, capture_owner, prompt_capture, prompt_capture_store | 1.8k | 2 |
| 10 | continuum-response-verbs | types, shared_analysis, response_orchestrator, inference_session, should_respond, check_redundancy, generate_response, generate_recipe, rate_proposals, turn_batch, validate_response, model_resolver, vision_describe, commands/ | 11.4k | 5 |
| 11 | continuum-workspace | workspace core, effect_mark, model_binding + handle types, faculty_pulse, faculty_ids, deferred_faculty, parroted_perception, self_repeat | 4.9k | 1,3; owns ParkListener |
| 12 | continuum-turn-framing | deliberation_budget, workspace_turns, deliberation_parse | 2.9k | 4,6,11 |
| 13 | continuum-acts | act vocabulary, tool_executor, open_views, extensions, resident_service | 5.9k | 2,3,6,11; owns ActingCustody |
| 14 | continuum-working-memory | working_memory, context_streams, dispatch_listener | 5.5k | 2,4,5,12,13, context |
| 15 | continuum-recall | recall_faculty | 2.0k | 7,8,11,14 |
| 16 | continuum-deliberation | llm_deliberation_faculty, deliberation_prompt, perception_facts, generation_drop, rag_source_faculty | 15.7k | 3-6,9,11-14 |
| 17 | continuum-act-observe | settle loop, apply, perception, page_budget | 5.8k | 8,13,14,16 |
| 18 | continuum-persona-workspace | persona_workspace, WorkspaceCycle/ActingBody/EvalIsolation, should_respond_module, workspace_capture, replay, request_replay, introspect_commands | 7.7k | all; implements both traits |
| top | continuum-cognition | re-exports only, so crate::cognition::X paths keep working | <0.2k | all |

Cut order: 1-10 (acyclic, pure moves, parallel) -> 11 (after relocations + ParkListener) -> 12-13 (ActingCustody + act vocabulary split) -> 14-18.

## Glob leak
cognition lib.rs `pub use continuum_live::*` (-> engine -> kernel); academy re-exports cognition; mind re-exports academy::cognition + live. Cognition uses via the glob: ai 295, inference 161, identity 98, sdk_codegen 74, orm 35, paths 33, runtime 27, model_registry 20, routing 11, media 9, paging 6, resources 4; macros action_command/register_stateless_command/register_command. Child crates import explicitly (continuum_engine::{ai, inference, model_registry}, continuum_kernel::{identity, sdk_codegen, orm, paths, runtime, routing, paging, resources}, continuum_live::media, continuum_hands::code). memory, perception, rag, recipe have zero uses (genome one).
