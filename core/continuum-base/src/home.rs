//! The continuum home: `$CONTINUUM_HOME`, else `~/.continuum`. The one rule, at the floor, so
//! everything that reads the home's files resolves the same tree: the kernel's
//! `paths::continuum_home` (which adds its test-build homes in between), and this crate's own
//! `config_env` and `secrets`, which used to build `~/.continuum` by hand and so ignored
//! `$CONTINUUM_HOME` (card f04a437f, line 7).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// The continuum home as a production process resolves it.
pub fn continuum_home() -> Result<PathBuf, String> {
    continuum_home_with(|| None)
}

/// The continuum home with one step between an explicit `$CONTINUUM_HOME` and the
/// `~/.continuum` default: the kernel's per-process test home, so a test binary never reaches
/// the operator's state. A retired `$CONTINUUM_ROOT` naming another tree is refused either way.
pub fn continuum_home_with(before_default: impl FnOnce() -> Option<PathBuf>) -> Result<PathBuf, String> {
    let home = set(std::env::var_os("CONTINUUM_HOME"))
        .map(PathBuf::from)
        .or_else(before_default)
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))
        .ok_or_else(|| "no home dir".to_string())?;
    retired_root(set(std::env::var_os("CONTINUUM_ROOT")).as_deref(), &home)?;
    Ok(home)
}

/// An environment value that is set and not empty: an empty one is unset, as the shell's
/// `${VAR:-default}` and `[ -n "$VAR" ]` read it, so the core and the lifecycle scripts never
/// resolve two different homes from one environment (Codex on #5420).
fn set(value: Option<OsString>) -> Option<OsString> {
    value.filter(|v| !v.is_empty())
}

/// `$CONTINUUM_ROOT` was a second name for the continuum home and is no longer read (line 7). Set
/// to anything but the home this process resolved, it would silently open another state tree, so
/// it is refused, naming the one setting that keeps the operator on the tree they meant. Nothing
/// is moved.
fn retired_root(root: Option<&OsStr>, home: &Path) -> Result<(), String> {
    match root {
        Some(root) if Path::new(root) != home => Err(format!(
            "$CONTINUUM_ROOT={} is no longer read: the continuum home is $CONTINUUM_HOME, else ~/.continuum ({} here). \
             Set CONTINUUM_HOME={} to keep that tree, or unset CONTINUUM_ROOT; nothing was moved",
            root.to_string_lossy(),
            home.display(),
            root.to_string_lossy()
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on #5420): an operator's retired $CONTINUUM_ROOT silently ignored,
    // so the core opens a different state tree. A root that is not the resolved home is refused,
    // naming CONTINUUM_HOME=<that root>; one equal to the home, or none, passes.
    #[test]
    fn a_retired_root_that_names_another_tree_is_refused() {
        let home = Path::new("/home/op/.continuum");
        let refusal = retired_root(Some(OsStr::new("/srv/continuum")), home).expect_err("another tree"); // JUSTIFIED: test asserts the refusal
        assert!(refusal.contains("CONTINUUM_HOME=/srv/continuum"), "{refusal}");
        assert_eq!(retired_root(Some(home.as_os_str()), home), Ok(()));
        assert_eq!(retired_root(None, home), Ok(()));
    }

    // what this catches (Codex on #5420): an empty CONTINUUM_HOME read as the path "" and an
    // empty CONTINUUM_ROOT read as set, while the scripts read both as unset. Empty is unset.
    #[test]
    fn an_empty_value_is_unset_as_the_shell_reads_it() {
        assert_eq!(set(Some(OsString::new())), None);
        assert_eq!(set(None), None);
        assert_eq!(set(Some("/srv/c".into())), Some("/srv/c".into()));
    }
}
