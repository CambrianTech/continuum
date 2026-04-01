//! Grid command name constants — the ONE place command paths are defined.
//!
//! Every reference to a grid command name MUST use these constants.
//! If a command moves or gets renamed, change it HERE and the compiler
//! catches every broken reference.
//!
//! This pattern should be adopted by EVERY module in the codebase.
//! Long-term: generate these from command specs (like generate-command-constants.ts does for TS).

use crate::runtime::{CommandSchema, ParamSchema};

// ============================================================================
// Command name constants
// ============================================================================

pub const STATUS:   &str = "grid/status";
pub const NODES:    &str = "grid/nodes";
pub const PING:     &str = "grid/ping";
pub const SEND:     &str = "grid/send";
pub const DISCOVER: &str = "grid/discover";
pub const PAIR:     &str = "grid/pair";
pub const TRUST:    &str = "grid/trust";
pub const AUDIT:    &str = "grid/audit";
pub const ROUTE:       &str = "grid/route";
pub const NODE_STATUS: &str = "grid/node-status";
pub const JOB_SUBMIT:  &str = "grid/job-submit";
pub const JOB_CONTROL: &str = "grid/job-control";
pub const JOB_QUEUE:   &str = "grid/job-queue";

// ============================================================================
// Command schemas (defined alongside their names — no duplication)
// ============================================================================

/// All grid command schemas. Used by command_schemas() in the ServiceModule impl
/// and by MCP for dynamic tool discovery.
pub fn schemas() -> Vec<CommandSchema> {
    vec![
        CommandSchema {
            name: STATUS,
            description: "Grid transport status and local node identity",
            params: vec![],
        },
        CommandSchema {
            name: NODES,
            description: "List known Grid nodes with capabilities and trust levels",
            params: vec![],
        },
        CommandSchema {
            name: PING,
            description: "Measure round-trip latency to a remote node",
            params: vec![ParamSchema {
                name: "nodeId",
                param_type: "string",
                required: true,
                description: "Node ID to ping",
            }],
        },
        CommandSchema {
            name: SEND,
            description: "Execute a command on a remote Grid node",
            params: vec![
                ParamSchema { name: "nodeId",  param_type: "string", required: true,  description: "Target node ID" },
                ParamSchema { name: "command", param_type: "string", required: true,  description: "Command to execute remotely" },
                ParamSchema { name: "params",  param_type: "object", required: false, description: "Command parameters" },
            ],
        },
        CommandSchema {
            name: DISCOVER,
            description: "Trigger transport-level node discovery",
            params: vec![],
        },
        CommandSchema {
            name: PAIR,
            description: "Pair with a new node by address",
            params: vec![
                ParamSchema { name: "address", param_type: "string", required: true,  description: "Transport address (e.g., Tailscale IP)" },
                ParamSchema { name: "name",    param_type: "string", required: false, description: "Friendly name for the node" },
                ParamSchema { name: "trust",   param_type: "string", required: false, description: "Trust level: blocked, provisional, trusted, owner" },
            ],
        },
        CommandSchema {
            name: TRUST,
            description: "Update trust level for a known node",
            params: vec![
                ParamSchema { name: "nodeId", param_type: "string", required: true, description: "Node ID to update" },
                ParamSchema { name: "trust",  param_type: "string", required: true, description: "New trust level: blocked, provisional, trusted, owner" },
            ],
        },
        CommandSchema {
            name: AUDIT,
            description: "View remote command audit trail",
            params: vec![ParamSchema { name: "limit", param_type: "number", required: false, description: "Number of entries (default 50)" }],
        },
        CommandSchema {
            name: ROUTE,
            description: "Check where a command would be routed (dry run)",
            params: vec![
                ParamSchema { name: "command",     param_type: "string", required: true,  description: "Command to check routing for" },
                ParamSchema { name: "routingHint", param_type: "string", required: false, description: "Routing hint to test" },
            ],
        },
        CommandSchema {
            name: NODE_STATUS,
            description: "Query node GPU utilization, running jobs, queue depth, temperature",
            params: vec![
                ParamSchema { name: "nodeId", param_type: "string", required: false, description: "Target node (omit for local)" },
            ],
        },
        CommandSchema {
            name: JOB_SUBMIT,
            description: "Submit a forge job to the local queue",
            params: vec![
                ParamSchema { name: "alloy",    param_type: "object", required: true,  description: "Complete alloy JSON recipe" },
                ParamSchema { name: "priority", param_type: "number", required: false, description: "Queue priority 0-10 (default: 5)" },
            ],
        },
        CommandSchema {
            name: JOB_CONTROL,
            description: "Control a running forge job: pause, resume, cancel",
            params: vec![
                ParamSchema { name: "jobId",  param_type: "string", required: true,  description: "Job ID to control" },
                ParamSchema { name: "action", param_type: "string", required: true,  description: "pause, resume, or cancel" },
            ],
        },
        CommandSchema {
            name: JOB_QUEUE,
            description: "List forge jobs with state and progress",
            params: vec![
                ParamSchema { name: "state", param_type: "string", required: false, description: "Filter: queued, running, paused, completed, failed, all" },
                ParamSchema { name: "limit", param_type: "number", required: false, description: "Max jobs to return (default: 20)" },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_commands_have_grid_prefix() {
        let all = [STATUS, NODES, PING, SEND, DISCOVER, PAIR, TRUST, AUDIT, ROUTE,
                   NODE_STATUS, JOB_SUBMIT, JOB_CONTROL, JOB_QUEUE];
        for cmd in &all {
            assert!(cmd.starts_with("grid/"), "Command {cmd} missing grid/ prefix");
        }
    }

    #[test]
    fn test_schemas_match_constants() {
        let schemas = schemas();
        let all = [STATUS, NODES, PING, SEND, DISCOVER, PAIR, TRUST, AUDIT, ROUTE,
                   NODE_STATUS, JOB_SUBMIT, JOB_CONTROL, JOB_QUEUE];
        assert_eq!(schemas.len(), all.len(), "Schema count mismatch");
        for (schema, constant) in schemas.iter().zip(all.iter()) {
            assert_eq!(schema.name, *constant, "Schema name doesn't match constant");
        }
    }
}
