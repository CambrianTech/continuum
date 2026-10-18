#![cfg(test)]
//! Tests for the `benchmark/*` command family, split by the module each exercises.

use super::*;
use crate::cognition::swe_bench::SweVerdict;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

mod dispatch;
mod swe_grade;
mod swe_setup;
