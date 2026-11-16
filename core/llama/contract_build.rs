// The core<->engine contract (CONTINUUM-ENGINE 4.11): the fork's `tools/server/server-continuum.h` is the one
// definition of every frame. This step binds it with bindgen (layout tests ON: these are fixed-size POD frames,
// unlike the opaque pointer-only structs the llama bindings bind), compiles the engine's own validator so the core
// can check its Rust edge against it byte for byte, and derives the schema identity exactly as the fork's CMake
// does: the first 16 bytes of the header's SHA-256.

use sha2::{Digest, Sha256};
use std::path::Path;

pub fn build(submodule: &Path, out_dir: &Path) {
    let server = submodule.join("tools").join("server");
    let header = server.join("server-continuum.h");
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", server.join("server-continuum.cpp").display());

    let bytes = std::fs::read(&header).unwrap_or_else(|e| panic!("the contract header {}: {e}", header.display()));
    let digest = Sha256::digest(&bytes);
    let id: Vec<String> = digest[..16].iter().map(|b| format!("0x{b:02x}")).collect();

    let schema_c = out_dir.join("server-continuum-schema.c");
    std::fs::write(
        &schema_c,
        format!("#include \"server-continuum.h\"\nconst ctm_id ctm_schema_id = {{ {{ {} }} }};\n", id.join(", ")),
    )
    .expect("write the contract's schema identity");
    std::fs::write(out_dir.join("contract_schema_id.rs"), format!("[{}]", id.join(", ")))
        .expect("write the contract's schema identity for Rust");

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .flag_if_supported("/EHsc")
        .include(&server)
        .file(server.join("server-continuum.cpp"))
        .compile("continuum_contract_cpp");
    cc::Build::new().include(&server).file(&schema_c).compile("continuum_contract_schema");

    bindgen::Builder::default()
        .header(header.to_str().expect("the contract header's path is UTF-8"))
        .allowlist_type("ctm_.*")
        .allowlist_function("ctm_.*")
        .allowlist_var("CTM_.*")
        .allowlist_var("ctm_schema_id")
        .derive_default(true)
        .derive_eq(true)
        .layout_tests(true)
        .generate()
        .expect("bind the contract header")
        .write_to_file(out_dir.join("contract.rs"))
        .expect("write the contract bindings");
}
