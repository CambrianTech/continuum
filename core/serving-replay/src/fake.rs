//! A scripted seat engine for the replay's own tests (gated: `test-fixtures`). Each seat
//! holds the rendered text of its last act; a token is a character, so reuse is the
//! common prefix of the seat's text and the new prompt, exactly as a prefix cache works.

use std::collections::HashMap;

use crate::seated::{RestoredPage, SavedPage, SlotEngine};
use crate::target::{ActOutcome, ActRequest, EngineStats, TargetError};

pub struct FakeEngine {
    seats: Vec<String>,
    pages: HashMap<String, String>,
    acts: usize,
    /// The engine dies before this act number (0-based), once, losing every seat.
    pub die_before_act: Option<usize>,
    dead: bool,
    relaunches: u32,
    /// Page names whose restore the engine refuses.
    pub refuse_restore: Vec<String>,
}

impl FakeEngine {
    pub fn new(seats: u32) -> Self {
        Self {
            seats: vec![String::new(); seats as usize],
            pages: HashMap::new(),
            acts: 0,
            die_before_act: None,
            dead: false,
            relaunches: 0,
            refuse_restore: Vec::new(),
        }
    }

    fn render(request: &ActRequest) -> String {
        let mut text = request.system.clone().unwrap_or_default(); // unwrap_or_default: no system prompt renders empty
        for tool in &request.tools {
            text.push_str(&tool.to_string());
        }
        for m in &request.messages {
            text.push_str(&format!("<{}>{}", m.role, m.content));
        }
        text
    }
}

impl SlotEngine for FakeEngine {
    fn seats(&self) -> u32 {
        self.seats.len() as u32
    }

    fn revive(&mut self) -> Result<bool, TargetError> {
        if !self.dead {
            return Ok(false);
        }
        self.dead = false;
        self.relaunches += 1;
        self.seats.iter_mut().for_each(String::clear);
        Ok(true)
    }

    fn save(&mut self, seat: u32, page: &str) -> Result<SavedPage, TargetError> {
        let text = self.seats[seat as usize].clone();
        let tokens = text.chars().count() as u64;
        self.pages.insert(page.to_owned(), text);
        Ok(SavedPage { tokens, bytes: tokens * 2, ms: 1.0 })
    }

    fn restore(&mut self, seat: u32, page: &str) -> Result<RestoredPage, TargetError> {
        if self.refuse_restore.iter().any(|p| p == page) {
            return Err(TargetError(format!("restore of {page} refused")));
        }
        let text = self.pages.get(page).cloned().ok_or_else(|| TargetError(format!("no page {page}")))?;
        let tokens = text.chars().count() as u64;
        self.seats[seat as usize] = text;
        Ok(RestoredPage { tokens, ms: 0.5 })
    }

    fn act(&mut self, seat: u32, request: &ActRequest) -> Result<ActOutcome, TargetError> {
        if self.die_before_act == Some(self.acts) {
            self.die_before_act = None;
            self.dead = true;
            self.acts += 1;
            return Err(TargetError("engine died mid-act".into()));
        }
        self.acts += 1;
        let prompt = Self::render(request);
        let held = &self.seats[seat as usize];
        let cached = held.chars().zip(prompt.chars()).take_while(|(a, b)| a == b).count() as u64;
        let total = prompt.chars().count() as u64;
        self.seats[seat as usize] = prompt;
        Ok(ActOutcome {
            prompt_tokens: total,
            cached_tokens: cached,
            fresh_tokens: total - cached,
            generated_tokens: u64::from(request.n_predict),
            prompt_ms: (total - cached) as f64 / 1000.0,
            generate_ms: 1.0,
        })
    }

    fn stats(&mut self) -> EngineStats {
        EngineStats { engine: "fake".into(), seats: self.seats.len() as u32, relaunches: Some(self.relaunches), argv: vec![] }
    }
}
