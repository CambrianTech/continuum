//! `dataset/*` — import, build, and query training datasets.
//!
//! The six verbs are the rooms→training-data bridge of the coordination↔learning
//! flywheel: recorded/live persona turns and external corpora become the JSONL SFT
//! datasets the forge trains LoRA genomes on. Each is a dep-holding
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) capturing the module's
//! [`DatasetService`](crate::modules::dataset::DatasetService); they are assembled by
//! [`command_objects`] and exposed through `DatasetModule::commands`.
//!
//! Access split: the four writers (import-csv, import-realclasseval, from-turns,
//! from-captures) accept an arbitrary `outputDir` and write datasets to disk → a
//! filesystem-write vector → `Privileged`. The two pure reads (list, info) → `AiSafe`.

use std::sync::Arc;

use crate::modules::dataset::DatasetService;
use crate::sdk_codegen::DynCommand;

pub mod from_captures;
pub mod from_turns;
pub mod import_csv;
pub mod import_realclasseval;
pub mod info;
pub mod list;

/// Build the dep-holding `dataset/*` command objects over the shared
/// [`DatasetService`]. Called from `DatasetModule::commands`.
pub fn command_objects(service: Arc<DatasetService>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(import_csv::DatasetImportCsv {
            service: service.clone(),
        }),
        Arc::new(import_realclasseval::DatasetImportRealClassEval {
            service: service.clone(),
        }),
        Arc::new(from_turns::DatasetFromTurns {
            service: service.clone(),
        }),
        Arc::new(from_captures::DatasetFromCaptures {
            service: service.clone(),
        }),
        Arc::new(list::DatasetList {
            service: service.clone(),
        }),
        Arc::new(info::DatasetInfo { service }),
    ]
}
