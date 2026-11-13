//! Shared test boundary for ORM integration and load fixtures.
use super::*;

#[path = "derive_test.rs"]
mod derive_test;
#[path = "relational_tests.rs"]
mod relational_tests;
#[path = "binary_tests.rs"]
mod binary_tests;
#[path = "binary_load_tests.rs"]
mod binary_load_tests;
