//! The verbs a mind can call, each with the price of its schema in the head.

use std::collections::HashMap;

/// A verb, as an index into its [`Vocabulary`]: copied, compared and used as a slot index,
/// never a string. The canonical command name (`code/read`) is its rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolId(u16);

impl ToolId {
    /// The slot this tool occupies in every per-tool vector.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// One verb's schema as the registry describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSchema {
    /// Canonical command name (`code/read`).
    pub name: String,
    /// What the verb does: the text a relevance model reads.
    pub description: String,
    /// What the schema costs in the head, in prompt tokens.
    pub tokens: u32,
}

/// Every verb, in canonical (name) order, so ids are stable for one registry.
#[derive(Debug, Clone, Default)]
pub struct Vocabulary {
    schemas: Vec<ToolSchema>,
    by_name: HashMap<String, ToolId>,
}

impl Vocabulary {
    /// The vocabulary of `schemas`. A name listed twice keeps its first schema.
    pub fn new(mut schemas: Vec<ToolSchema>) -> Result<Self, String> {
        schemas.sort_by(|a, b| a.name.cmp(&b.name));
        schemas.dedup_by(|later, earlier| later.name == earlier.name);
        if schemas.len() > usize::from(u16::MAX) {
            return Err(format!("{} verbs do not fit a u16 tool id", schemas.len()));
        }
        let by_name = schemas
            .iter()
            .enumerate()
            .map(|(index, schema)| (schema.name.clone(), ToolId(index as u16)))
            .collect();
        Ok(Self { schemas, by_name })
    }

    /// How many verbs there are: the length of every per-tool vector.
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    /// The verb named `name`, when the registry has it.
    pub fn id(&self, name: &str) -> Option<ToolId> {
        self.by_name.get(name).copied()
    }

    pub fn schema(&self, tool: ToolId) -> &ToolSchema {
        &self.schemas[tool.index()]
    }

    pub fn name(&self, tool: ToolId) -> &str {
        &self.schema(tool).name
    }

    pub fn tokens(&self, tool: ToolId) -> u32 {
        self.schema(tool).tokens
    }

    /// Every verb, in id order.
    pub fn ids(&self) -> impl Iterator<Item = ToolId> + '_ {
        (0..self.schemas.len()).map(|index| ToolId(index as u16))
    }

    /// What a set of verbs costs in the head.
    pub fn tokens_of(&self, tools: &[ToolId]) -> u64 {
        tools.iter().map(|tool| u64::from(self.tokens(*tool))).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(name: &str, tokens: u32) -> ToolSchema {
        ToolSchema { name: name.to_owned(), description: String::new(), tokens }
    }

    // what this catches: ids depending on the order the registry listed its verbs, which
    // would make a head chosen on one load mean different verbs on the next.
    #[test]
    fn ids_follow_canonical_name_order_whatever_the_input_order() {
        let forward = Vocabulary::new(vec![schema("code/read", 3), schema("work/get", 5), schema("code/edit", 7)]);
        let backward = Vocabulary::new(vec![schema("code/edit", 7), schema("work/get", 5), schema("code/read", 3)]);
        let (forward, backward) = (forward.expect("small vocabulary"), backward.expect("small vocabulary"));
        for name in ["code/edit", "code/read", "work/get"] {
            assert_eq!(forward.id(name), backward.id(name), "{name}");
        }
        let edit = forward.id("code/edit").expect("listed");
        assert_eq!((edit.index(), forward.tokens(edit)), (0, 7));
        assert_eq!(forward.id("frobnicate"), None);
    }
}
