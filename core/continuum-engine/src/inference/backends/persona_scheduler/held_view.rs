//! Her view's tokens, kept from her last act: her next act tokenizes only what is new (rule 9b: text is
//! tokenized once at the edge where it enters; nothing tokenized is tokenized again). Within an epoch
//! her view only appends (the codec's law), so her new prompt begins with the view her last act held;
//! the view's text and tokens are kept per persona, and an act whose prompt begins with them tokenizes
//! only past them. The split is a message boundary, where the template starts a special token, so the
//! parts tokenize as the whole does.
//!
//! The M5, 2026-11-26 00:52 to 01:06Z (build 7140): tokenizing her whole prompt each act took 0.27 to
//! 0.36 s at 61k tokens and 4.2 to 7.1 s at 100k to 233k while the Core was busy, all before her turn
//! reached the Core.
//!
//! "Begins with" is decided by structure (her view appends within an epoch) and checked by one
//! comparison of the kept view's bytes against the new prompt's start (a memcmp, ~25 us for 500 KB,
//! against seconds of tokenizing): a template that does not render a prefix identically must never feed
//! her tokens of text she was not sent.
//! cold: once an act, on the act's own thread; never on the engine thread.

use std::sync::Mutex;
use std::time::Instant;

use llama::Model;

/// What her last act's view held: its text and its tokens.
struct Held {
    text: String,
    tokens: Vec<i32>,
}

/// Each persona's held view, one entry a persona, replaced at every act: bounded by the residents
/// this engine serves (a one-off carries no persona and keeps none), about two bytes a character of
/// her view plus four a token.
#[derive(Default)]
pub struct HeldViews {
    held: Mutex<Vec<(uuid::Uuid, Held)>>,
}

impl HeldViews {
    /// Her prompt as tokens and where her view ends in them, tokenizing only past the view her last act
    /// held when her prompt begins with it. A turn with no persona, or a view rendered apart from the
    /// prompt, is tokenized whole (`super::prepare`).
    pub fn prepare(&self, model: &Model, persona: Option<uuid::Uuid>, prompt: &str, view: Option<&str>) -> Result<(Vec<i32>, Option<usize>), String> {
        let held_view = view.filter(|view| view.as_ptr() == prompt.as_ptr() && view.len() <= prompt.len());
        let (Some(persona), Some(view)) = (persona, held_view) else { return super::prepare(model, prompt, view) };
        let started = Instant::now();
        // her entry taken out, so no other mind waits while she tokenizes
        let previous = {
            let mut held = self.held.lock().map_err(|_| "the held views' lock was poisoned".to_string())?;
            held.iter().position(|(who, _)| *who == persona).map(|at| held.swap_remove(at).1)
        };
        let kept = previous.filter(|kept| view.as_bytes().starts_with(kept.text.as_bytes()));
        let (mut tokens, from) = match kept {
            Some(kept) => {
                let from = kept.text.len();
                (kept.tokens, from)
            }
            None => (Vec::new(), 0),
        };
        let reused = tokens.len();
        tokens.extend(model.tokenize(view.get(from..).unwrap_or(""), from == 0, true)?); // unwrap_or: a view no longer than what was kept adds nothing
        let view_end = tokens.len();
        let entry = Held { text: view.to_owned(), tokens: tokens.clone() };
        self.held.lock().map_err(|_| "the held views' lock was poisoned".to_string())?.push((persona, entry));
        tokens.extend(model.tokenize(prompt.get(view.len()..).unwrap_or(""), false, true)?); // unwrap_or: a view as long as the prompt leaves nothing after it
        continuum_base::probe!(
            class = "inference.submit.tokenize",
            prompt_tokens = tokens.len() as u64,
            reused_tokens = reused as u64,
            tokenized_tokens = (tokens.len() - reused) as u64,
            prompt_ms = started.elapsed().as_millis() as u64,
            view_ms = 0u64,
            "an act's prompt tokenized past the view her last act held"
        );
        let view_end = (view_end > 0 && view_end < tokens.len()).then_some(view_end);
        Ok((tokens, view_end))
    }
}
