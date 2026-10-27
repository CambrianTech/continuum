//! One TLS stack: rustls. No crate in the build may depend on OpenSSL or `native-tls`.
//!
//! 2026-10-27: the 5090's deploy failed staging an `OPENSSL_INCLUDE_DIR-NOTFOUND` runtime
//! (#5512). The engine had found whatever OpenSSL the machine carried, because llama.cpp's
//! `LLAMA_OPENSSL` defaults ON, and nine crates pulled `native-tls` (OpenSSL on Linux)
//! through reqwest's default features. Joel: a security nightmare, bloat, and a continuous
//! cross-compilation fight (mobile binaries under 5 MB). #5513 removed it; this keeps it out.
//!
//! The rule asks Cargo's resolver, not `Cargo.lock`: the lockfile records each package's
//! dependencies as a union over every feature configuration, so one optional feature
//! (pocket-tts's hf-hub 0.4.3, card b48bf496) would make reqwest look like it reaches
//! OpenSSL in every build. `cargo tree --workspace --target all -e normal,build -i <crate>`
//! is the default build's real graph on every platform, read without compiling.

/// The OpenSSL family: none of these may be reached by the default build on any target.
pub const FORBIDDEN: &[&str] = &["openssl", "openssl-sys", "native-tls", "hyper-tls", "tokio-native-tls"];

/// Which forbidden crates the default build reaches, given a way to ask Cargo whether one is
/// in the graph. `reaches(name)` is `Ok(true)` when it is.
pub fn reached<E>(mut reaches: impl FnMut(&str) -> Result<bool, E>) -> Result<Vec<&'static str>, E> {
    let mut found = Vec::new();
    for name in FORBIDDEN {
        if reaches(name)? {
            found.push(*name);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a forbidden crate reported by the resolver is named, and one it
    // can't find is not (the inverted tree prints nothing for a package not in the graph).
    #[test]
    fn only_the_crates_the_resolver_finds_are_reported() {
        let found = reached::<()>(|name| Ok(name == "native-tls")).unwrap(); // unwrap: the closure never errs
        assert_eq!(found, vec!["native-tls"]);
    }

    // what this catches: OpenSSL or native-tls creeping back into the default build on any
    // platform after #5513 (reqwest defaults, a crate asking for native-tls by name).
    #[test]
    fn the_default_build_reaches_no_openssl() {
        let root = crate::repo_root().expect("the workspace root is above this crate");
        let found = reached(|name| {
            let out = std::process::Command::new(env!("CARGO"))
                .args(["tree", "--workspace", "--target", "all", "-e", "normal,build", "--offline", "-i", name])
                .current_dir(&root)
                .output()?;
            // An inverted tree for a package outside the graph exits non-zero, saying so.
            Ok::<bool, std::io::Error>(out.status.success() && !out.stdout.is_empty())
        })
        .expect("cargo tree runs");
        assert!(
            found.is_empty(),
            "the default build reaches the OpenSSL family again: {found:?}. Use rustls; TLS is \
             declared once in the workspace manifest (reqwest, hf-hub, livekit)."
        );
    }
}
