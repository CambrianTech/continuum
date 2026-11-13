//! Windows: install locations built from the environment, never a literal `C:\`.
//!
//! The shared list had no Windows arm, so the browser was never found on any Windows host,
//! including those with Microsoft Edge (Chromium), which ships with the OS. Measured on BigMama
//! 2026-09-05: Edge at `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`, and
//! `web/search` failing 100% with "no web-search provider is available". Paths are composed from
//! `%ProgramFiles%`, `%ProgramFiles(x86)%` and `%LOCALAPPDATA%`, with forward slashes so the
//! caller's path branch takes them (Windows APIs accept either separator).

pub(super) fn install_candidates() -> Vec<String> {
    // Chrome first (best-tested headless), then Edge (present on every Windows), then the rest.
    const RELATIVE: &[&str] = &[
        "Google/Chrome/Application/chrome.exe",
        "Chromium/Application/chrome.exe",
        "Microsoft/Edge/Application/msedge.exe",
        "BraveSoftware/Brave-Browser/Application/brave.exe",
        "Vivaldi/Application/vivaldi.exe",
    ];
    let roots: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(|k| std::env::var(k).ok()) // an unset root is a location this host does not have
        .map(|r| r.replace('\\', "/"))
        .collect();
    let mut out = Vec::with_capacity(roots.len() * RELATIVE.len());
    for rel in RELATIVE {
        for root in &roots {
            out.push(format!("{root}/{rel}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    // what this catches: Windows with no browser candidates (web/search dark on every Windows host,
    // BigMama 2026-09-05), or candidates that are not derived from the environment. Asserting "does
    // not start with C:/" would be the wrong test: on a C: box those variables ARE C:\Program Files.
    // The property that matters is derivation, and the forward slashes the caller's path branch needs.
    #[test]
    fn candidates_are_derived_from_the_environment_and_include_edge() {
        let cands = super::install_candidates();
        assert!(!cands.is_empty(), "Windows had ZERO browser candidates: the regression this pins");
        let roots: Vec<String> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .map(|r| r.replace('\\', "/"))
            .collect();
        assert!(!roots.is_empty(), "Windows always sets at least one of these");
        assert!(cands.iter().all(|c| roots.iter().any(|r| c.starts_with(r.as_str()))), "every candidate is rooted at an environment path {roots:?}: {cands:?}");
        assert!(cands.iter().any(|c| c.ends_with("msedge.exe")), "Edge ships with Windows; it must be a candidate");
        assert!(cands.iter().all(|c| c.contains('/')), "forward slashes, or the caller treats these as $PATH names");
    }
}
