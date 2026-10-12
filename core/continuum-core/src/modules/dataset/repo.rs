//! `dataset/from-repo`: a repository's source at ONE revision as a training corpus, so a
//! gene can carry knowledge a persona never produced herself (Joel, 2026-10-11: help her
//! with NEW abilities, not only her old turns). Every other automatic source trains on her
//! own output; this is the codebase she is asked to change, read the way she will meet it:
//! a file's path, then its source.
//!
//! One example per chunk of a file: the prompt names the repository, the revision, the
//! path and the line span; the completion is that span. Chunks end on whole top-level items
//! (a blank line followed by an unindented line) and stay under the example's byte budget,
//! so a chunk is code a reader would quote, never half a function. The revision is resolved
//! to its commit once and every file is read AT it (`git show <sha>:<path>`), so the corpus
//! is exactly one commit whatever the working tree holds, and the manifest names it.
//!
//! The byte budget only sizes chunks: the training run measures each example's true token
//! count in the engine and parks what its window cannot hold (`/train/measure`, #4937).

use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use super::{DatasetManifest, DatasetService};
use crate::code::git_bridge::{origin_repo_slug, run_git};

/// Bytes per token of source code on the bases she serves, the densest of five measured core
/// files (qwen3.5-4b-code-forged tokenizer, the qwen35 family of her 27B, 2026-10-11 on the
/// M5: competence.rs 3.53, dataset.rs 3.94, training_trigger.rs 4.06, service_loop.rs 4.16,
/// job_create.rs 4.18). The densest bounds a chunk's tokens from above.
const SOURCE_BYTES_PER_TOKEN: f64 = 3.53;

/// What a corpus reads when its caller names no extensions: source and prose a reader quotes.
/// An ALLOWLIST, never a deny-list: its output is an adapter that can reproduce what it read,
/// so the unknown (a key with no extension, a token in a config file) is refused, not admitted
/// (Cormac and BigMama on #4953). Data formats that carry credentials (json, yaml, env) are out.
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "py", "go", "java", "kt", "swift", "c", "h", "cc", "cpp", "hpp",
    "cs", "rb", "md", "toml", "sql", "html", "css", "proto", "wgsl", "metal", "cu", "cuh",
];

/// A path that must never become training text, whatever extensions the caller names: key
/// material and credential stores by their conventional names.
fn is_secret_shaped(path: &str) -> bool {
    path.split('/').any(|part| {
        let p = part.to_ascii_lowercase();
        p.starts_with(".env")
            || p.starts_with("id_rsa")
            || p.starts_with("id_ed25519")
            || p.starts_with("id_ecdsa")
            || [".pem", ".key", ".p12", ".pfx", ".keystore", ".jks", ".asc", ".gpg"].iter().any(|e| p.ends_with(e))
            || [".npmrc", ".netrc", ".pypirc", ".git-credentials", "credentials"].contains(&p.as_str())
            || p.contains("secret")
    })
}

/// Params for `dataset/from-repo`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/dataset/FromRepoParams.ts")]
pub struct FromRepoParams {
    /// A local clone of the repository.
    pub repo_dir: String,
    /// The revision to read (a branch, tag or sha). Default `HEAD`; resolved to one commit.
    #[serde(default = "default_revision")]
    pub revision: String,
    /// Pathspecs under the repository root to include. Empty: the whole tree.
    #[serde(default)]
    pub paths: Vec<String>,
    /// File extensions to include, without the dot (`["rs", "md"]`). Empty: the source and
    /// prose allowlist. Secret-shaped paths are refused either way.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Dataset name (subdirectory under the datasets root).
    pub name: String,
    /// Override the datasets root directory (default `~/.continuum/datasets`).
    #[serde(default)]
    #[ts(optional)]
    pub output_dir: Option<String>,
    /// The most tokens one example's source may take. Default 16,384: the 5090 trains her
    /// at a 16,896-token window today, and a corpus example carries no system prompt.
    #[serde(default = "default_example_tokens")]
    pub max_tokens_per_example: u32,
    /// Fraction of examples placed in the train split. Default `1.0`: the corpus is
    /// knowledge to carry, and the gene is judged on her own work, not on held-out files.
    #[serde(default = "default_corpus_split")]
    pub split_ratio: f64,
}

fn default_revision() -> String {
    "HEAD".to_string()
}
pub(crate) fn default_example_tokens() -> u32 {
    16_384
}
fn default_corpus_split() -> f64 {
    1.0
}

/// One file's lines `[first, last]` (1-based, inclusive) and their text.
#[derive(Debug, PartialEq)]
struct Span {
    first: usize,
    last: usize,
    text: String,
}

/// A file cut into spans of at most `max_bytes`, each ending on a whole top-level item
/// where one fits; an item longer than the budget is cut at line boundaries.
fn spans(source: &str, max_bytes: usize) -> Vec<Span> {
    // items: maximal runs of lines, a new one starting at an unindented line after a blank one
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let mut items: Vec<(usize, usize)> = Vec::new(); // [start, end) line indices
    let mut start = 0;
    for i in 1..lines.len() {
        let begins_item = lines[i - 1].trim().is_empty() && !lines[i].starts_with([' ', '\t', '\n', '\r']);
        if begins_item {
            items.push((start, i));
            start = i;
        }
    }
    if start < lines.len() {
        items.push((start, lines.len()));
    }
    let bytes = |a: usize, b: usize| lines[a..b].iter().map(|l| l.len()).sum::<usize>();
    let mut out = Vec::new();
    let emit = |a: usize, b: usize, out: &mut Vec<Span>| {
        let text: String = lines[a..b].concat();
        if !text.trim().is_empty() {
            out.push(Span { first: a + 1, last: b, text });
        }
    };
    let (mut a, mut b) = (0usize, 0usize); // the open span's lines [a, b)
    for (s, e) in items {
        if bytes(a, e) <= max_bytes {
            b = e; // the item joins the open span
            continue;
        }
        if b > a {
            emit(a, b, &mut out);
        }
        // the item opens the next span, cut at line boundaries if it alone is over the budget
        a = s;
        b = s;
        while b < e {
            if bytes(a, b + 1) > max_bytes && b > a {
                emit(a, b, &mut out);
                a = b;
            }
            b += 1;
        }
    }
    if b > a {
        emit(a, b, &mut out);
    }
    out
}

/// A repository's source at one commit, cut into examples: the one reader both the operator's
/// `dataset/from-repo` and a citizen's `code/study` use.
pub(crate) struct Corpus {
    /// `owner/name` of `origin`, else the directory name.
    pub label: String,
    /// The commit every file was read at.
    pub sha: String,
    /// (prompt, completion): `<label>@<sha9>:<path> lines a-b`, then that span.
    pub examples: Vec<(String, String)>,
    /// Files under the named paths left out: not on the allowlist, secret-shaped, or not UTF-8.
    pub skipped: usize,
}

impl Corpus {
    /// Read `paths` (pathspecs; empty = the whole tree) of `repo` at `revision`, keeping files
    /// whose extension is in `extensions` (empty = [`SOURCE_EXTENSIONS`]) and whose path is not
    /// secret-shaped, each cut into spans of at
    /// most `max_tokens` (bounded by [`SOURCE_BYTES_PER_TOKEN`]).
    pub(crate) fn read(
        repo: &Path,
        revision: &str,
        paths: &[String],
        extensions: &[String],
        max_tokens: u32,
    ) -> Result<Self, String> {
        if !repo.is_dir() {
            return Err(format!("repository not found: {}", repo.display()));
        }
        let sha = run_git(repo, &["rev-parse", "--verify", &format!("{revision}^{{commit}}")])
            .map_err(|e| format!("revision {revision:?} does not name a commit in {}: {e}", repo.display()))?
            .trim()
            .to_string();
        let mut ls = vec!["ls-tree", "-r", "-z", "--name-only", sha.as_str(), "--"];
        ls.extend(paths.iter().map(String::as_str));
        let listing = run_git(repo, &ls).map_err(|e| format!("listing {sha}: {e}"))?;
        let wanted = |path: &str| {
            !is_secret_shaped(path)
                && Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    if extensions.is_empty() {
                        SOURCE_EXTENSIONS.contains(&e)
                    } else {
                        extensions.iter().any(|w| w == e)
                    }
                })
        };
        let max_bytes = (f64::from(max_tokens) * SOURCE_BYTES_PER_TOKEN) as usize;
        let label = repo_label(repo);
        let short = sha[..sha.len().min(9)].to_string();
        let mut examples = Vec::new();
        let mut skipped = 0;
        for path in listing.split('\0').filter(|s| !s.is_empty()) {
            if !wanted(path) {
                skipped += 1;
                continue;
            }
            // a file that is not UTF-8 is not source a reader quotes: skipped, never mangled
            let Ok(source) = run_git(repo, &["show", &format!("{sha}:{path}")]) else {
                skipped += 1;
                continue;
            };
            for span in spans(&source, max_bytes) {
                examples.push((format!("{label}@{short}:{path} lines {}-{}", span.first, span.last), span.text));
            }
        }
        if examples.is_empty() {
            return Err(format!(
                "no source in {} at {short} under {paths:?} with extensions {extensions:?}: nothing to study",
                repo.display()
            ));
        }
        Ok(Self { label, sha, examples, skipped })
    }
}

impl DatasetService {
    /// Build a training dataset from a repository's source at one revision.
    pub fn from_repo(&self, p: &FromRepoParams) -> Result<DatasetManifest, String> {
        let corpus = Corpus::read(
            Path::new(&p.repo_dir),
            &p.revision,
            &p.paths,
            &p.extensions,
            p.max_tokens_per_example,
        )?;
        let examples: Vec<Value> = corpus
            .examples
            .iter()
            .map(|(prompt, completion)| {
                json!({ "messages": [
                    { "role": "user", "content": prompt },
                    { "role": "assistant", "content": completion },
                ]})
            })
            .collect();
        let output_dir = self.resolve_root(p.output_dir.as_deref()).join(&p.name);
        let mut manifest = Self::split_and_write(&p.name, &output_dir, &examples, p.split_ratio, None)?;
        manifest.source = Some(format!("repo:{}@{}", corpus.label, corpus.sha));
        manifest.write_to(&output_dir)?;
        Ok(manifest)
    }
}

/// The repository's name as a reader knows it: `origin`'s `owner/name`, else the directory.
fn repo_label(repo: &Path) -> String {
    origin_repo_slug(repo).unwrap_or_else(|| {
        repo.file_name().and_then(|n| n.to_str()).unwrap_or("repo").to_string() // unwrap_or: a root path has no name
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a chunk that ends inside a function. Spans close on whole top-level
    // items while they fit, every line lands in exactly one span, and line numbers are true.
    #[test]
    fn spans_end_on_whole_items_and_cover_every_line() {
        let src = "fn a() {\n    one();\n}\n\nfn b() {\n    two();\n}\n\nfn c() {\n    three();\n}\n";
        let budget = "fn a() {\n    one();\n}\n\nfn b() {\n    two();\n}\n\n".len();
        let got = spans(src, budget);
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].first, got[0].last), (1, 8));
        assert!(got[0].text.ends_with("}\n\n"), "the first span closes after fn b, not inside it");
        assert_eq!((got[1].first, got[1].last), (9, 11));
        assert_eq!(got.iter().map(|s| s.text.as_str()).collect::<String>(), src, "every line, once, in order");
    }

    // what this catches: one item longer than the budget either vanishing or producing a span
    // over the budget. It is cut at line boundaries, each piece within the budget.
    #[test]
    fn an_item_over_the_budget_is_cut_at_lines_within_it() {
        let body: String = (0..40).map(|i| format!("    line_{i:02}();\n")).collect();
        let src = format!("fn big() {{\n{body}}}\n");
        let got = spans(&src, 64);
        assert!(got.len() > 1);
        assert!(got.iter().all(|s| s.text.len() <= 64));
        assert_eq!(got.iter().map(|s| s.text.as_str()).collect::<String>(), src);
    }

    // what this catches: a corpus that trains an adapter on key material. With no extensions
    // named only allowlisted source is read; a secret-shaped path is refused even when its
    // extension is asked for by name; and the refusals are counted, never silent.
    #[test]
    fn keys_and_unknown_files_never_reach_a_corpus() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let git = |args: &[&str]| run_git(repo, args).unwrap();
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        for (name, body) in [
            ("lib.rs", "fn ok() {}\n"),
            ("id_ed25519", "-----BEGIN OPENSSH PRIVATE KEY-----\n"),
            (".env", "TOKEN=abc\n"),
            ("config.json", "{\"token\": \"abc\"}\n"),
            ("deploy_secret.rs", "const TOKEN: &str = \"abc\";\n"),
        ] {
            std::fs::write(repo.join(name), body).unwrap();
        }
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "-q", "-m", "one"]);
        let all = Corpus::read(repo, "HEAD", &[], &[], default_example_tokens()).unwrap();
        assert_eq!(all.examples.len(), 1, "only lib.rs is allowlisted source");
        assert_eq!(all.skipped, 4);
        let named = Corpus::read(repo, "HEAD", &[], &["rs".into()], default_example_tokens()).unwrap();
        assert!(named.examples.iter().all(|(p, _)| !p.contains("secret")), "a secret-shaped .rs is refused by name too");
    }

    // what this catches: a corpus read from the working tree instead of the named commit. A
    // file changed after the commit must appear as committed, and the manifest names the sha.
    #[test]
    fn the_corpus_is_the_named_commit_not_the_working_tree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let git = |args: &[&str]| run_git(repo, args).unwrap();
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(repo.join("lib.rs"), "fn committed() {}\n").unwrap();
        std::fs::write(repo.join("notes.txt"), "not rust\n").unwrap();
        git(&["add", "."]);
        git(&["-c", "commit.gpgsign=false", "commit", "-q", "-m", "one"]);
        let sha = git(&["rev-parse", "HEAD"]).trim().to_string();
        std::fs::write(repo.join("lib.rs"), "fn uncommitted() {}\n").unwrap();

        let out = tempfile::tempdir().unwrap();
        let service = DatasetService::new(out.path().to_path_buf());
        let manifest = service
            .from_repo(&FromRepoParams {
                repo_dir: repo.display().to_string(),
                revision: "HEAD".into(),
                paths: vec![],
                extensions: vec!["rs".into()],
                name: "corpus".into(),
                output_dir: None,
                max_tokens_per_example: default_example_tokens(),
                split_ratio: default_corpus_split(),
            })
            .unwrap();
        assert_eq!(manifest.total_examples, 1, "the .txt file is not an .rs file");
        assert!(manifest.source.as_deref().is_some_and(|s| s.ends_with(&sha)));
        let train = std::fs::read_to_string(out.path().join("corpus/train.jsonl")).unwrap();
        assert!(train.contains("fn committed()") && !train.contains("uncommitted"));
        assert!(train.contains(":lib.rs lines 1-1"));
    }
}
