#![cfg(test)]
//! Mind tests that read the REAL command registry or ACL: core's commands register through
//! `inventory`, so only core's test binary has them (the mind's own binary sees an empty
//! registry). Each asserts exactly what it asserted before the move.

mod act_observe;
mod deliberation;
mod persona_tools;
mod tool_executor;
mod experience;
mod training_acl;
