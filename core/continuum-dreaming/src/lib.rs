//! continuum-dreaming: gene training on the served weights, one domain with its commands
//! (MODULE-ARCHITECTURE.md section 9.2), cut out of continuum-core (card 2a69e6ba). It builds on the
//! mind and everything below it; core re-exports each module under its old path.

pub use continuum_mind::*;

pub mod modules {
    pub mod dataset;
    pub mod dream_stage;
    pub mod gene_examiner;
    pub mod genome;
    pub mod genome_fitness_sentinel;
    pub mod training_trigger;
}

pub mod commands {
    pub mod dataset;
    pub mod genome;
    pub mod genome_recall;
    pub mod genome_share;
    pub mod training_trigger;
    pub use continuum_academy::hf;
}
