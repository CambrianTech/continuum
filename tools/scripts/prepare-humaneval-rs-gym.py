#!/usr/bin/env python3
"""Build the HumanEval-Rust gym set for `cognition/eval` from MultiPL-E.

Reuses HuggingFace `datasets` (the layer unsloth itself uses) to pull the
STANDARD HumanEval benchmark TRANSLATED TO RUST (`nuprl/MultiPL-E`, config
`humaneval-rs`) and projects each problem onto the `EvalTask` shape the Rust
gym grades:

    {"id", "prompt", "test", "lang": "rust"}

Why the Rust translation and not the Python original: the gym grades Rust
(`gym_grader::test_grade` compiles the persona's output with `rustc` and runs
it) because the persona ships Rust — it codes on continuum, which is Rust. So
the credible standard benchmark we measure against is HumanEval-in-Rust
(MultiPL-E), comparable to that project's published per-model Rust pass@1.

Mapping detail — MultiPL-E's `tests` blob is `}` (closing the prompt's open
`fn`) followed by a full `fn main() { <asserts> }`. Our grader wraps OUR `test`
field in its OWN `fn main() {…}`, so we extract just the inner assert body. The
asserts bind `let candidate = <entry_point>;`, which resolves against the
complete function the persona returns.

Output: docs/genome/humaneval-rs.jsonl  (one EvalTask per line)
Run:    python3 tools/scripts/prepare-humaneval-rs-gym.py
"""

import json
import pathlib
import sys

OUT = pathlib.Path(__file__).resolve().parents[2] / "docs/genome/humaneval-rs.jsonl"


def main_body(tests: str) -> str:
    """Return the BODY of `fn main() {…}` from a MultiPL-E Rust tests blob.

    Brace-matched (not regex) so nested braces in the asserts can't truncate
    it. Raises ValueError if the blob has no balanced `fn main` body — the
    caller skips that task loudly rather than emitting a half-test."""
    i = tests.find("fn main")
    if i < 0:
        raise ValueError("no `fn main` in tests blob")
    open_brace = tests.find("{", i)
    if open_brace < 0:
        raise ValueError("no opening brace after `fn main`")
    depth = 0
    start = None
    for j in range(open_brace, len(tests)):
        c = tests[j]
        if c == "{":
            depth += 1
            if depth == 1:
                start = j + 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return tests[start:j].strip("\n")
    raise ValueError("unbalanced braces in `fn main` body")


def signature(prompt: str) -> str:
    """The doc + signature posed to the model, trailing ` {` stripped so it
    reads as a spec to implement, not an open block to continue."""
    s = prompt.rstrip()
    if s.endswith("{"):
        s = s[:-1].rstrip()
    return s


def build_prompt(sig: str) -> str:
    """Frame the signature as an unambiguous "return the complete function"
    task — the grader only ever compiles the persona's code, never the prompt,
    so she must emit a standalone, compilable `fn`."""
    return (
        "Implement the following Rust function so it passes its tests. Reply with "
        "ONLY the complete function (signature and body) in a single ```rust fenced "
        "code block — no prose, no explanation.\n\n"
        f"{sig}\n"
    )


def main() -> int:
    try:
        from datasets import load_dataset
    except ImportError:
        print(
            "missing dependency: `datasets`. Install with `pip install datasets` "
            "(the HuggingFace layer unsloth uses).",
            file=sys.stderr,
        )
        return 1

    ds = load_dataset("nuprl/MultiPL-E", "humaneval-rs", split="test")
    written = 0
    skipped = 0
    with OUT.open("w") as f:
        for ex in ds:
            try:
                body = main_body(ex["tests"])
            except ValueError as e:
                print(f"skip {ex['name']}: {e}", file=sys.stderr)
                skipped += 1
                continue
            task = {
                "id": ex["name"],
                "prompt": build_prompt(signature(ex["prompt"])),
                "test": body,
                "lang": "rust",
            }
            f.write(json.dumps(task) + "\n")
            written += 1
    print(f"wrote {written} tasks ({skipped} skipped) -> {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
