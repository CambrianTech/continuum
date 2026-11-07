//! A refused call she can act on: why it was refused, typed, and what she can do next.
//!
//! A refusal that says only what went wrong leaves her to guess the next call, and a guess is
//! often the same call again (ONE-ACT 6b). So a refusal is built only with its next move:
//! [`Refusal::new`] takes a [`RefusalCause`] and [`NextMoves`], either the calls that would make
//! progress ([`NextCall`], each a verb in her hands with its own params) or the typed reason
//! there is none ([`NoMove`]). A refusal without a next move does not exist.
//!
//! The cause is the tool's own error type ([`RefusalCause`]); its kind ([`Category`]) is a
//! property of the cause, never a free string beside it, and its chain is `Error::source`.

use std::sync::Arc;

use serde::Serialize;

use super::{wire_name, offer_style_for, primary_alias, CommandSpec, VerbRef};
use continuum_base::tool_disclosure::Disclosure;

/// What kind of refusal a cause is: the classification a client or log reads without parsing
/// prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// The call decoded but failed a check of the request or a precondition of the domain.
    Invalid,
    /// What the call names (an output, a file, a card) does not exist.
    NotFound,
    /// The caller may not do this.
    Denied,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Invalid => "invalid",
            Category::NotFound => "not_found",
            Category::Denied => "denied",
        }
    }
}

/// A tool's typed reason for refusing a call. Each tool's own error enum implements it; its
/// chain of causes is `Error::source`.
pub trait RefusalCause: std::error::Error + Send + Sync + 'static {
    fn category(&self) -> Category;
}

/// A command by identity: its canonical name and the aliases it declared, taken from its
/// [`VerbRef`], so it renders exactly as her tool list offers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId {
    name: &'static str,
    aliases: &'static [&'static str],
    disclosure: Disclosure,
}

impl CommandId {
    fn of<C: CommandSpec>(_verb: VerbRef<C>) -> Self {
        Self { name: C::NAME, aliases: C::ALIASES, disclosure: C::DISCLOSURE }
    }

    /// The canonical command name, for matching a resolved call; never for her text.
    pub fn canonical(self) -> &'static str {
        self.name
    }

    /// Where the command declares it sits in her hands.
    pub fn disclosure(self) -> Disclosure {
        self.disclosure
    }

    /// The name her tool list offers it under ([`wire_name`]).
    pub fn offered(self) -> String {
        wire_name(self.name, primary_alias(self.aliases), offer_style_for(None))
    }
}

/// A call that would make progress after a refusal: a verb in her hands, with arguments of that
/// verb's own params type, so the call she is shown always decodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextCall {
    verb: CommandId,
    args: serde_json::Map<String, serde_json::Value>,
    why: &'static str,
}

/// A next call whose params did not serialize as an object of named fields.
#[derive(Debug, thiserror::Error)]
#[error("the next call to {verb} has params that are not an object of named fields: {problem}")]
pub struct NextCallArgs {
    verb: &'static str,
    problem: String,
}

impl NextCall {
    /// The call `verb(params)`, with `why` it would make progress. `params` is the command's own
    /// params type, serialized here beside the verb it belongs to, so the two cannot drift.
    pub fn new<C>(verb: VerbRef<C>, params: &C::Params, why: &'static str) -> Result<Self, NextCallArgs>
    where
        C: CommandSpec,
        C::Params: Serialize,
    {
        let refused = |problem: String| NextCallArgs { verb: C::NAME, problem };
        match serde_json::to_value(params).map_err(|e| refused(e.to_string()))? {
            serde_json::Value::Object(args) => Ok(Self { verb: CommandId::of(verb), args, why }),
            other => Err(refused(format!("they serialized as {other}"))),
        }
    }

    pub fn verb(&self) -> CommandId {
        self.verb
    }

    pub fn args(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.args
    }

    pub fn why(&self) -> &'static str {
        self.why
    }
}

impl std::fmt::Display for NextCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A map of JSON values always serializes; an error is a formatter error.
        let args = serde_json::to_string(&self.args).map_err(|_| std::fmt::Error)?; // boundary: her tool-call wire, the arguments she reads and sends back as written
        write!(f, "{}({args}): {}", self.verb.offered(), self.why)
    }
}

/// Why a refusal has no call that would make progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoMove {
    /// What she asked for no longer exists anywhere: what she was shown of it is all there is.
    NothingMoreExists,
}

impl std::fmt::Display for NoMove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoMove::NothingMoreExists => f.write_str("no call recovers it; what you were shown is all there is"),
        }
    }
}

/// What she can do after a refusal: at least one call, or the typed reason there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextMoves {
    Calls { first: NextCall, more: Vec<NextCall> },
    None(NoMove),
}

impl NextMoves {
    pub fn call(first: NextCall) -> Self {
        NextMoves::Calls { first, more: Vec::new() }
    }

    /// Every call, first first; empty for [`NextMoves::None`].
    pub fn calls(&self) -> impl Iterator<Item = &NextCall> {
        let (first, more): (Option<&NextCall>, &[NextCall]) = match self {
            NextMoves::Calls { first, more } => (Some(first), more),
            NextMoves::None(_) => (None, &[]),
        };
        first.into_iter().chain(more)
    }
}

/// A refused call: its typed cause and her next moves. Built only by [`Refusal::new`].
#[derive(Debug)]
pub struct Refusal {
    cause: Box<dyn RefusalCause>,
    next: NextMoves,
}

impl Refusal {
    pub fn new(cause: impl RefusalCause, next: NextMoves) -> Self {
        Self { cause: Box::new(cause), next }
    }

    pub fn category(&self) -> Category {
        self.cause.category()
    }

    pub fn cause(&self) -> &dyn RefusalCause {
        self.cause.as_ref()
    }

    pub fn next(&self) -> &NextMoves {
        &self.next
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cause)?;
        let mut source = self.cause.source();
        while let Some(cause) = source {
            write!(f, ": because {cause}")?;
            source = cause.source();
        }
        match &self.next {
            NextMoves::None(none) => write!(f, ". {none}"),
            NextMoves::Calls { .. } => {
                f.write_str(". Next:")?;
                for call in self.next.calls() {
                    write!(f, " {call};")?;
                }
                Ok(())
            }
        }
    }
}

/// Two refusals are the same when she would read the same thing: the same category and the
/// same rendered cause and next moves.
impl PartialEq for Refusal {
    fn eq(&self, other: &Self) -> bool {
        self.category() == other.category() && self.next == other.next && self.cause.to_string() == other.cause.to_string()
    }
}

impl Eq for Refusal {}

impl From<Refusal> for super::CommandError {
    fn from(refusal: Refusal) -> Self {
        super::CommandError::Refused(Arc::new(refusal))
    }
}

#[cfg(test)]
mod tests;
