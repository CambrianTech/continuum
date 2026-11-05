//! A `code/edit` call: its schema IS its parser (ONE-ACT 6b).
//!
//! The schema she is shown is derived from [`CodeEditParams`] (the canonical tagged
//! `edit_mode`), and her call decodes into it with no normalizer guessing a mode. Two other
//! shapes are accepted only where they name exactly one typed edit:
//! - the names other agents emit are serde aliases on [`EditMode`] itself (`old_string` /
//!   `new_string` / `replace_all`, Claude Code's Edit tool);
//! - the same fields written flat beside `file_path`, or with `edit_mode` as a bare mode name,
//!   when the fields present are exactly one mode's fields. Anything else (a mode inferred from
//!   a partial set, or a flat field beside a tagged `edit_mode`) is a typed refusal naming the
//!   field, never a guess: the old normalizer read `{end_line, new_content}` as "from line 1",
//!   which replaced the top of the file (Benchy, the M5, 2026-11-05).

use serde::de::{self, value::MapAccessDeserializer, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use ts_rs::TS;

use crate::code::EditMode;

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
pub struct CodeEditParams {
    // The field docs are what the advertised schema says, so they stay as short as the
    // operation's own variants (the agentic surface's token ceiling pins them).
    /// Path relative to the workspace root.
    pub file_path: String,
    /// Operation to apply.
    pub edit_mode: EditMode,
    /// Optional note recorded in change history.
    #[serde(default)]
    pub description: Option<String>,
}

/// Every key a `code/edit` call may carry, as decoded: the canonical three, and one mode's
/// fields written flat (under their canonical names or the aliases [`EditMode`] accepts).
#[derive(Deserialize)]
struct Wire {
    file_path: String,
    #[serde(default)]
    edit_mode: Option<ModeInput>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, alias = "old_string", alias = "old_str")]
    search: Option<String>,
    #[serde(default, alias = "new_string", alias = "new_str")]
    replace: Option<String>,
    #[serde(default, alias = "replace_all")]
    all: Option<bool>,
    #[serde(default)]
    start_line: Option<u32>,
    #[serde(default)]
    end_line: Option<u32>,
    #[serde(default)]
    new_content: Option<String>,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    content: Option<String>,
}

/// `edit_mode` as she wrote it: the tagged operation, or only its name.
enum ModeInput {
    Op(EditMode),
    Named(String),
}

impl<'de> Deserialize<'de> for ModeInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ModeVisitor;
        impl<'de> Visitor<'de> for ModeVisitor {
            type Value = ModeInput;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an edit_mode object, or the name of an edit mode")
            }
            fn visit_str<E: de::Error>(self, name: &str) -> Result<ModeInput, E> {
                Ok(ModeInput::Named(name.to_string()))
            }
            // The tagged operation decodes through the same map access, so its own refusals
            // (a missing field, an unknown tag) keep their type and their path.
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ModeInput, A::Error> {
                EditMode::deserialize(MapAccessDeserializer::new(map)).map(ModeInput::Op)
            }
        }
        deserializer.deserialize_any(ModeVisitor)
    }
}

/// Each edit mode by its tag, with every field it takes (`all` is search_replace's optional one).
const MODES: [(&str, &[&str]); 4] = [
    ("search_replace", &["search", "replace", "all"]),
    ("line_range", &["start_line", "end_line", "new_content"]),
    ("insert_at", &["line", "content"]),
    ("append", &["content"]),
];

const MODE_TAGS: &[&str] = &["search_replace", "line_range", "insert_at", "append"];

/// The fields a mode cannot go without.
fn required(fields: &'static [&'static str]) -> impl Iterator<Item = &'static str> {
    fields.iter().copied().filter(|f| *f != "all")
}

impl Wire {
    /// The mode fields written flat, by canonical name.
    fn flat(&self) -> Vec<&'static str> {
        [
            ("search", self.search.is_some()),
            ("replace", self.replace.is_some()),
            ("all", self.all.is_some()),
            ("start_line", self.start_line.is_some()),
            ("end_line", self.end_line.is_some()),
            ("new_content", self.new_content.is_some()),
            ("line", self.line.is_some()),
            ("content", self.content.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, present)| present.then_some(name))
        .collect()
    }

    /// The one edit the flat fields name (with `named`, the mode she named), or the typed
    /// refusal of the field that keeps them from naming exactly one.
    fn into_edit<E: de::Error>(self, named: Option<&str>) -> Result<EditMode, E> {
        let flat = self.flat();
        // The modes whose fields hold everything she sent.
        let admits = |fields: &[&str]| flat.iter().all(|f| fields.contains(f));
        let complete = |fields: &'static [&'static str]| required(fields).all(|f| flat.contains(&f));
        let (mode, fields) = match named {
            Some(name) => *MODES.iter().find(|(mode, _)| *mode == name).ok_or_else(|| E::unknown_variant(name, MODE_TAGS))?,
            None => {
                let candidates: Vec<_> = MODES.iter().filter(|(_, fields)| admits(fields)).collect();
                let done: Vec<_> = candidates.iter().filter(|(_, fields)| complete(fields)).collect();
                match (done.as_slice(), candidates.as_slice()) {
                    ([only], _) => ***only,
                    ([], [only]) if !flat.is_empty() => **only,
                    _ => return Err(E::missing_field("edit_mode")),
                }
            }
        };
        if let Some(stray) = flat.iter().find(|f| !fields.contains(f)) {
            return Err(E::unknown_field(stray, fields));
        }
        if let Some(absent) = required(fields).find(|f| !flat.contains(f)) {
            return Err(E::missing_field(absent));
        }
        let have = |value: Option<String>, field: &'static str| value.ok_or_else(|| E::missing_field(field));
        let at = |value: Option<u32>, field: &'static str| value.ok_or_else(|| E::missing_field(field));
        Ok(match mode {
            "search_replace" => EditMode::SearchReplace {
                search: have(self.search, "search")?,
                replace: have(self.replace, "replace")?,
                all: self.all.unwrap_or(false), // unwrap_or: `all` is optional, false by default, as on EditMode
            },
            "line_range" => EditMode::LineRange {
                start_line: at(self.start_line, "start_line")?,
                end_line: at(self.end_line, "end_line")?,
                new_content: have(self.new_content, "new_content")?,
            },
            "insert_at" => EditMode::InsertAt { line: at(self.line, "line")?, content: have(self.content, "content")? },
            _ => EditMode::Append { content: have(self.content, "content")? },
        })
    }
}

impl<'de> Deserialize<'de> for CodeEditParams {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut wire = Wire::deserialize(deserializer)?;
        let (file_path, description) = (std::mem::take(&mut wire.file_path), wire.description.take());
        let edit_mode = match wire.edit_mode.take() {
            Some(ModeInput::Op(op)) => match wire.flat().first() {
                // A flat field beside a tagged operation: two edits, not one.
                Some(stray) => return Err(de::Error::unknown_field(stray, &["file_path", "edit_mode", "description"])),
                None => op,
            },
            Some(ModeInput::Named(name)) => wire.into_edit(Some(&name))?,
            None => wire.into_edit(None)?,
        };
        Ok(CodeEditParams { file_path, edit_mode, description })
    }
}
