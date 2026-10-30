# Typed boundaries: strings are the enemy

**Status:** design, 2026-10-30 (Joel: "Strings are the enemy"; "Every contains is probably suspect"; "We have
to do every api like this"). Owner of the contract: Fable. Same shape as ONE-BUDGET.md: an inventory, one
contract, a ratchet that drives the counts to zero, and slices with owners.

## Why

A string standing in for a type is re-parsed by every reader. Writer and reader drift apart, a typo compiles,
and a failure is silent. Today: a monitor grepping for `Codex:` missed a whole design review;
`should_respond` decides by `.contains("would respond")`; working memory re-parses its own rendered text with
`split_once("; room ")`.

Inventory of canary production code, 2026-10-30 (regex counts over ~344k lines, about ±5%):

| Shape | Count |
|---|---|
| Choosing behavior by a name (`match x.as_str()`, string arms, `== "lit"`, prefix routing) | ~1,300 |
| Untyped JSON field access (`v["f"]`, `.get("f")`, `.as_*` chains on `Value`) | ~1,800 |
| Ids typed `String`, plus `Uuid::parse_str` round trips | ~1,070 |
| Kinds and states as `String` (`kind`, `status`, `role`, `provider`, `topic`) | 341 |
| String searches with a literal: ~440 in parsers of an external format, ~520 decisions | 961 |

## The contract

1. **Text exists only at the edge.** Raw text (a file, a CLI's output, a model's reply, a wire frame) is parsed
   once, by the one parser for that format, into a type. Nothing after the parser holds the text or searches it.
2. **Every boundary is typed:**
   - commands are an enum of verbs, each carrying a params struct (serde, ts-rs), never `(&str, Value)`;
   - events carry a signed class and a typed payload (airc's EventClass);
   - ids are typed newtypes over `Uuid`/`u128` (`PeerId`, `RoomId` and `EventId` exist; add `PersonaId`,
     `CallId`, `LeaseId` and the others), rendered and parsed only through `airc_core::shown_id`;
   - kinds and states are enums.
3. **A rendered string is never read back.** What the system renders for a mind or a human is output; the
   typed value it was rendered from is the source of truth.
4. **Delivery and dispatch select on types** (headers, enum variants), never on text: the same rule as "never
   scan the body".
5. **No fallback parse.** An unknown variant is a typed refusal, never a default.

## Ratchet

`source-hygiene::typed_boundaries`, built on #5609's machinery (counts can't rise, a touched file ends clean,
no exemption markers):
- `string_dispatch`: name matches and literal comparisons that choose behavior;
- `untyped_json`: field access on `serde_json::Value` outside the codec of its type;
- `string_ids`: `*_id: String`/`&str` and `Uuid::parse_str` outside `shown_id` and the edge parsers;
- `string_kinds`: `kind`/`status`/`state`/`role`/`topic` as `String`;
- `text_search`: `contains`/`starts_with`/`ends_with`/`find`/`split_once`/`strip_*` with a literal.

External-format parsers are allowed by owner path, each named in the rule: the GGUF shard name parser, the
VRM bone names, the launchd plist parser, the swe-bench log parser, the tool-call text parser, `.env`, and so
on. Each must produce a type at its return.

## Slices (worst boundary first)

| # | Boundary | Owner |
|---|---|---|
| T1 | `ServiceModule::handle_command(&str, Value)`: commands become an enum with params structs (`CommandUri` gains a typed path); 97 modules and 106 arms migrate | Codex (kernel), each module's owner migrates its verbs |
| T2 | `Params` / `.get("k")`: replaced by T1's typed params; `Params` deleted | with T1 |
| T3 | `MessageBus::publish(&str, Value)`: typed events, routing by class, not by name prefix | Codex |
| T4 | airc envelopes: signed EventClass, typed command and event payloads, sender-resolved typed mentions | Codex, Cormac (in progress) |
| T5 | Ids: typed newtypes through every crate; `Uuid::parse_str` only at the edge | Fable (cognition, mind), crate owners elsewhere |
| T6 | Text decisions: `should_respond`, working memory's self-parse, recall's `[t=` parse, the sentinel's error-string matching (typed errors from the source), hardware detection by name (typed probe results) | Fable (cognition, mind), Codex (engine, serving), Cormac (sentinel, genome) |

Each slice deletes the string path in the same change: no dual API, no back-compat.
