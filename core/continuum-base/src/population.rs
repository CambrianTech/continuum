//! The population the operator declared for this node.

/// The minds this node hosts (`CONTINUUM_PERSONA_FLOOR`): the serving plan's lane demand, and the
/// floor no bound draws the roster below. One owner for every reader (boot, the spawner's draw,
/// the prompt cache). Undeclared, the population is one mind: a node that hosts any mind hosts at
/// least one.
pub fn persona_floor() -> usize {
    crate::config_env::read("CONTINUUM_PERSONA_FLOOR")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(1) // unwrap_or: undeclared = one mind, the least a hosting node has
}
