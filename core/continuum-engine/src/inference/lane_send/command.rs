//! Immutable prompt controls and the one serializer of resolved output authority.
use crate::ai::{inference_error::InferenceError, reply_room::{OutputIntent, QuotedReplyRoom}};

#[derive(Debug, Clone, Copy)]
pub(crate) enum SentOutput {
    ProviderControlled,
    Counted(QuotedReplyRoom),
    ProviderFixed(std::num::NonZeroU32),
}

impl SentOutput {
    pub fn reply(self) -> Option<u32> {
        match self { Self::Counted(room) => Some(room.reply), Self::ProviderFixed(reply) => Some(reply.get()), Self::ProviderControlled => None }
    }
}

pub(super) struct PreparedCommand { wire: PreparedWire }

enum PreparedWire {
    ProviderControlled { body: Vec<u8> },
    Fixed { reply: std::num::NonZeroU32, body: Vec<u8> },
    Adaptive { required_reply: std::num::NonZeroU32, prompt: serde_json::Map<String, serde_json::Value> },
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
        if matches!(intent, OutputIntent::ProviderControlled) && prompt.contains_key("max_tokens") {
            return Err(InferenceError::Protocol("provider-controlled command contains a fixed output bound".into()));
        }
        if let OutputIntent::Fixed(reply) = intent {
            if let Some(cap) = prompt.get("max_tokens") {
                if cap.as_u64() != Some(u64::from(reply.get())) {
                    return Err(InferenceError::Protocol("prepared fixed output contradicts caller authority".into()));
                }
            }
        }
        // Only adaptive commands defer serialization until their quote resolves.
        // Fixed and provider-controlled commands always own executable wire bytes.
        match intent {
            OutputIntent::ProviderControlled => Ok(Self { wire: PreparedWire::ProviderControlled { body: body.to_vec() } }),
            OutputIntent::Fixed(reply) => {
                let body = if prompt.contains_key("max_tokens") {
                    body.to_vec()
                } else {
                    Self::encode_body(&prompt, reply.get())?
                };
                Ok(Self { wire: PreparedWire::Fixed { reply, body } })
            }
            OutputIntent::UseAdmittedRoom { required_reply } => {
                prompt.remove("max_tokens");
                Ok(Self { wire: PreparedWire::Adaptive { required_reply, prompt } })
            }
        }
    }

    fn intent(&self) -> OutputIntent {
        match &self.wire {
            PreparedWire::ProviderControlled { .. } => OutputIntent::ProviderControlled,
            PreparedWire::Fixed { reply, .. } => OutputIntent::Fixed(*reply),
            PreparedWire::Adaptive { required_reply, .. } => OutputIntent::UseAdmittedRoom { required_reply: *required_reply },
        }
    }

    pub fn quote_builder(&self, template: &reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder, InferenceError> {
        let body = match &self.wire {
            PreparedWire::Adaptive { required_reply, prompt } => Self::encode_body(prompt, required_reply.get())?,
            PreparedWire::ProviderControlled { body } | PreparedWire::Fixed { body, .. } => body.clone(),
        };
        template.try_clone().map(|builder| builder.body(body))
            .ok_or_else(|| InferenceError::Protocol("generation headers are not cloneable".into()))
    }

    pub fn fixed(&self) -> Result<ResolvedCommand, InferenceError> {
        match &self.wire {
            PreparedWire::ProviderControlled { body } => Ok(ResolvedCommand { body: body.clone(), output: SentOutput::ProviderControlled }),
            PreparedWire::Fixed { reply, body } => Ok(ResolvedCommand { body: body.clone(), output: SentOutput::ProviderFixed(*reply) }),
            PreparedWire::Adaptive { .. } => Err(InferenceError::Protocol(
                "adaptive output requires an owned endpoint, exact quote and grant lease".into())),
        }
    }

    pub fn counted(&self, prompt: u32, physical: u32, admitted: u32) -> Result<ResolvedCommand, InferenceError> {
        let room = self.intent().resolve(prompt, physical, admitted)
            .map_err(|short| InferenceError::ContextExceeded { requested: short.prompt, available: short.window })?;
        let body = match &self.wire {
            PreparedWire::ProviderControlled { body } | PreparedWire::Fixed { body, .. } => body.clone(),
            PreparedWire::Adaptive { prompt, .. } => Self::encode_body(prompt, room.reply)?,
        };
        Ok(ResolvedCommand { body, output: SentOutput::Counted(room) })
    }

    fn encode_body(prompt: &serde_json::Map<String, serde_json::Value>, reply: u32) -> Result<Vec<u8>, InferenceError> {
        let mut body = prompt.clone();
        body.insert("max_tokens".into(), serde_json::json!(reply));
        serde_json::to_vec(&body) // wire boundary: generation request sent to the model HTTP endpoint
            .map_err(|e| InferenceError::Protocol(format!("generation serialization failed: {e}")))
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
    fn provider_controlled_preserves_omitted_limit_and_has_no_admitted_cap() {
        let bytes = br#"{"messages":[],"model":"teacher"}"#;
        let command = PreparedCommand::new(bytes, OutputIntent::ProviderControlled).expect("general command");
        let resolved = command.fixed().expect("provider-controlled wire");
        assert_eq!(resolved.body, bytes);
        assert_eq!(resolved.output.reply(), None);
        assert!(command.counted(10, 32768, 32768).is_err());
        assert!(PreparedCommand::new(br#"{"max_tokens":32}"#, OutputIntent::ProviderControlled).is_err());
    }

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
    fn omitted_fixed_limit_is_serialized_once_for_quote_and_send() {
        let command = PreparedCommand::new(br#"{"messages":[],"model":"fixture"}"#,
            OutputIntent::Fixed(NonZeroU32::new(32).expect("positive fixture"))).expect("fixed command");
        let template = reqwest::Client::new().post("http://localhost/generate");
        let quote = command.quote_builder(&template).expect("quote").build().expect("request");
        let sent = command.counted(40, 100, 90).expect("counted command");
        assert_eq!(quote.body().and_then(|body| body.as_bytes()), Some(sent.body.as_slice()));
        let value: serde_json::Value = serde_json::from_slice(&sent.body).expect("wire");
        assert_eq!(value["max_tokens"], 32);
    }

    #[test]
    fn adaptive_intent_cannot_be_encoded_as_an_uncounted_fixed_send() {
        let command = PreparedCommand::new(br#"{"messages":[],"max_tokens":999999}"#,
            OutputIntent::UseAdmittedRoom { required_reply: NonZeroU32::new(4534).expect("positive fixture") }).expect("adaptive command");
        assert!(command.fixed().is_err());
        assert!(command.counted(27221, 32512, 31000).is_err());
        let resolved = command.counted(27221, 32512, 32512).expect("recorded fit");
        assert_eq!(resolved.output.reply(), Some(5290));
        let wire: serde_json::Value = serde_json::from_slice(&resolved.body).expect("wire");
        assert_eq!(wire["max_tokens"], 5290);
    }
}
