#![cfg(test)]
//! Tests for the `continuum` binary, grouped by the responsibility they cover.

use super::*;

#[path = "tests/checkpoint_tests.rs"]
mod checkpoint_tests;
#[path = "tests/core_artifact_tests.rs"]
mod core_artifact_tests;
#[path = "tests/deploy_consume_tests.rs"]
mod deploy_consume_tests;
#[path = "tests/dispatch_tests.rs"]
mod dispatch_tests;
#[path = "tests/processes_tests.rs"]
mod processes_tests;
#[path = "tests/reboot_tests.rs"]
mod reboot_tests;
#[path = "tests/stop_tests.rs"]
mod stop_tests;
#[path = "tests/dark_build_tests.rs"]
mod dark_build_tests;
