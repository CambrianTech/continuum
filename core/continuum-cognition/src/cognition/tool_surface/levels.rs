//! Each verb's levels of detail, generated from its schema's shape (the context codec's contract,
//! MIND-CONTEXT "one codec for every layer"): her hands are a budget consumer like any other, so a
//! small window shows a verb smaller instead of refusing the turn.
//!
//! - **whole**: the schema as built;
//! - **summary**: its description at the codec's summary level and every parameter with its type,
//!   without the parameters' own descriptions;
//! - **pointer**: its name, every parameter's name (any value), and the expand handle naming the
//!   verb that shows its whole (`commands/help`, [`crate::cognition::tool_head::FINDER`]).
//!
//! Every level keeps every parameter: the served engine builds its tool-call grammar and parser
//! over the head's schemas, and a verb loaded later in the tail never replaces a head verb of the
//! same name (the fork's `common/chat.cpp`, `common_chat_templates_apply`: a tail tool already in
//! the head is skipped). A level that dropped a parameter would make it uncallable until her
//! head's next epoch. A verb the expand handle cannot open (the turn's own verdict verbs, which
//! declare `Always` and have no command) floors at its summary, never at a pointer nothing expands.
//!
//! Every cost is measured from the rendered level ([`super::tool_surface_tokens_of`]); a level no
//! smaller than the one above it is not a level ([`continuum_context::ladder::Rungs::new`]).

use continuum_context::ladder::{Leveled, Rungs};

use super::tool_surface_tokens_of;
use crate::ai::types::{NativeToolSpec, ToolInputSchema};

/// Where a verb's levels end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Floor {
    /// Its pointer: the expand handle opens it.
    Pointer,
    /// Its summary: nothing expands a pointer of it.
    Summary,
}

/// One verb at each level it can be sent at, most detail first, each with its measured cost.
#[derive(Clone, Debug)]
pub struct VerbLevels {
    levels: Vec<(NativeToolSpec, usize)>,
}

impl VerbLevels {
    /// `spec`'s levels; `canonical` is its command name, `finder` the expand verb's wire name.
    pub fn of(spec: &NativeToolSpec, canonical: &str, finder: &str, floor: Floor) -> Self {
        let mut levels = vec![spec.clone(), summary(spec)];
        if floor == Floor::Pointer {
            levels.push(pointer(spec, canonical, finder));
        }
        let mut kept: Vec<(NativeToolSpec, usize)> = Vec::new();
        for level in levels {
            let cost = tool_surface_tokens_of(std::slice::from_ref(&level));
            if kept.last().is_none_or(|(_, above)| cost < *above) {
                kept.push((level, cost));
            }
        }
        Self { levels: kept }
    }

    pub fn spec(&self, level: usize) -> &NativeToolSpec {
        &self.levels[level].0
    }

    pub fn cost(&self, level: usize) -> usize {
        self.levels[level].1
    }

    pub fn len(&self) -> usize {
        self.levels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }
}

/// The verb's summary: its description at the codec's summary level when it has one
/// (`continuum_context::levels::summarize`, its head and tail with what lies between counted),
/// every parameter with its type and none of the parameters' descriptions.
fn summary(spec: &NativeToolSpec) -> NativeToolSpec {
    let properties = match spec.input_schema.properties.as_object() {
        Some(props) => serde_json::Value::Object(
            props
                .iter()
                .map(|(name, schema)| {
                    let mut schema = schema.clone();
                    if let Some(fields) = schema.as_object_mut() {
                        fields.remove("description");
                    }
                    (name.clone(), schema)
                })
                .collect(),
        ),
        None => spec.input_schema.properties.clone(),
    };
    NativeToolSpec {
        name: spec.name.clone(),
        description: continuum_context::levels::summarize(&spec.description, continuum_context::levels::Shape::Output).unwrap_or_else(|| spec.description.clone()), // unwrap_or_else: a description with no smaller level is its own summary
        input_schema: ToolInputSchema { properties, ..spec.input_schema.clone() },
    }
}

/// The verb's pointer: its name, its parameters' names (any value), and how to open its whole.
fn pointer(spec: &NativeToolSpec, canonical: &str, finder: &str) -> NativeToolSpec {
    let names = spec.input_schema.properties.as_object().map(|props| props.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(); // unwrap_or_default: a verb with no parameters names none
    let properties = serde_json::Value::Object(names.into_iter().map(|name| (name, serde_json::json!({}))).collect());
    NativeToolSpec {
        name: spec.name.clone(),
        description: expand_handle(finder, canonical),
        input_schema: ToolInputSchema { schema_type: spec.input_schema.schema_type.clone(), properties, required: spec.input_schema.required.clone(), definitions: None },
    }
}

/// The one rendering of a verb's expand handle.
pub fn expand_handle(finder: &str, canonical: &str) -> String {
    format!("{finder}(name=\"{canonical}\") shows its parameters")
}

/// This act's verbs, each at its levels, in the order they are sent (canonical, as held), with the
/// order their detail is given up in (least important first).
#[derive(Clone, Debug, Default)]
pub struct ToolSet {
    /// Canonical command names, in sent order.
    pub names: Vec<String>,
    levels: Vec<VerbLevels>,
    /// Indices into `names`, least important first: the items' order in the allocation.
    importance: Vec<usize>,
}

/// The tool set as the allocation's consumer, and which level of each verb each rung is (a level
/// whose charged cost is no smaller than the one above it is not a rung).
pub struct ToolsLeveled {
    pub leveled: Leveled,
    /// Per item (importance order), the verb level of each rung.
    rung_levels: Vec<Vec<usize>>,
}

impl ToolSet {
    /// `entries` in sent order: (canonical name, levels, importance rank: higher keeps detail longer).
    pub fn new(entries: Vec<(String, VerbLevels, u8)>) -> Self {
        let mut importance: Vec<usize> = (0..entries.len()).collect();
        importance.sort_by_key(|&i| (entries[i].2, i));
        let (names, levels): (Vec<String>, Vec<VerbLevels>) = entries.into_iter().map(|(name, levels, _)| (name, levels)).unzip();
        Self { names, levels, importance }
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The set as a consumer in the allocation's unit (`charge`).
    pub fn leveled(&self, charge: impl Fn(usize) -> u32) -> ToolsLeveled {
        let mut items = Vec::with_capacity(self.importance.len());
        let mut rung_levels = Vec::with_capacity(self.importance.len());
        for &i in &self.importance {
            let verb = &self.levels[i];
            let mut costs: Vec<u32> = Vec::new();
            let mut map: Vec<usize> = Vec::new();
            for level in 0..verb.len() {
                let cost = charge(verb.cost(level));
                if costs.last().is_none_or(|&above| cost < above) {
                    costs.push(cost);
                    map.push(level);
                }
            }
            items.push(Rungs::new(costs).unwrap_or_else(|| Rungs::whole(0))); // unwrap_or_else: a verb always has its whole level
            rung_levels.push(map);
        }
        ToolsLeveled { leveled: Leveled::new(items), rung_levels }
    }

    /// The verb levels `rungs` (one per item, importance order) name, as sent-order levels.
    pub fn levels_of(&self, leveled: &ToolsLeveled, rungs: &[usize]) -> Vec<usize> {
        let mut levels = vec![0; self.names.len()];
        for ((&i, map), &rung) in self.importance.iter().zip(&leveled.rung_levels).zip(rungs) {
            levels[i] = map.get(rung).copied().unwrap_or(0); // unwrap_or: a rung past the map is the whole
        }
        levels
    }

    /// The rungs (importance order) that sent-order `levels` are, when every level is a rung.
    pub fn rungs_of(&self, leveled: &ToolsLeveled, levels: &[usize]) -> Option<Vec<usize>> {
        if levels.len() != self.names.len() {
            return None;
        }
        self.importance.iter().zip(&leveled.rung_levels).map(|(&i, map)| map.iter().position(|&level| level == levels[i])).collect()
    }

    /// The schemas at sent-order `levels`, and what they cost.
    pub fn render(&self, levels: &[usize]) -> (Vec<NativeToolSpec>, usize) {
        let specs: Vec<NativeToolSpec> = self.levels.iter().zip(levels).map(|(verb, &level)| verb.spec(level.min(verb.len() - 1)).clone()).collect();
        let tokens = self.levels.iter().zip(levels).map(|(verb, &level)| verb.cost(level.min(verb.len() - 1))).sum();
        (specs, tokens)
    }

    /// Every verb whole.
    pub fn whole(&self) -> Vec<usize> {
        vec![0; self.names.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> NativeToolSpec {
        NativeToolSpec {
            name: "edit_file".into(),
            description: "Edit a file in place. Replaces the exact text you name with new text, and refuses when the old text is absent or ambiguous.".into(),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: serde_json::json!({
                    "file_path": { "type": "string", "description": "The path of the file, relative to the worktree root." },
                    "old_string": { "type": "string", "description": "The exact text to replace; it must occur once." },
                    "new_string": { "type": "string", "description": "The text it becomes." },
                    "replace_all": { "type": "boolean", "description": "Replace every occurrence instead of exactly one." }
                }),
                required: Some(vec!["file_path".into(), "old_string".into(), "new_string".into()]),
                definitions: None,
            },
        }
    }

    // what this catches: a collapsed verb that can no longer be called with one of its arguments
    // (the engine's grammar is built over the head; a tail never replaces it), a pointer that does
    // not say how to open it, or a level no smaller than the one above. Each level is smaller,
    // every level keeps every parameter name and the required list, and the pointer names the
    // expand verb with this verb's command name.
    #[test]
    fn each_level_is_smaller_keeps_every_parameter_and_the_pointer_names_its_expand_handle() {
        let levels = VerbLevels::of(&spec(), "code/edit", "commands_help", Floor::Pointer);
        assert_eq!(levels.len(), 3);
        assert!(levels.cost(0) > levels.cost(1) && levels.cost(1) > levels.cost(2), "{} {} {}", levels.cost(0), levels.cost(1), levels.cost(2));
        for level in 0..levels.len() {
            let level = levels.spec(level);
            let names: Vec<&String> = level.input_schema.properties.as_object().map(|p| p.keys().collect()).unwrap_or_default(); // JUSTIFIED: test fixture
            assert_eq!(names.len(), 4, "every parameter at every level: {level:?}");
            assert_eq!(level.input_schema.required, spec().input_schema.required);
        }
        assert!(levels.spec(1).description.len() <= spec().description.len(), "the summary's description is no longer than the whole's");
        assert_eq!(levels.spec(2).description, expand_handle("commands_help", "code/edit"));
        let verdict = VerbLevels::of(&spec(), "conclude", "commands_help", Floor::Summary);
        assert_eq!(verdict.len(), 2, "a verb nothing expands floors at its summary");
    }

    // what this catches: the wire order moving with the order detail is given up in (every act's
    // head bytes would move), or a rung mapped to the wrong verb level after charging.
    #[test]
    fn levels_map_back_to_sent_order_through_the_importance_order() {
        let verb = || VerbLevels::of(&spec(), "code/edit", "commands_help", Floor::Pointer);
        let set = ToolSet::new(vec![("a".into(), verb(), 2), ("b".into(), verb(), 0), ("c".into(), verb(), 1)]);
        let leveled = set.leveled(|tokens| tokens as u32);
        let rungs = leveled.leveled.reduce(leveled.leveled.hard_limit());
        let levels = set.levels_of(&leveled, &rungs.rungs);
        assert_eq!(levels, [2, 2, 2]);
        let one_up = leveled.leveled.reduce(leveled.leveled.hard_limit() + (verb().cost(0) - verb().cost(2)) as u32);
        let levels = set.levels_of(&leveled, &one_up.rungs);
        assert_eq!(levels, [0, 2, 2], "the most important verb (a) rises first, in its sent place");
        assert_eq!(set.rungs_of(&leveled, &levels), Some(one_up.rungs.clone()));
        let (specs, tokens) = set.render(&levels);
        assert_eq!(specs.len(), 3);
        assert_eq!(tokens, verb().cost(0) + 2 * verb().cost(2));
    }
}
