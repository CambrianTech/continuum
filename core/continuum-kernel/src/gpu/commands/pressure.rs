//! `gpu/pressure` — quick GPU pressure query (0.0–1.0), the cheap version of
//! `gpu/stats` when all you need is the headroom signal.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::gpu::GpuMemoryManager;

/// `gpu/pressure` takes no input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuPressureParams.ts"
)]
pub struct GpuPressureParams {}

/// Just the GPU memory pressure, 0.0 (idle) to 1.0 (saturated).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/gpu/GpuPressureResult.ts"
)]
pub struct GpuPressureResult {
    /// Current GPU memory pressure across all subsystems, 0.0–1.0.
    #[ts(type = "number")]
    pub pressure: f32,
}

crate::action_command! {
    /// Quick GPU memory pressure read, 0.0 (idle) to 1.0 (saturated). Cheaper than
    /// `gpu/stats` when you only need the headroom signal — e.g. to decide whether
    /// to defer a heavy job.
    pub struct GpuPressureCmd { manager: Arc<GpuMemoryManager> }
    name: "gpu/pressure",
    access: AiSafe,
    params: GpuPressureParams,
    output: GpuPressureResult,
    run(this, _ctx, _p) => {
        Ok(GpuPressureResult { pressure: this.manager.pressure() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — pressure is a harmless read, so AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GpuPressureCmd::NAME, "gpu/pressure");
        assert!(matches!(
            GpuPressureCmd::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: pressure stays in the documented 0.0–1.0 band — a caller
    // gating on it would misbehave if it ever returned out-of-range.
    #[tokio::test]
    async fn pressure_in_unit_range() {
        let cmd = GpuPressureCmd {
            manager: Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53_000_000_000)),
        };
        let r = cmd
            .run(&Ctx::default(), GpuPressureParams {})
            .await
            .unwrap();
        assert!(r.pressure >= 0.0 && r.pressure <= 1.0);
    }
}
