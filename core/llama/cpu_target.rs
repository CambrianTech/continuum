//! CPU instruction contract for binaries distributed beyond their build host.

pub const PORTABLE_ENV: &str = "CONTINUUM_PORTABLE_CPU";

// GGML_NATIVE=OFF alone enables AVX2 by default, and a reused CMake cache may
// still contain AVX512 options from a native build. Explicitly overwrite every
// optional x86 extension. SSE2 is the x86_64 target baseline; no newer CPU floor
// is implied by the hardware on which CI happens to execute.
const X86_EXTENSIONS: &[&str] = &[
    "GGML_SSE42",
    "GGML_AVX",
    "GGML_AVX2",
    "GGML_AVX_VNNI",
    "GGML_BMI2",
    "GGML_FMA",
    "GGML_F16C",
    "GGML_AVX512",
    "GGML_AVX512_VBMI",
    "GGML_AVX512_VNNI",
    "GGML_AVX512_BF16",
    "GGML_AMX_TILE",
    "GGML_AMX_INT8",
    "GGML_AMX_BF16",
];

pub fn definitions(arch: &str, portable: bool) -> Vec<(&'static str, &'static str)> {
    if arch != "x86_64" {
        return Vec::new();
    }
    if !portable {
        // Switching the same Cargo/CMake output back to a local build must not
        // inherit portable mode's disabled host probing.
        return vec![("GGML_NATIVE", "ON")];
    }
    std::iter::once(("GGML_NATIVE", "OFF"))
        .chain(X86_EXTENSIONS.iter().map(|name| (*name, "OFF")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn portable_target_overrides_a_previous_native_cache_without_avx2_floor() {
        let publisher = include_str!("../../.github/workflows/core-binaries.yml");
        assert!(publisher.contains(&format!("{PORTABLE_ENV}: \"1\"")));
        let mut cache: BTreeMap<_, _> = X86_EXTENSIONS.iter().map(|key| (*key, "ON")).collect();
        cache.insert("GGML_NATIVE", "ON");
        cache.insert("GGML_CUDA", "ON");
        cache.extend(definitions("x86_64", true));
        assert_eq!(cache["GGML_NATIVE"], "OFF");
        for extension in X86_EXTENSIONS {
            assert_eq!(cache[extension], "OFF", "{extension}");
        }
        assert_eq!(cache["GGML_CUDA"], "ON");
        assert!(definitions("aarch64", true).is_empty());
        cache.extend(definitions("x86_64", false));
        assert_eq!(cache["GGML_NATIVE"], "ON");
    }
}
