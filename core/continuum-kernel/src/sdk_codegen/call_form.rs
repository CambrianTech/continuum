//! A call as she makes it: the name her tool list offers and the arguments object she sends.
//!
//! Every example or suggested call her text shows (a command's manual, a refusal's next move)
//! renders through [`CallForm`], so each is the shape that decodes: `list_files({"path":"."})`.
//! It is never an envelope such as `{"tool_call": {"name": …, "arguments": …}}`: she copied that
//! wrapper into an argument field, `bash(cmd: {"tool_call": …})`, five times in fifteen minutes
//! (the M5, 2026-11-07, card 1d9c0a64). The name comes from [`super::offered_name_of`], the same
//! resolver her offered tools are named by.

use std::fmt::{self, Write as _};

use serde_json::{Map, Value};

/// A call in her dialect: her offered tool name with its arguments object. A null argument is
/// the absent one, so it is not shown: the call is the minimal one that decodes.
#[derive(Debug, Clone, Copy)]
pub struct CallForm<'a> {
    offered: &'a str,
    args: &'a Map<String, Value>,
}

impl<'a> CallForm<'a> {
    /// `offered` is a name from [`super::offered_name_of`]; `args` an object of the command's
    /// own params.
    pub fn new(offered: &'a str, args: &'a Map<String, Value>) -> Self {
        Self { offered, args }
    }
}

impl fmt::Display for CallForm<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shown: Map<String, Value> = self.args.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
        // A map of JSON values always serializes; an error is a formatter error.
        let args = serde_json::to_string(&shown).map_err(|_| fmt::Error)?; // boundary: her tool-call wire, the arguments she reads and sends back as written
        f.write_str(self.offered)?;
        f.write_char('(')?;
        f.write_str(&args)?;
        f.write_char(')')
    }
}

/// Why a text holds no [`CallForm`].
#[cfg(any(test, feature = "test-fixtures"))]
#[derive(Debug, thiserror::Error)]
pub enum CallFormMiss {
    #[error("no line has the form name({{...}})")]
    NoCall,
    #[error("the arguments are not a JSON object: {0}")]
    Arguments(serde_json::Error),
}

/// Read the first call in `text` back as (offered name, arguments): the inverse of
/// [`CallForm`]'s rendering, for tests that hold a rendered example to what decodes.
#[cfg(any(test, feature = "test-fixtures"))]
pub fn read_call(text: &str) -> Result<(String, Map<String, Value>), CallFormMiss> {
    let (name, args) = text
        .lines()
        .find_map(|line| {
            let (name, rest) = line.trim().split_once("({")?;
            let named = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || "_-/.".contains(c));
            Some((name, rest.strip_suffix(')')?)).filter(|_| named)
        })
        .ok_or(CallFormMiss::NoCall)?;
    let args = serde_json::from_str(&format!("{{{args}")).map_err(CallFormMiss::Arguments)?;
    Ok((name.to_owned(), args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: an example call rendered in any shape other than the one she calls
    // with (her tool name, then its arguments object), and the reader drifting from the renderer.
    #[test]
    fn a_call_renders_as_her_tool_name_and_arguments_and_reads_back() {
        let args: Map<String, Value> = [("path".to_owned(), json!("."))].into_iter().collect();
        let mut with_absent = args.clone();
        with_absent.insert("depth".to_owned(), Value::Null);
        let shown = CallForm::new("list_files", &with_absent).to_string();
        assert_eq!(shown, r#"list_files({"path":"."})"#, "an absent (null) argument is not shown");
        let (name, read) = read_call(&format!("Call it like this:\n{shown}\n\nArguments:")).expect("reads back");
        assert_eq!((name.as_str(), read), ("list_files", args));
        assert!(matches!(read_call("no call here"), Err(CallFormMiss::NoCall)));
    }
}
