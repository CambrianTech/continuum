//! [`CommandIdent`] — every derived name a generated command file needs, computed
//! ONCE from the wire name so the scaffold renderer and the mod-tree wiring can't
//! disagree about where a command lives or what its types are called.
//!
//! The contract (task #48): the source tree IS the namespace. A wire name
//! `data/list` ⟺ the file `commands/data/list.rs` ⟺ the struct `DataList`. This
//! type is the single place that mapping is defined.

use std::path::PathBuf;

use crate::sdk_codegen::CommandError;

/// All the identifiers derived from a command's wire name (e.g. `"data/list"`).
///
/// Built via [`CommandIdent::parse`], which validates the name and rejects the
/// shapes that would produce an uncompilable module tree (empty segments, a
/// leading/trailing slash, non-identifier characters). Fail-loud at parse time so
/// the generator never emits a file that won't build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandIdent {
    /// The wire name, verbatim (`"data/list"`). The routing key every caller binds.
    pub name: String,
    /// Path segments (`["data", "list"]`).
    pub segments: Vec<String>,
    /// Rust-safe module identifier for the file stem (`"list"`; hyphens → `_`).
    pub mod_stem: String,
    /// PascalCase struct name concatenating every segment (`"DataList"`).
    pub struct_name: String,
    /// The params type name (`"DataListParams"`).
    pub params_type: String,
    /// The result type name (`"DataListResult"`).
    pub result_type: String,
    /// Source-relative file path (`"commands/data/list.rs"`), relative to `src/`.
    pub rel_file: PathBuf,
    /// First segment — the `protocol/typescript/<subdir>/` the wire types export to.
    pub ts_subdir: String,
}

impl CommandIdent {
    /// Parse a wire name into every derived identifier, or a typed `Invalid` error
    /// naming exactly what's wrong with the name.
    pub fn parse(name: &str) -> Result<Self, CommandError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CommandError::Invalid("command name is empty".into()));
        }
        if name.starts_with('/') || name.ends_with('/') {
            return Err(CommandError::Invalid(format!(
                "command name '{name}' must not start or end with '/'"
            )));
        }
        let segments: Vec<String> = name.split('/').map(|s| s.to_string()).collect();
        for seg in &segments {
            if seg.is_empty() {
                return Err(CommandError::Invalid(format!(
                    "command name '{name}' has an empty path segment (a double slash?)"
                )));
            }
            // A segment must read as `[a-z0-9_-]+` so it maps to a Rust module
            // identifier (hyphens normalized to underscores) and a clean URI.
            if !seg
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
            {
                return Err(CommandError::Invalid(format!(
                    "command name segment '{seg}' must be lowercase [a-z0-9_-]"
                )));
            }
        }

        let verb = segments.last().expect("non-empty checked above").clone();
        let mod_stem = verb.replace('-', "_");
        let struct_name = segments.iter().map(|s| pascal(s)).collect::<String>();
        let params_type = format!("{struct_name}Params");
        let result_type = format!("{struct_name}Result");

        let mut rel_file = PathBuf::from("commands");
        for (i, seg) in segments.iter().enumerate() {
            if i + 1 == segments.len() {
                rel_file.push(format!("{mod_stem}.rs"));
            } else {
                rel_file.push(seg.replace('-', "_"));
            }
        }

        let ts_subdir = segments[0].replace('-', "_");

        Ok(Self {
            name: name.to_string(),
            segments,
            mod_stem,
            struct_name,
            params_type,
            result_type,
            rel_file,
            ts_subdir,
        })
    }

    /// The ancestor module directories under `commands/` that must carry a
    /// `pub mod <child>;` line, paired with the child identifier to insert.
    ///
    /// For `data/list`: `[(commands/mod.rs, "data"), (commands/data/mod.rs, "list")]`.
    /// For a top-level `ping`: `[(commands/mod.rs, "ping")]`. The list is the exact
    /// set of edits that make the new file reachable from the crate root.
    pub fn mod_wiring(&self) -> Vec<ModEdit> {
        let mut edits = Vec::new();
        let mut dir = PathBuf::from("commands");
        for (i, seg) in self.segments.iter().enumerate() {
            let ident = if i + 1 == self.segments.len() {
                self.mod_stem.clone()
            } else {
                seg.replace('-', "_")
            };
            edits.push(ModEdit {
                mod_file: dir.join("mod.rs"),
                child: ident.clone(),
            });
            if i + 1 < self.segments.len() {
                dir.push(seg.replace('-', "_"));
            }
        }
        edits
    }
}

/// One `pub mod <child>;` line to ensure inside a `commands/.../mod.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModEdit {
    /// Source-relative mod file (`"commands/data/mod.rs"`), relative to `src/`.
    pub mod_file: PathBuf,
    /// The child module identifier to declare (`"list"`).
    pub child: String,
}

/// PascalCase a single segment: split on `-`/`_`, uppercase each subword's first
/// char. `"git-status"` → `"GitStatus"`, `"data"` → `"Data"`.
fn pascal(segment: &str) -> String {
    segment
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the name→identifier mapping (the path==namespace contract).
    // A drift here means the generated file lands at the wrong path or declares a
    // struct/type name that collides or doesn't match the wire name.
    #[test]
    fn derives_every_identifier_from_a_two_segment_name() {
        let id = CommandIdent::parse("data/list").expect("valid");
        assert_eq!(id.segments, vec!["data", "list"]);
        assert_eq!(id.mod_stem, "list");
        assert_eq!(id.struct_name, "DataList");
        assert_eq!(id.params_type, "DataListParams");
        assert_eq!(id.result_type, "DataListResult");
        assert_eq!(id.rel_file, PathBuf::from("commands/data/list.rs"));
        assert_eq!(id.ts_subdir, "data");
    }

    // what this catches: three-segment names nest correctly and hyphenated verbs
    // normalize to a Rust-safe module stem while keeping the hyphen in the wire name.
    #[test]
    fn handles_nested_and_hyphenated_names() {
        let id = CommandIdent::parse("code/git/status").expect("valid");
        assert_eq!(id.struct_name, "CodeGitStatus");
        assert_eq!(id.rel_file, PathBuf::from("commands/code/git/status.rs"));

        let h = CommandIdent::parse("runtime/spawn-region").expect("valid");
        assert_eq!(h.mod_stem, "spawn_region");
        assert_eq!(h.struct_name, "RuntimeSpawnRegion");
        assert_eq!(
            h.rel_file,
            PathBuf::from("commands/runtime/spawn_region.rs")
        );
    }

    // what this catches: mod_wiring lists exactly the pub-mod edits that make a
    // nested command reachable from the crate root — the wiring step relies on this
    // being complete and ordered root→leaf.
    #[test]
    fn mod_wiring_lists_root_to_leaf_edits() {
        let id = CommandIdent::parse("data/list").expect("valid");
        let edits = id.mod_wiring();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].mod_file, PathBuf::from("commands/mod.rs"));
        assert_eq!(edits[0].child, "data");
        assert_eq!(edits[1].mod_file, PathBuf::from("commands/data/mod.rs"));
        assert_eq!(edits[1].child, "list");
    }

    // what this catches: invalid names fail loud at parse — never emit a file that
    // won't compile or a name with an empty/illegal segment.
    #[test]
    fn rejects_malformed_names() {
        assert!(CommandIdent::parse("").is_err());
        assert!(CommandIdent::parse("/data").is_err());
        assert!(CommandIdent::parse("data/").is_err());
        assert!(CommandIdent::parse("data//list").is_err());
        assert!(
            CommandIdent::parse("Data/List").is_err(),
            "uppercase rejected"
        );
    }
}
