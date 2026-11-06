//! Immutable prompt controls and the one serializer of resolved output authority.
use crate::ai::{inference_error::InferenceError, reply_room::{OutputIntent, QuotedReplyRoom}};

#[derive(Debug, Clone, Copy)]
pub(crate) enum SentOutput {
    Counted(QuotedReplyRoom),
    ProviderFixed(std::num::NonZeroU32),
}

impl SentOutput {
    pub fn reply(self) -> u32 {
        match self { Self::Counted(room) => room.reply, Self::ProviderFixed(reply) => reply.get() }
    }
}

pub(super) struct PreparedCommand {
    prompt: serde_json::Map<String, serde_json::Value>,
    intent: OutputIntent,
    fixed_body: Option<Vec<u8>>,
}

pub(super) struct ResolvedCommand {
    body: Vec<u8>,
    pub output: SentOutput,
}

impl PreparedCommand {
    pub fn new(body: &[u8], intent: OutputIntent) -> Result<Self, InferenceError> {
        let value: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| InferenceError::Protocol(format!("invalid prepared generation: {e}")))?;
        let serde_json::Value::Object(mut prompt) = value else {
            return Err(InferenceError::Protocol("prepared generation must be an object".into()));
        };
        // Native n_predict overrides max_tokens. No alternate output control may
        // disagree with the authority used for reservation and the sent receipt.
        for alias in ["n_predict", "max_completion_tokens", "max_output_tokens"] {
            if prompt.contains_key(alias) {
                return Err(InferenceError::Protocol(format!("prepared generation contains output alias {alias}")));
            }
        }
        if let OutputIntent::Fixed(reply) = intent {
            if let Some(cap) = prompt.get("max_tokens") {
                if cap.as_u64() != Some(u64::from(reply.get())) {
                    return Err(InferenceError::Protocol("prepared fixed output contradicts caller authority".into()));
                }
            }
        }
        // Output limits are not template input on this counted backend. Retain
        // every other finalized field; the serializer alone supplies max_tokens.
        let fixed_body = (matches!(intent, OutputIntent::Fixed(_)) && prompt.contains_key("max_tokens")).then(|| body.to_vec());
        prompt.remove("max_tokens");
        Ok(Self { prompt, intent, fixed_body })
    }

    pub fn quote_builder(&self, template: &reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, InferenceError> {
        let reply = match self.intent {
            OutputIntent::Fixed(reply) => reply.get(),
            OutputIntent::UseAdmittedRoom { required_reply } => required_reply.get(),
        };
        template.try_clone().map(|builder| self.body(reply).map(|body| builder.body(body)))
            .ok_or_else(|| InferenceError::Protocol("generation headers are not cloneable".into()))?
    }

    pub fn fixed(&self) -> Result<ResolvedCommand, InferenceError> {
        match self.intent {
            OutputIntent::Fixed(reply) => self.encode(SentOutput::ProviderFixed(reply)),
            OutputIntent::UseAdmittedRoom { .. } => Err(InferenceError::Protocol(
                "adaptive output requires an owned endpoint, exact quote and grant lease".into())),
        }
    }

    pub fn counted(&self, prompt: u32, physical: u32, admitted: u32) -> Result<ResolvedCommand, InferenceError> {
        let room = self.intent.resolve(prompt, physical, admitted)
            .map_err(|short| InferenceError::ContextExceeded { requested: short.prompt, available: short.window })?;
        self.encode(SentOutput::Counted(room))
    }

    fn encode(&self, output: SentOutput) -> Result<ResolvedCommand, InferenceError> {
        Ok(ResolvedCommand { body: self.body(output.reply())?, output })
    }

    fn body(&self, reply: u32) -> Result<Vec<u8>, InferenceError> {
        if let Some(body) = &self.fixed_body { return Ok(body.clone()); }
        let mut body = self.prompt.clone();
        body.insert("max_tokens".into(), serde_json::json!(reply));
        let body = serde_json::to_vec(&body) // wire boundary: immutable generation request sent to the model HTTP endpoint
            .map_err(|e| InferenceError::Protocol(format!("generation serialization failed: {e}")))?;
        Ok(body)
    }
}

impl ResolvedCommand {
    pub fn builder(&self, template: &reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, InferenceError> {
        template.try_clone().map(|builder| builder.body(self.body.clone()))
            .ok_or_else(|| InferenceError::Protocol("generation headers are not cloneable".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    #[test]
    fn output_aliases_and_contradictory_fixed_controls_are_refused() {
        let fixed = OutputIntent::Fixed(NonZeroU32::new(32).expect("positive fixture"));
        for alias in ["n_predict", "max_completion_tokens", "max_output_tokens"] {
            let mut body = serde_json::json!({"messages":[], "max_tokens":32});
            body[alias] = serde_json::json!(32000);
            assert!(PreparedCommand::new(&serde_json::to_vec(&body).expect("fixture"), fixed).is_err());
        }
        assert!(PreparedCommand::new(br#"{"max_tokens":33}"#, fixed).is_err());
        assert!(PreparedCommand::new(b"[]", fixed).is_err());
    }

    #[test]
    fn fixed_commands_keep_the_finalized_bytes_and_exact_authority() {
        let bytes = br#"{ "messages":[{"role":"user","content":"unchanged"}], "max_tokens":32 }"#;
        let fixed = OutputIntent::Fixed(NonZeroU32::new(32).expect("positive fixture"));
        let command = PreparedCommand::new(bytes, fixed).expect("valid fixed command");
        assert_eq!(command.fixed().expect("fixed").body, bytes);
        assert_eq!(command.counted(40, 100, 90).expect("fits").body, bytes);
        assert!(command.counted(58, 100, 90).is_err(), "strict one-token boundary remains enforced");
    }

    #[test]
    fn adaptive_intent_cannot_be_encoded_as_an_uncounted_fixed_send() {
        let command = PreparedCommand::new(br#"{"messages":[],"max_tokens":999999}"#,
            OutputIntent::UseAdmittedRoom { required_reply: NonZeroU32::new(4534).expect("positive fixture") }).expect("adaptive command");
        assert!(command.fixed().is_err());
        assert!(command.counted(27221, 32512, 31000).is_err());
        let resolved = command.counted(27221, 32512, 32512).expect("recorded fit");
        assert_eq!(resolved.output.reply(), 5290);
        let wire: serde_json::Value = serde_json::from_slice(&resolved.body).expect("wire");
        assert_eq!(wire["max_tokens"], 5290);
    }
}
