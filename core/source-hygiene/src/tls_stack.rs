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
//! is the default build's real graph on every platform, read without compiling. One tree of the
//! whole graph, which must succeed: an inverted tree (`-i name`) exits non-zero both for a crate
//! outside the graph and for a cargo that failed, so it read every failure as "not reached"
//! (Cormac on #5513, reproduced offline with a crate missing).

/// The OpenSSL family: none of these may be reached by the default build on any target.
pub const FORBIDDEN: &[&str] = &["openssl", "openssl-sys", "native-tls", "hyper-tls", "tokio-native-tls"];

/// Which forbidden crates appear in `tree`: `cargo tree --prefix none -f {p}` output, one package
/// per line as `name vX.Y.Z[ (path)]`.
pub fn reached(tree: &str) -> Vec<&'static str> {
    let packages: std::collections::BTreeSet<&str> = tree.lines().filter_map(|line| line.split_whitespace().next()).collect();
    FORBIDDEN.iter().copied().filter(|name| packages.contains(name)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a forbidden crate in the tree is named by its package name, and a crate
    // whose name merely contains one (`openssl-probe`, a path) is not.
    #[test]
    fn only_forbidden_packages_in_the_tree_are_reported() {
        let tree = "reqwest v0.12.9\nnative-tls v0.2.12\nopenssl-probe v0.1.5\ncontinuum-core v0.1.0 (/x/openssl)\n";
        assert_eq!(reached(tree), vec!["native-tls"]);
    }

    // what this catches: OpenSSL or native-tls creeping back into the default build on any
    // platform after #5513 (reqwest defaults, a crate asking for native-tls by name).
    #[test]
    fn the_default_build_reaches_no_openssl() {
        let root = crate::repo_root().expect("the workspace root is above this crate");
        let out = std::process::Command::new(env!("CARGO"))
            .args(["tree", "--workspace", "--target", "all", "-e", "normal,build", "--locked", "--prefix", "none", "-f", "{p}"])
            .current_dir(&root)
            .output()
            .expect("cargo runs");
        // A failed resolve is a failed check, never "not reached". `--locked`, not `--offline`: the
        // graph spans every target, and a runner's cache holds only its own platform's crates
        // (CI 2026-10-31: `cxxbridge-cmd` missing offline), so cargo may fetch what the lock
        // names but never change it.
        assert!(out.status.success(), "cargo tree failed, so the graph is unknown: {}", String::from_utf8_lossy(&out.stderr));
        let found = reached(&String::from_utf8_lossy(&out.stdout));
        assert!(
            found.is_empty(),
            "the default build reaches the OpenSSL family again: {found:?}. Use rustls; TLS is \
             declared once in the workspace manifest (reqwest, hf-hub, livekit)."
        );
    }
}
