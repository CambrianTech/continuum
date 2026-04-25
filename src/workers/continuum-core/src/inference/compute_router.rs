//! ComputeRouter — routes ops to CPU SIMD or GPU based on kernel size and chip tier.
//!
//! Same principle as routing convolutions by kernel size in vision:
//! small ops → CPU (SIMD/BLAS), large ops → GPU (Metal/CUDA).
//! Calibrated per chip family at startup. Every model uses the same router.

use candle_core::Device;

/// Hardware tier — determines dispatch thresholds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChipTier {
    /// M1-M3: higher Metal dispatch overhead, NEON SIMD strong
    AppleSilicon,
    /// M4-M5: Metal4 tensor API, lower dispatch overhead, BF16 native
    AppleSiliconAdvanced,
    /// NVIDIA GPU: very low dispatch overhead, massive parallelism
    Cuda,
    /// CPU only (no GPU available)
    CpuOnly,
}

/// What device to run an op on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComputeTarget {
    Cpu,
    Gpu,
}

/// Op shape descriptor — enough to decide routing.
#[derive(Debug, Clone, Copy)]
pub struct OpShape {
    /// Total FLOPs (approximate) — m*k*n for matmul, elements for elementwise
    pub flops: usize,
    /// Whether the op is a matmul (benefits from parallelism at scale)
    pub is_matmul: bool,
    /// Whether the op is part of a sequential recurrence (many small dispatches)
    pub is_sequential: bool,
}

impl OpShape {
    /// Matmul: m×k×n
    pub fn matmul(m: usize, k: usize, n: usize) -> Self {
        Self {
            flops: m * k * n,
            is_matmul: true,
            is_sequential: false,
        }
    }

    /// Elementwise op on n elements
    pub fn elementwise(n: usize) -> Self {
        Self {
            flops: n,
            is_matmul: false,
            is_sequential: false,
        }
    }

    /// Sequential recurrence step (small matmul inside a loop)
    pub fn recurrence_step(m: usize, k: usize, n: usize) -> Self {
        Self {
            flops: m * k * n,
            is_matmul: true,
            is_sequential: true,
        }
    }
}

/// Thresholds per chip tier — FLOP count below which CPU wins.
/// These should be calibrated empirically per chip.
struct Thresholds {
    /// Matmul FLOP threshold: below this, CPU SIMD is faster
    matmul_cpu_ceiling: usize,
    /// Sequential ops always go to CPU (dispatch overhead dominates)
    sequential_always_cpu: bool,
}

impl Thresholds {
    fn for_tier(tier: ChipTier) -> Self {
        match tier {
            ChipTier::AppleSilicon => Self {
                matmul_cpu_ceiling: 500_000, // ~128×128×32 = 524K → CPU
                sequential_always_cpu: true, // DeltaNet recurrence → always CPU
            },
            ChipTier::AppleSiliconAdvanced => Self {
                matmul_cpu_ceiling: 100_000, // M4/M5: lower dispatch overhead
                sequential_always_cpu: true, // Even on M5, sequential → CPU (benchmark may override)
            },
            ChipTier::Cuda => Self {
                matmul_cpu_ceiling: 50_000,   // CUDA: very low dispatch overhead
                sequential_always_cpu: false, // CUDA can handle sequential with fused kernels
            },
            ChipTier::CpuOnly => Self {
                matmul_cpu_ceiling: usize::MAX,
                sequential_always_cpu: true,
            },
        }
    }
}

/// The router. Created once at model load, used for every op.
#[derive(Debug, Clone)]
pub struct ComputeRouter {
    tier: ChipTier,
    gpu_device: Option<Device>,
}

impl ComputeRouter {
    /// Detect chip tier from the device.
    pub fn new(device: &Device) -> Self {
        let tier = Self::detect_tier(device);
        let gpu_device = if matches!(tier, ChipTier::CpuOnly) {
            None
        } else {
            Some(device.clone())
        };
        Self { tier, gpu_device }
    }

    pub fn tier(&self) -> ChipTier {
        self.tier
    }

    pub fn gpu_device(&self) -> Option<&Device> {
        self.gpu_device.as_ref()
    }

    /// Route an op to CPU or GPU.
    pub fn route(&self, op: &OpShape) -> ComputeTarget {
        let thresholds = Thresholds::for_tier(self.tier);

        // Sequential recurrence ops: CPU unless CUDA with fused kernels
        if op.is_sequential && thresholds.sequential_always_cpu {
            return ComputeTarget::Cpu;
        }

        // Size-based routing
        if op.flops < thresholds.matmul_cpu_ceiling {
            ComputeTarget::Cpu
        } else {
            ComputeTarget::Gpu
        }
    }

    fn detect_tier(device: &Device) -> ChipTier {
        match device {
            Device::Cpu => ChipTier::CpuOnly,
            #[cfg(feature = "cuda")]
            Device::Cuda(_) => ChipTier::Cuda,
            #[cfg(feature = "metal")]
            Device::Metal(_) => {
                // Detect M4/M5 vs M1-M3
                // M4+ has MTLGPUFamilyMetal4, Apple10+
                // For now: check env override or default to conservative
                if std::env::var("CANDLE_METAL_ADVANCED").is_ok() {
                    ChipTier::AppleSiliconAdvanced
                } else {
                    // TODO: probe device.supportsFamily(.metal4) via objc
                    ChipTier::AppleSilicon
                }
            }
            #[allow(unreachable_patterns)]
            _ => ChipTier::CpuOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_matmul_routes_to_cpu() {
        let router = ComputeRouter {
            tier: ChipTier::AppleSilicon,
            gpu_device: None,
        };
        // 128×128×128 = 2M flops — above 500K but let's test smaller
        let op = OpShape::matmul(32, 128, 32); // 131K flops
        assert_eq!(router.route(&op), ComputeTarget::Cpu);
    }

    #[test]
    fn large_matmul_routes_to_gpu() {
        let router = ComputeRouter {
            tier: ChipTier::AppleSilicon,
            gpu_device: None,
        };
        let op = OpShape::matmul(2560, 8192, 1); // 21M flops
        assert_eq!(router.route(&op), ComputeTarget::Gpu);
    }

    #[test]
    fn sequential_always_cpu_on_apple() {
        let router = ComputeRouter {
            tier: ChipTier::AppleSiliconAdvanced,
            gpu_device: None,
        };
        let op = OpShape::recurrence_step(128, 128, 128); // 2M flops, but sequential
        assert_eq!(router.route(&op), ComputeTarget::Cpu);
    }

    #[test]
    fn cuda_handles_sequential() {
        let router = ComputeRouter {
            tier: ChipTier::Cuda,
            gpu_device: None,
        };
        let op = OpShape::recurrence_step(128, 128, 128);
        assert_eq!(router.route(&op), ComputeTarget::Gpu); // CUDA has fused kernels
    }
}
