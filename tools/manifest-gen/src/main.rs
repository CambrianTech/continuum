//! manifest-gen — the ONE truth (install-manifest.toml) projected to per-platform
//! runner data files. Runners SOURCE these; they never parse TOML.
//!
//! Why a generator (not runtime parsing): PowerShell 5.1 has no TOML parser, and
//! hand-rolling one per-platform is the exact "horrible idea" this avoids. Same
//! precedent as ts-rs / vendor-views: author once, project mechanically, CI
//! drift-checks the projection. Approved on airc (M5, 2026-07-24) with three pins:
//!   1. generated files carry DO-NOT-EDIT headers naming this generator + source
//!   2. `--check` drift mode: regenerate in memory, diff on-disk, fail loud
//!   3. this generator is itself a dev-tier manifest module (toolchain bootstraps it)
//!
//! Usage:
//!   manifest-gen           write the generated files
//!   manifest-gen --check   exit 1 if any on-disk file differs from a fresh render
//!
//! Ownership: the Windows PS projection is validated end-to-end on a live RTX 5090
//! (BigMama). The unix .sh projection is a first-cut shape (flat assoc arrays) for
//! M5's bash runner to consume/reshape — she owns that consumer.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::exit;

// ─── Typed model of the manifest (no `any`; every field the manifest can carry) ──

#[derive(Debug, Deserialize)]
struct Manifest {
    #[allow(dead_code)]
    schema_version: u32,
    #[serde(rename = "module", default)]
    modules: Vec<Module>,
}

#[derive(Debug, Deserialize)]
struct Module {
    id: String,
    order: u32,
    tier: u32,
    #[serde(default)]
    flags: Vec<String>,
    #[serde(default)]
    applies: Option<String>,
    #[serde(default)]
    platforms: Vec<String>,
    #[serde(default)]
    accept: Option<String>,
    /// os ("windows"|"macos"|"linux") -> install source. Ordered for determinism.
    #[serde(default)]
    sources: BTreeMap<String, Source>,
    /// os -> build spec (build-core only).
    #[serde(default)]
    build: BTreeMap<String, BuildSpec>,
}

/// A per-OS install source. One flat struct covers every `type` in the schema
/// (archive|redist|winget|brew|apt|curl-sh|command|system) — the runner reads
/// only the keys its type uses. Field emit order below is FIXED for deterministic
/// output (drift-check depends on byte-stability).
#[derive(Debug, Deserialize)]
struct Source {
    #[serde(rename = "type")]
    kind: String,
    // archive
    url: Option<String>,
    version: Option<String>,
    sha256: Option<String>,
    extract: Option<String>,
    // redist
    manifest: Option<String>,
    components: Option<Vec<String>>,
    // winget
    id: Option<String>,
    scope: Option<String>,
    #[serde(rename = "override")]
    r#override: Option<String>,
    // brew
    formula: Option<String>,
    // apt
    package: Option<String>,
    // curl-sh
    args: Option<String>,
    // command
    run: Option<String>,
    // per-OS accept overrides (evidence is OS-shaped, e.g. libclang.dll vs .dylib)
    accept_macos: Option<String>,
    accept_linux: Option<String>,
    accept_windows: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BuildSpec {
    features: Option<String>,
    profile: Option<String>,
    crt: Option<String>,
    cmake_generator: Option<String>,
    cuda_arch: Option<String>,
    msvc_host: Option<String>,
}

// ─── Paths (pinned to the crate location, independent of cwd) ────────────────────

fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/install-manifest.toml")
}
fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/generated")
}
const REL_SOURCE: &str = "tools/scripts/install-manifest.toml";
const REL_GEN: &str = "tools/manifest-gen";

// ─── Escapers (byte-stable, per shell) ───────────────────────────────────────────

/// PowerShell single-quoted literal: only `'` needs escaping (doubled).
fn ps(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
/// Bash single-quoted literal: close, escaped-quote, reopen.
fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ─── Ordering ────────────────────────────────────────────────────────────────────

fn modules_for<'a>(m: &'a Manifest, os: &str) -> Vec<&'a Module> {
    let mut v: Vec<&Module> = m
        .modules
        .iter()
        .filter(|x| x.platforms.iter().any(|p| p == os))
        .collect();
    v.sort_by_key(|x| x.order);
    v
}

// ─── Windows PowerShell projection ───────────────────────────────────────────────

fn header_ps() -> String {
    format!(
        "# ==============================================================================\n\
         # GENERATED FILE - DO NOT EDIT.\n\
         # Rendered by `{gen}` (cargo run -p manifest-gen) from `{src}`.\n\
         # The manifest is the ONE source of truth; this is a mechanical projection.\n\
         # Edit the manifest and regenerate. CI drift-check (`manifest-gen --check`)\n\
         # fails loud if this file is stale or hand-edited.\n\
         # ==============================================================================\n\n",
        gen = REL_GEN,
        src = REL_SOURCE
    )
}

/// Emit the fixed-order source fields as `key = <ps-literal>;` pairs.
fn ps_source_body(s: &Source) -> String {
    let mut parts: Vec<String> = vec![format!("type = {}", ps(&s.kind))];
    let kv = |k: &str, v: &Option<String>, parts: &mut Vec<String>| {
        if let Some(val) = v {
            parts.push(format!("{k} = {}", ps(val)));
        }
    };
    kv("url", &s.url, &mut parts);
    kv("version", &s.version, &mut parts);
    kv("sha256", &s.sha256, &mut parts);
    kv("extract", &s.extract, &mut parts);
    kv("manifest", &s.manifest, &mut parts);
    if let Some(comps) = &s.components {
        let arr = comps.iter().map(|c| ps(c)).collect::<Vec<_>>().join(", ");
        parts.push(format!("components = @({arr})"));
    }
    kv("id", &s.id, &mut parts);
    kv("scope", &s.scope, &mut parts);
    kv("override", &s.r#override, &mut parts);
    kv("formula", &s.formula, &mut parts);
    kv("package", &s.package, &mut parts);
    kv("args", &s.args, &mut parts);
    kv("run", &s.run, &mut parts);
    kv("accept_windows", &s.accept_windows, &mut parts);
    parts.join("; ")
}

fn ps_build_body(b: &BuildSpec) -> String {
    let mut parts: Vec<String> = Vec::new();
    let kv = |k: &str, v: &Option<String>, parts: &mut Vec<String>| {
        if let Some(val) = v {
            parts.push(format!("{k} = {}", ps(val)));
        }
    };
    kv("features", &b.features, &mut parts);
    kv("profile", &b.profile, &mut parts);
    kv("crt", &b.crt, &mut parts);
    kv("cmake_generator", &b.cmake_generator, &mut parts);
    kv("cuda_arch", &b.cuda_arch, &mut parts);
    kv("msvc_host", &b.msvc_host, &mut parts);
    parts.join("; ")
}

fn render_windows(m: &Manifest) -> String {
    let mut out = header_ps();
    out.push_str("$script:ContinuumManifest = [ordered]@{\n");
    for module in modules_for(m, "windows") {
        let mut fields: Vec<String> = vec![
            format!("order = {}", module.order),
            format!("tier = {}", module.tier),
        ];
        if !module.flags.is_empty() {
            let arr = module.flags.iter().map(|f| ps(f)).collect::<Vec<_>>().join(", ");
            fields.push(format!("flags = @({arr})"));
        }
        if let Some(a) = &module.applies {
            fields.push(format!("applies = {}", ps(a)));
        }
        // per-OS accept override on the source wins over module accept
        let accept = module
            .sources
            .get("windows")
            .and_then(|s| s.accept_windows.clone())
            .or_else(|| module.accept.clone());
        if let Some(a) = &accept {
            fields.push(format!("accept = {}", ps(a)));
        }
        if let Some(s) = module.sources.get("windows") {
            fields.push(format!("source = @{{ {} }}", ps_source_body(s)));
        }
        if let Some(b) = module.build.get("windows") {
            fields.push(format!("build = @{{ {} }}", ps_build_body(b)));
        }
        out.push_str(&format!("  {} = @{{ {} }}\n", ps(&module.id), fields.join("; ")));
    }
    out.push_str("}\n");
    out
}

// ─── Unix bash projection (first-cut for M5's consumer) ──────────────────────────

fn header_sh() -> String {
    format!(
        "# ==============================================================================\n\
         # GENERATED FILE - DO NOT EDIT.\n\
         # Rendered by `{gen}` (cargo run -p manifest-gen) from `{src}`.\n\
         # The manifest is the ONE source of truth; this is a mechanical projection.\n\
         # Edit the manifest and regenerate. CI drift-check (`manifest-gen --check`)\n\
         # fails loud if this file is stale or hand-edited.\n\
         # ==============================================================================\n\n",
        gen = REL_GEN,
        src = REL_SOURCE
    )
}

/// One flat set of `declare -A` assoc arrays keyed by module id, plus an ordered
/// MODULES list. Values single-quoted. Absent fields are simply not set (consumer
/// treats unset as empty). Shape is a proposal — M5 owns the final bash consumer.
fn render_unix(m: &Manifest, os: &str) -> String {
    let mods = modules_for(m, os);
    let mut out = header_sh();
    out.push_str(&format!("# platform: {os}\n"));
    let ordered = mods.iter().map(|x| sh(&x.id)).collect::<Vec<_>>().join(" ");
    out.push_str(&format!("CONTINUUM_MODULES=({ordered})\n\n"));

    // Helper to render one assoc array from a per-module extractor.
    let mut array = |name: &str, f: &dyn Fn(&Module) -> Option<String>| {
        let mut lines: Vec<String> = Vec::new();
        for module in &mods {
            if let Some(v) = f(module) {
                lines.push(format!("[{}]={}", sh(&module.id), sh(&v)));
            }
        }
        if lines.is_empty() {
            out.push_str(&format!("declare -A {name}=()\n"));
        } else {
            out.push_str(&format!("declare -A {name}=( {} )\n", lines.join(" ")));
        }
    };

    array("MOD_ORDER", &|x| Some(x.order.to_string()));
    array("MOD_TIER", &|x| Some(x.tier.to_string()));
    array("MOD_FLAGS", &|x| {
        if x.flags.is_empty() { None } else { Some(x.flags.join(",")) }
    });
    array("MOD_APPLIES", &|x| x.applies.clone());
    array("MOD_ACCEPT", &|x| {
        x.sources
            .get(os)
            .and_then(|s| match os {
                "macos" => s.accept_macos.clone(),
                "linux" => s.accept_linux.clone(),
                _ => None,
            })
            .or_else(|| x.accept.clone())
    });
    array("MOD_TYPE", &|x| x.sources.get(os).map(|s| s.kind.clone()));
    array("MOD_URL", &|x| x.sources.get(os).and_then(|s| s.url.clone()));
    array("MOD_VERSION", &|x| x.sources.get(os).and_then(|s| s.version.clone()));
    array("MOD_SHA256", &|x| x.sources.get(os).and_then(|s| s.sha256.clone()));
    array("MOD_EXTRACT", &|x| x.sources.get(os).and_then(|s| s.extract.clone()));
    array("MOD_REDIST_MANIFEST", &|x| x.sources.get(os).and_then(|s| s.manifest.clone()));
    array("MOD_COMPONENTS", &|x| {
        x.sources.get(os).and_then(|s| s.components.as_ref().map(|c| c.join(",")))
    });
    array("MOD_FORMULA", &|x| x.sources.get(os).and_then(|s| s.formula.clone()));
    array("MOD_PACKAGE", &|x| x.sources.get(os).and_then(|s| s.package.clone()));
    array("MOD_ARGS", &|x| x.sources.get(os).and_then(|s| s.args.clone()));
    array("MOD_RUN", &|x| x.sources.get(os).and_then(|s| s.run.clone()));

    // build-core: features/profile per os
    array("MOD_BUILD_FEATURES", &|x| {
        x.build.get(os).and_then(|b| b.features.clone())
    });
    array("MOD_BUILD_PROFILE", &|x| {
        x.build.get(os).and_then(|b| b.profile.clone())
    });
    out
}

// ─── Files to render ─────────────────────────────────────────────────────────────

fn rendered_files(m: &Manifest) -> Vec<(PathBuf, String)> {
    let d = out_dir();
    vec![
        (d.join("manifest.windows.ps1"), render_windows(m)),
        (d.join("manifest.macos.sh"), render_unix(m, "macos")),
        (d.join("manifest.linux.sh"), render_unix(m, "linux")),
    ]
}

fn main() {
    let check = std::env::args().any(|a| a == "--check");

    let mpath = manifest_path();
    let text = match std::fs::read_to_string(&mpath) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("manifest-gen: cannot read {}: {e}", mpath.display());
            exit(2);
        }
    };
    let manifest: Manifest = match toml::from_str(&text) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("manifest-gen: parse error in {}:\n{e}", mpath.display());
            exit(2);
        }
    };

    let files = rendered_files(&manifest);

    if check {
        let mut drift = false;
        for (path, want) in &files {
            match std::fs::read_to_string(path) {
                Ok(have) if &have == want => {}
                Ok(_) => {
                    eprintln!("DRIFT: {} is stale (regenerate: cargo run -p manifest-gen)", path.display());
                    drift = true;
                }
                Err(_) => {
                    eprintln!("DRIFT: {} missing (regenerate: cargo run -p manifest-gen)", path.display());
                    drift = true;
                }
            }
        }
        if drift {
            exit(1);
        }
        println!("manifest-gen --check: OK ({} files in sync)", files.len());
        return;
    }

    if let Err(e) = std::fs::create_dir_all(out_dir()) {
        eprintln!("manifest-gen: cannot create {}: {e}", out_dir().display());
        exit(2);
    }
    for (path, body) in &files {
        if let Err(e) = std::fs::write(path, body) {
            eprintln!("manifest-gen: cannot write {}: {e}", path.display());
            exit(2);
        }
        println!("wrote {}", path.display());
    }
}
