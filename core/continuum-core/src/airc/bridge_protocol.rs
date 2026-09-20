//! Free-text `!continuum` directive parser — the inbound airc→continuum bridge,
//! Rust-native (headless-first; no Node `jtag airc/bridge` hop).
//!
//! # Why this exists
//!
//! airc rooms carry two kinds of traffic from peers:
//!
//! 1. **Conversation** — ordinary chat text. Per the vuln-A contract
//!    (`<pm-NONCE>` peer-content envelope) peer content is CONVERSATION, not
//!    instructions: it is bridged into a Continuum chat room verbatim and
//!    NEVER interpreted as a command.
//! 2. **Explicit directives** — a message that *opts in* by starting with the
//!    command prefix (`!continuum` by default). Only these are parsed as
//!    bounded development / test directives, and only against the
//!    **deny-by-default allowlist** below.
//!
//! This is the inbox-side companion to the outbound structured RPC
//! (`routing::airc_command_protocol` / `AircCommandRequest`). That path carries
//! typed JSON envelopes between two continuum-cores; THIS path parses the
//! free-text `!continuum …` a human or agent types in a chat room. They are
//! different wires and must not be conflated.
//!
//! # Headless-first port
//!
//! The TypeScript original lives at
//! `src/system/airc-bridge/shared/AircBridgeProtocol.ts` and is reached today
//! only by shelling out: `tools/scripts/continuum-airc-bridge.mjs` pipes each
//! airc line into `jtag airc/bridge`, which runs the TS parser under Node. The
//! comms→airc lane retires that Node hop. This module is the Rust port of the
//! parser — the security-critical, transport-agnostic half — so continuum-core
//! can classify inbound airc text natively. Later slices wire the airc inbound
//! stream into it and dispatch the parsed action to the kernel; this slice is
//! the pure parser, tested without a live mesh (exactly as the TS comment
//! promised: "stays transport-agnostic so it can be tested without a live mesh").
//!
//! The parser is kept byte-for-byte faithful to the TS so the two can run side
//! by side during the transition and a parity test can pin them together. When
//! the TS face is finally retired, this becomes the sole source of truth.
//!
//! # The allowlist (deny-by-default)
//!
//! The ONLY directives that parse to an actionable verb are: `ping`, `status`,
//! `rooms`, `activity list`, `export`, `assert seen <marker>`, `chat`. Every
//! other prefixed verb resolves to [`BridgeAction::Unknown`] carrying an error
//! — it does NOT fall through to arbitrary command execution. Adding a verb is
//! a deliberate edit here, never an open-ended escape hatch.

use serde::{Deserialize, Serialize};

/// Parsed classification of one inbound airc message.
///
/// Mirrors `AircBridgeAction` in `AircBridgeProtocol.ts`. `as_str` is the
/// stable lowercase tag used in structured outputs / probes — a rename would
/// break log scrapers, so it is pinned by a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BridgeAction {
    /// Bridge the text into a Continuum chat room (conversation, or an explicit
    /// `!continuum chat …`). Not a control directive.
    Chat,
    /// Liveness check.
    Ping,
    /// Report bridge / core status.
    Status,
    /// List rooms.
    Rooms,
    /// Export recent messages from a room.
    Export,
    /// Assert a marker string was seen in recent room history (test harness).
    AssertSeen,
    /// List recent activity entries.
    ActivityList,
    /// Self-originated echo (`[continuum]` prefix) — ignored to break loops.
    Skip,
    /// Prefixed, but the verb is not on the allowlist (or is malformed). Carries
    /// an operator-readable `error`. Never executes anything.
    Unknown,
}

impl BridgeAction {
    /// Stable lowercase tag for structured outputs / probe fields.
    pub fn as_str(self) -> &'static str {
        match self {
            BridgeAction::Chat => "chat",
            BridgeAction::Ping => "ping",
            BridgeAction::Status => "status",
            BridgeAction::Rooms => "rooms",
            BridgeAction::Export => "export",
            BridgeAction::AssertSeen => "assert-seen",
            BridgeAction::ActivityList => "activity-list",
            BridgeAction::Skip => "skip",
            BridgeAction::Unknown => "unknown",
        }
    }
}

/// Structured parser output. Mirrors `ParsedAircBridgeMessage` in the TS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedBridgeMessage {
    pub action: BridgeAction,
    pub original_text: String,
    pub sender_nick: String,
    pub channel: String,
    pub room: String,
    /// True when the message opted in with the command prefix. Plain
    /// conversation and self-echo `skip` are `false`.
    pub is_directive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Caller-supplied context for parsing. Mirrors `ParseAircBridgeOptions`.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub sender_nick: Option<String>,
    pub channel: Option<String>,
    pub room: Option<String>,
    pub command_prefix: Option<String>,
    pub default_room: Option<String>,
}

const DEFAULT_PREFIX: &str = "!continuum";
const DEFAULT_ROOM: &str = "general";
const DEFAULT_SENDER: &str = "airc-peer";
const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 500;

/// Derive a Continuum room name from an airc channel: strip a leading `#`,
/// trim, fall back to `fallback` (default `general`) when empty.
pub fn room_from_airc_channel(channel: Option<&str>, fallback: &str) -> String {
    let normalized = channel.unwrap_or("").trim().trim_start_matches('#').trim();
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized.to_string()
    }
}

/// Classify one inbound airc message into a [`ParsedBridgeMessage`].
///
/// Security contract: returns [`BridgeAction::Chat`] (conversation, not a
/// command) for ANY text that does not start with the command prefix, and a
/// self-echo `[continuum]` prefix maps to [`BridgeAction::Skip`]. Only prefixed
/// text reaches the allowlist in [`parse_directive`].
pub fn parse_airc_bridge_message(text: &str, options: &ParseOptions) -> ParsedBridgeMessage {
    let prefix = options
        .command_prefix
        .as_deref()
        .filter(|p| !p.is_empty())
        .unwrap_or(DEFAULT_PREFIX);
    let ctx = ParseContext::new(text, options);
    let trimmed = text.trim();

    // Self-originated mirror — never re-interpret our own echo (loop guard).
    if trimmed.starts_with("[continuum]") {
        return ctx.parsed(BridgeAction::Skip, |p| {
            p.is_directive = false;
            p.message = Some(text.to_string());
        });
    }

    // No prefix → conversation, bridged verbatim. NOT a command.
    if !trimmed.starts_with(prefix) {
        return ctx.parsed(BridgeAction::Chat, |p| {
            p.is_directive = false;
            p.message = Some(text.to_string());
        });
    }

    let rest = trimmed[prefix.len()..].trim();
    parse_directive(&ctx, tokenize(rest), prefix)
}

/// Render the bridged chat line for a parsed message:
/// `[airc:<sender>] <body>` (body = explicit message, else original text).
pub fn format_airc_bridge_chat_text(parsed: &ParsedBridgeMessage) -> String {
    let body = parsed.message.as_deref().unwrap_or(&parsed.original_text);
    format!("[airc:{}] {}", parsed.sender_nick, body)
}

/// Collapse CRLF, trim, and cap a bridge response at `max_chars`, appending a
/// truncation marker when clipped. Mirrors `summarizeBridgeResponse`.
pub fn summarize_bridge_response(text: &str, max_chars: usize) -> String {
    let normalized = text.replace("\r\n", "\n");
    let normalized = normalized.trim();
    if normalized.chars().count() <= max_chars {
        return normalized.to_string();
    }
    // Match the TS: keep the first (max_chars - 32) chars, trim trailing
    // whitespace, append the marker. Char-based to stay UTF-8 safe.
    let keep: String = normalized
        .chars()
        .take(max_chars.saturating_sub(32))
        .collect();
    format!("{}\n... [truncated]", keep.trim_end())
}

// ---------------------------------------------------------------------------
// internals
// ---------------------------------------------------------------------------

struct ParseContext {
    original_text: String,
    sender_nick: String,
    channel: String,
    room: String,
}

impl ParseContext {
    fn new(text: &str, options: &ParseOptions) -> Self {
        let fallback_room = options.default_room.as_deref().unwrap_or(DEFAULT_ROOM);
        let sender_nick = non_empty(options.sender_nick.as_deref()).unwrap_or(DEFAULT_SENDER);
        let explicit_room = non_empty(options.room.as_deref());
        ParseContext {
            original_text: text.to_string(),
            sender_nick: sender_nick.to_string(),
            channel: room_from_airc_channel(options.channel.as_deref(), fallback_room),
            room: explicit_room.unwrap_or(fallback_room).to_string(),
        }
    }

    /// Build a parsed message defaulting to `is_directive = true` (every
    /// prefixed branch), letting `f` override fields.
    fn parsed(
        &self,
        action: BridgeAction,
        f: impl FnOnce(&mut ParsedBridgeMessage),
    ) -> ParsedBridgeMessage {
        let mut p = ParsedBridgeMessage {
            action,
            original_text: self.original_text.clone(),
            sender_nick: self.sender_nick.clone(),
            channel: self.channel.clone(),
            room: self.room.clone(),
            is_directive: true,
            message: None,
            marker: None,
            limit: None,
            error: None,
        };
        f(&mut p);
        p
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|t| !t.is_empty())
}

fn parse_directive(
    ctx: &ParseContext,
    mut tokens: Vec<String>,
    prefix: &str,
) -> ParsedBridgeMessage {
    if tokens.is_empty() {
        return ctx.parsed(BridgeAction::Unknown, |p| {
            p.error = Some(format!("Missing directive after {prefix}"));
        });
    }
    let verb = tokens.remove(0).to_lowercase();

    match verb.as_str() {
        "ping" => ctx.parsed(BridgeAction::Ping, |_| {}),
        "status" => ctx.parsed(BridgeAction::Status, |_| {}),
        "rooms" => parse_rooms(ctx, &mut tokens),
        "activity" => parse_activity(ctx, &mut tokens),
        "export" => parse_export(ctx, &mut tokens),
        "assert" => parse_assert(ctx, &mut tokens),
        "chat" => parse_chat(ctx, &mut tokens),
        other => ctx.parsed(BridgeAction::Unknown, |p| {
            p.error = Some(format!("Unknown directive: {other}"));
        }),
    }
}

fn parse_rooms(ctx: &ParseContext, tokens: &mut Vec<String>) -> ParsedBridgeMessage {
    let limit = read_int_flag(tokens, "limit").unwrap_or(DEFAULT_LIMIT);
    ctx.parsed(BridgeAction::Rooms, |p| p.limit = Some(limit))
}

fn parse_activity(ctx: &ParseContext, tokens: &mut Vec<String>) -> ParsedBridgeMessage {
    let subcommand = if tokens.is_empty() {
        String::new()
    } else {
        tokens.remove(0).to_lowercase()
    };
    if subcommand != "list" {
        return ctx.parsed(BridgeAction::Unknown, |p| {
            p.error = Some("Expected: !continuum activity list".to_string());
        });
    }
    let limit = read_int_flag(tokens, "limit").unwrap_or(DEFAULT_LIMIT);
    ctx.parsed(BridgeAction::ActivityList, |p| p.limit = Some(limit))
}

fn parse_export(ctx: &ParseContext, tokens: &mut Vec<String>) -> ParsedBridgeMessage {
    let room = read_room_arg(tokens).unwrap_or_else(|| ctx.room.clone());
    let limit = read_int_flag(tokens, "last")
        .or_else(|| read_int_flag(tokens, "limit"))
        .unwrap_or(DEFAULT_LIMIT);
    ctx.parsed(BridgeAction::Export, |p| {
        p.room = room;
        p.limit = Some(limit);
    })
}

fn parse_assert(ctx: &ParseContext, tokens: &mut Vec<String>) -> ParsedBridgeMessage {
    let assertion = if tokens.is_empty() {
        String::new()
    } else {
        tokens.remove(0).to_lowercase()
    };
    let marker = if tokens.is_empty() {
        None
    } else {
        Some(tokens.remove(0))
    };
    match (assertion.as_str(), marker) {
        ("seen", Some(marker)) => {
            let room = read_string_flag(tokens, "room").unwrap_or_else(|| ctx.room.clone());
            let limit = read_int_flag(tokens, "last")
                .or_else(|| read_int_flag(tokens, "limit"))
                .unwrap_or(DEFAULT_LIMIT);
            ctx.parsed(BridgeAction::AssertSeen, |p| {
                p.marker = Some(marker);
                p.room = room;
                p.limit = Some(limit);
            })
        }
        _ => ctx.parsed(BridgeAction::Unknown, |p| {
            p.error = Some("Expected: !continuum assert seen <marker>".to_string());
        }),
    }
}

fn parse_chat(ctx: &ParseContext, tokens: &mut Vec<String>) -> ParsedBridgeMessage {
    let target_room = read_string_flag(tokens, "room").unwrap_or_else(|| ctx.room.clone());
    let message = tokens.join(" ").trim().to_string();
    if message.is_empty() {
        return ctx.parsed(BridgeAction::Unknown, |p| {
            p.error = Some("Expected: !continuum chat [--room room] <message>".to_string());
        });
    }
    ctx.parsed(BridgeAction::Chat, |p| {
        p.room = target_room;
        p.message = Some(message);
    })
}

/// Whitespace tokenizer with single/double quoting and backslash escapes.
/// Faithful to the TS `tokenize` state machine.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaping = false;

    for ch in input.chars() {
        if escaping {
            current.push(ch);
            escaping = false;
            continue;
        }
        if ch == '\\' {
            escaping = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }

    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Positional-or-flag room argument: `--room=x`, `--room x`, or a bare leading
/// token that is not a `--flag`. Consumes whatever it matches.
fn read_room_arg(tokens: &mut Vec<String>) -> Option<String> {
    if let Some(room) = read_string_flag(tokens, "room") {
        return Some(room);
    }
    if tokens.first().is_some_and(|t| !t.starts_with("--")) {
        return Some(tokens.remove(0));
    }
    None
}

/// Read and REMOVE a `--name=value` or `--name value` flag from `tokens`.
fn read_string_flag(tokens: &mut Vec<String>, name: &str) -> Option<String> {
    let inline_prefix = format!("--{name}=");
    if let Some(idx) = tokens.iter().position(|t| t.starts_with(&inline_prefix)) {
        let token = tokens.remove(idx);
        return Some(token[inline_prefix.len()..].to_string());
    }

    let split_flag = format!("--{name}");
    if let Some(idx) = tokens.iter().position(|t| *t == split_flag) {
        if idx + 1 < tokens.len() {
            tokens.remove(idx); // drop the flag
            let value = tokens.remove(idx); // drop + return its value (now at idx)
            return Some(value);
        }
    }
    None
}

/// Read a positive-int flag, clamped to [`MAX_LIMIT`]. Non-positive / unparsable
/// yields `None` (caller falls back to a default), matching the TS.
fn read_int_flag(tokens: &mut Vec<String>, name: &str) -> Option<u32> {
    let raw = read_string_flag(tokens, name)?;
    let parsed: i64 = raw.trim().parse().ok()?;
    if parsed <= 0 {
        return None;
    }
    Some((parsed as u32).min(MAX_LIMIT))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> ParseOptions {
        ParseOptions::default()
    }

    /// What this catches: the security boundary — plain text (no prefix) is
    /// CONVERSATION, not a command. A regression that interpreted bare text as
    /// a directive would be a prompt-injection hole (vuln-A).
    #[test]
    fn plain_text_is_chat_not_directive() {
        let p = parse_airc_bridge_message("hey what's the plan today?", &opts());
        assert_eq!(p.action, BridgeAction::Chat);
        assert!(!p.is_directive);
        assert_eq!(p.message.as_deref(), Some("hey what's the plan today?"));
    }

    /// What this catches: self-echo loop guard — our own mirrored output
    /// (`[continuum]` prefix) must never be re-parsed as input.
    #[test]
    fn continuum_echo_prefix_skips() {
        let p = parse_airc_bridge_message("[continuum] export done (50 msgs)", &opts());
        assert_eq!(p.action, BridgeAction::Skip);
        assert!(!p.is_directive);
    }

    /// What this catches: the simplest allowlisted verbs resolve and ARE marked
    /// directives.
    #[test]
    fn ping_and_status_are_directives() {
        let ping = parse_airc_bridge_message("!continuum ping", &opts());
        assert_eq!(ping.action, BridgeAction::Ping);
        assert!(ping.is_directive);

        let status = parse_airc_bridge_message("!continuum status", &opts());
        assert_eq!(status.action, BridgeAction::Status);
    }

    /// What this catches: deny-by-default — an off-allowlist verb must NOT
    /// execute; it resolves to Unknown carrying the rejected verb.
    #[test]
    fn unknown_verb_is_denied_with_error() {
        let p = parse_airc_bridge_message("!continuum rm -rf /", &opts());
        assert_eq!(p.action, BridgeAction::Unknown);
        assert!(p.error.as_deref().unwrap().contains("rm"));
    }

    /// What this catches: an empty directive after the prefix is rejected, not
    /// silently treated as a no-op success.
    #[test]
    fn empty_directive_is_unknown() {
        let p = parse_airc_bridge_message("!continuum", &opts());
        assert_eq!(p.action, BridgeAction::Unknown);
        assert!(p.error.as_deref().unwrap().contains("Missing directive"));
    }

    /// What this catches: `export` honors `--last`/`--limit` and the positional
    /// room arg, and clamps the limit to MAX_LIMIT.
    #[test]
    fn export_parses_room_and_clamps_limit() {
        let p = parse_airc_bridge_message("!continuum export design --last 9999", &opts());
        assert_eq!(p.action, BridgeAction::Export);
        assert_eq!(p.room, "design");
        assert_eq!(p.limit, Some(MAX_LIMIT));
    }

    /// What this catches: `export` with no args falls back to the context room
    /// and DEFAULT_LIMIT.
    #[test]
    fn export_defaults_to_context_room() {
        let options = ParseOptions {
            room: Some("ops".to_string()),
            ..Default::default()
        };
        let p = parse_airc_bridge_message("!continuum export", &options);
        assert_eq!(p.room, "ops");
        assert_eq!(p.limit, Some(DEFAULT_LIMIT));
    }

    /// What this catches: `activity` only accepts the `list` subcommand; a bare
    /// `activity` is rejected (no silent default).
    #[test]
    fn activity_requires_list_subcommand() {
        assert_eq!(
            parse_airc_bridge_message("!continuum activity list", &opts()).action,
            BridgeAction::ActivityList
        );
        assert_eq!(
            parse_airc_bridge_message("!continuum activity", &opts()).action,
            BridgeAction::Unknown
        );
    }

    /// What this catches: `assert seen <marker>` extracts the marker and room;
    /// a missing marker is rejected.
    #[test]
    fn assert_seen_extracts_marker() {
        let p = parse_airc_bridge_message("!continuum assert seen DEPLOY-OK --room ops", &opts());
        assert_eq!(p.action, BridgeAction::AssertSeen);
        assert_eq!(p.marker.as_deref(), Some("DEPLOY-OK"));
        assert_eq!(p.room, "ops");

        let bad = parse_airc_bridge_message("!continuum assert seen", &opts());
        assert_eq!(bad.action, BridgeAction::Unknown);
    }

    /// What this catches: explicit `!continuum chat` IS a directive but still
    /// produces a Chat action with the message body and target room; quoting is
    /// honored by the tokenizer.
    #[test]
    fn explicit_chat_directive_keeps_message_and_room() {
        let p =
            parse_airc_bridge_message("!continuum chat --room general \"hello there\"", &opts());
        assert_eq!(p.action, BridgeAction::Chat);
        assert!(p.is_directive);
        assert_eq!(p.room, "general");
        assert_eq!(p.message.as_deref(), Some("hello there"));
    }

    /// What this catches: `!continuum chat` with no body is rejected rather than
    /// sending an empty message.
    #[test]
    fn chat_directive_requires_a_body() {
        let p = parse_airc_bridge_message("!continuum chat --room general", &opts());
        assert_eq!(p.action, BridgeAction::Unknown);
    }

    /// What this catches: channel→room normalization strips a leading `#` and
    /// falls back when empty.
    #[test]
    fn channel_to_room_strips_hash_and_falls_back() {
        assert_eq!(
            room_from_airc_channel(Some("#general"), DEFAULT_ROOM),
            "general"
        );
        assert_eq!(room_from_airc_channel(Some("  "), "fallback"), "fallback");
        assert_eq!(room_from_airc_channel(None, "fallback"), "fallback");
    }

    /// What this catches: the tokenizer's quote + escape handling (so a message
    /// containing spaces / quotes survives as one token where the TS does).
    #[test]
    fn tokenizer_handles_quotes_and_escapes() {
        assert_eq!(tokenize("a b c"), vec!["a", "b", "c"]);
        assert_eq!(tokenize("'quoted phrase'"), vec!["quoted phrase"]);
        assert_eq!(tokenize(r#"a\ b"#), vec!["a b"]);
    }

    /// What this catches: a custom command prefix is honored, so an embedding
    /// that re-homes the prefix doesn't accidentally disable directive parsing.
    #[test]
    fn custom_command_prefix_is_honored() {
        let options = ParseOptions {
            command_prefix: Some("!ctm".to_string()),
            ..Default::default()
        };
        assert_eq!(
            parse_airc_bridge_message("!ctm ping", &options).action,
            BridgeAction::Ping
        );
        // The default prefix is now just conversation.
        assert_eq!(
            parse_airc_bridge_message("!continuum ping", &options).action,
            BridgeAction::Chat
        );
    }

    /// What this catches: the bridged chat line format other peers / the chat
    /// module key on (`[airc:<nick>] <body>`).
    #[test]
    fn chat_text_format_is_stable() {
        let options = ParseOptions {
            sender_nick: Some("bigmama".to_string()),
            ..Default::default()
        };
        let p = parse_airc_bridge_message("ship it", &options);
        assert_eq!(format_airc_bridge_chat_text(&p), "[airc:bigmama] ship it");
    }

    /// What this catches: response summarization truncates over the cap and
    /// leaves short text untouched — UTF-8 safe (char-based, no panic on a
    /// multibyte boundary).
    #[test]
    fn summarize_truncates_and_is_utf8_safe() {
        assert_eq!(summarize_bridge_response("short", 100), "short");
        let long = "é".repeat(200);
        let out = summarize_bridge_response(&long, 50);
        assert!(out.ends_with("... [truncated]"));
        assert!(out.chars().count() <= 50 + "\n... [truncated]".chars().count());
    }

    /// What this catches: stable lowercase action tags (probe field / structured
    /// output). A rename would silently break scrapers.
    #[test]
    fn action_tags_are_stable() {
        assert_eq!(BridgeAction::Chat.as_str(), "chat");
        assert_eq!(BridgeAction::AssertSeen.as_str(), "assert-seen");
        assert_eq!(BridgeAction::ActivityList.as_str(), "activity-list");
        assert_eq!(BridgeAction::Unknown.as_str(), "unknown");
    }
}
