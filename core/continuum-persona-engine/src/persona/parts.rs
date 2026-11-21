//! Her five parts as one value, so a persona is moved only whole.

/// Everything one persona's execution holds: her KV blocks, her tokens, her genome, her learning
/// state and her decode handle. A [`PersonaUnit`](super::PersonaUnit) is built only from all five.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaParts<Blocks, Tokens, Genome, Learning, Decode> {
    pub blocks: Blocks,
    pub tokens: Tokens,
    pub genome: Genome,
    pub learning: Learning,
    pub decode: Decode,
}
