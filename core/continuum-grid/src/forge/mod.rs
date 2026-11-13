//! The forge custodian's discovery row and wire contract, the part a grid node advertises
//! (`grid::node::GridNode::forge`). Core's `forge` re-exports these under their old paths.

pub mod custodian_client;
pub mod endpoint;
pub mod protocol;
