#!/usr/bin/env python3
"""Cargo's Linux lib-test runner: reuse its executable, cwd and loader environment.

The ordinary suite and the ignored Postgres suite run serially even if one fails.
This is invoked only by the workflow's `cargo test --lib` target.runner setting;
it neither builds another target nor reconstructs Cargo's native-library paths.
"""

import argparse
import hashlib
import json
import re
import subprocess
import sys
import time
from pathlib import Path


SUMMARY = re.compile(
    rb"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;",
    re.MULTILINE,
)
PG_ARGS = ["orm::postgres", "--ignored", "--test-threads=1", "--color=never"]


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def run_suite(executable, arguments, log_path):
    started = time.monotonic()
    tail = b""
    with log_path.open("wb") as log:
        try:
            child = subprocess.Popen(
                [str(executable), *arguments],
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
        except OSError as error:
            return {"exit_code": None, "error": str(error), "log": str(log_path)}
        try:
            while chunk := child.stdout.read(65536):
                log.write(chunk)
                sys.stdout.buffer.write(chunk)
                sys.stdout.buffer.flush()
                # libtest's final summary is small; retain bounded parsing state,
                # while preserving the complete output in the log and CI console.
                tail = (tail + chunk)[-8192:]
            exit_code = child.wait()
        except BaseException:
            child.kill()
            child.wait()
            raise
        finally:
            child.stdout.close()
    summaries = list(SUMMARY.finditer(tail))
    summary = None
    if len(summaries) == 1:
        match = summaries[0]
        summary = {
            "result": match[1].decode("ascii"),
            "passed": int(match[2]),
            "failed": int(match[3]),
            "ignored": int(match[4]),
        }
    return {
        "exit_code": exit_code,
        "summary": summary,
        "elapsed_seconds": round(time.monotonic() - started, 3),
        "log": str(log_path),
    }


def passing(result, minimum, allow_ignored):
    summary = result.get("summary")
    return (
        result["exit_code"] == 0
        and summary is not None
        and summary["result"] == "ok"
        and summary["passed"] >= minimum
        and summary["failed"] == 0
        and (allow_ignored or summary["ignored"] == 0)
    )


def run(executable, arguments, output_dir, source_head):
    executable = executable.resolve(strict=True)
    output_dir.mkdir(parents=True, exist_ok=True)
    before = digest(executable)
    ordinary_args = [*arguments, "--color=never"]
    print("Running ordinary lib tests from Cargo's selected executable", flush=True)
    ordinary = run_suite(executable, ordinary_args, output_dir / "lib.log")
    print("Running ignored Postgres tests from the same executable", flush=True)
    postgres = run_suite(executable, PG_ARGS, output_dir / "postgres.log")
    unchanged = digest(executable) == before
    success = unchanged and passing(ordinary, 1, True) and passing(postgres, 11, False)
    receipt = {
        "source_head": source_head,
        "executable": str(executable),
        "executable_sha256": before,
        "executable_unchanged": unchanged,
        "cwd": str(Path.cwd()),
        "ordinary_args": ordinary_args,
        "postgres_args": PG_ARGS,
        "ordinary": ordinary,
        "postgres": postgres,
        "success": success,
    }
    receipt_path = output_dir / "receipt.json"
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(f"Rust test receipt: {receipt_path}", flush=True)
    if not success:
        print(
            "::error::Rust tests failed, the executable changed, or the required "
            "libtest summaries are missing (Postgres requires at least 11 passes, "
            "zero failures and zero ignored tests).",
            file=sys.stderr,
        )
    return 0 if success else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("executable", type=Path)
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    source_head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True
    ).strip()
    return run(args.executable, args.arguments, args.output_dir, source_head)


if __name__ == "__main__":
    sys.exit(main())
