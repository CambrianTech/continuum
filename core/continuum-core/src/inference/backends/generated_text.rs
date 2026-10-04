//! Incremental tokenizer-byte decoding and stop withholding, shared by local
//! scheduler and mtmd generation. Owns only the unpublished suffix.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StopOrigin { Caller, Template }

pub(super) struct TerminalText {
    pub(super) observed: String,
    pub(super) origin: StopOrigin,
}

pub(super) struct GeneratedText {
    pending: Vec<u8>,
    stops: Vec<(String, StopOrigin)>,
    matched: Option<TerminalText>,
    terminal: bool,
}

impl GeneratedText {
    pub(super) fn new(stops: impl IntoIterator<Item = String>) -> Self {
        Self::with_origins(stops.into_iter().map(|s| (s, StopOrigin::Caller)))
    }

    pub(super) fn with_origins(stops: impl IntoIterator<Item = (String, StopOrigin)>) -> Self {
        Self { pending: Vec::new(), stops: stops.into_iter().filter(|(s, _)| !s.is_empty()).collect(),
            matched: None, terminal: false }
    }

    pub(super) fn matched_stop(&self) -> Option<&str> { self.matched.as_ref().map(|s| s.observed.as_str()) }
    pub(super) fn terminal_text(&self) -> Option<&TerminalText> { self.matched.as_ref() }

    /// A genuinely sampled EOG token explicitly mapped to a native parser closer.
    /// Existing caller stops retain precedence, including a match in pending bytes.
    pub(super) fn push_template_end(&mut self, bytes: &[u8]) -> Result<String, String> {
        let observed = std::str::from_utf8(bytes).map_err(|_| "invalid native terminal UTF-8".to_string())?;
        if observed.is_empty() { return Err("empty native terminal bytes".into()); }
        self.stops.push((observed.to_owned(), StopOrigin::Template));
        self.push(bytes)
    }

    pub(super) fn push(&mut self, bytes: &[u8]) -> Result<String, String> {
        if self.terminal { return Err("generated text decoder is terminal".into()); }
        self.pending.extend_from_slice(bytes);
        let found = self.stops.iter().filter_map(|(stop, origin)| {
            self.pending.windows(stop.len()).position(|part| part == stop.as_bytes())
                .map(|offset| (offset, stop.len(), *origin))
        }).min_by_key(|(offset, _, _)| *offset);
        if let Some((offset, len, origin)) = found {
            // Bytes beyond the first actual stop belong to neither public output
            // nor final parsing; retain the real matched delimiter separately.
            let text = std::str::from_utf8(&self.pending[..offset])
                .map(str::to_owned).map_err(|_| "invalid UTF-8 before generated stop".to_string());
            self.terminal = true;
            if text.is_ok() {
                // Copy observed bytes, never reconstruct syntax from configured stops.
                self.matched = Some(TerminalText {
                    observed: std::str::from_utf8(&self.pending[offset..offset + len])
                        .expect("matched UTF-8 stop").to_owned(), origin,
                });
            }
            self.pending.clear();
            return text;
        }
        let valid_end = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => { self.terminal = true; return Err("invalid generated UTF-8".into()); }
        };
        let mut withheld = 0;
        for (stop, _) in &self.stops {
            for size in 1..stop.len() {
                if self.pending.ends_with(&stop.as_bytes()[..size]) { withheld = withheld.max(size); }
            }
        }
        let mut end = valid_end.min(self.pending.len() - withheld);
        // Stop prefixes can end inside a multi-byte character; publish whole text only.
        while std::str::from_utf8(&self.pending[..end]).is_err() { end -= 1; }
        let text = std::str::from_utf8(&self.pending[..end]).expect("validated prefix").to_owned();
        self.pending.drain(..end);
        Ok(text)
    }

    pub(super) fn finish(&mut self) -> Result<String, String> {
        if self.matched.is_some() { return Ok(String::new()); }
        if self.terminal { return Err("generated text decoder is terminal".into()); }
        self.terminal = true;
        let tail = std::str::from_utf8(&self.pending).map(str::to_owned)
            .map_err(|_| "incomplete generated UTF-8 at completion".to_string());
        self.pending.clear();
        tail
    }
}
