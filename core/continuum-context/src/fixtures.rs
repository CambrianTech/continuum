//! Test fixtures shared by the crate's test modules (one owner per concern): a fixture item and a
//! source shaped like the M5's.

use crate::cause::{Cause, ElementId};
use crate::levels::{summarize, Shape};
use crate::source::{Item, Source};

/// A fixture occurrence at `key`: its id from the key, its cause unrecorded (these tests are about
/// levels and starts, not chains).
pub(crate) fn item(key: impl Into<String>, full: String, summary: Option<String>, pointer: Option<String>) -> Item<String> {
    let key = key.into();
    let mut item = Item::whole(ElementId::of(&[key.as_bytes()]), Cause::Unrecorded, key, full);
    item.summary = summary;
    item.pointer = pointer;
    item
}

/// A source shaped like the M5's: items keyed, each renderable in full or as a pointer, and, when
/// `shaped`, at its output summary level too (`levels::summarize`, `Shape::Output`).
pub(crate) struct Fake {
    pub name: &'static str,
    pub priority: u8,
    pub items: Vec<(String, String)>,
    pub retired: Vec<(String, String)>,
    pub shaped: bool,
    /// Its items are states, each ended by `[<key> ended]` once it is no longer offered.
    pub states: bool,
}

impl Fake {
    pub fn new(name: &'static str, priority: u8) -> Self {
        Self { name, priority, items: Vec::new(), retired: Vec::new(), shaped: false, states: false }
    }

    /// The same source whose items carry their summary level.
    pub fn shaped(name: &'static str, priority: u8) -> Self {
        Self { shaped: true, ..Self::new(name, priority) }
    }

    /// The same source whose items are states (`Source::states`).
    pub fn states(name: &'static str, priority: u8) -> Self {
        Self { states: true, ..Self::new(name, priority) }
    }

    pub fn drop_item(&mut self, key: &str) {
        self.items.retain(|(k, _)| k != key);
    }

    pub fn push(&mut self, key: &str, text: &str) {
        match self.items.iter_mut().find(|(k, _)| k == key) {
            Some(item) => item.1 = text.to_owned(),
            None => self.items.push((key.to_owned(), text.to_owned())),
        }
    }
}

impl Source<String> for Fake {
    fn name(&self) -> &str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }
    fn items(&self) -> Vec<Item<String>> {
        self.items
            .iter()
            .map(|(key, text)| {
                let summary = if self.shaped { summarize(text, Shape::Output) } else { None };
                item(key.clone(), text.clone(), summary, Some(format!("[{key}: {} chars, reopen by key]", text.len())))
            })
            .collect()
    }
    fn states(&self) -> bool {
        self.states
    }
    fn ended(&self, key: &str) -> Option<String> {
        Some(format!("[{key} ended]"))
    }
    fn retired(&self) -> Vec<Item<String>> {
        self.retired
            .iter()
            .map(|(key, text)| item(key.clone(), text.clone(), None, Some(text.clone())))
            .collect()
    }

    fn fold(&self, _folded: &[Item<String>]) -> Option<String> {
        None // a fixture that does not exercise folding
    }
}
