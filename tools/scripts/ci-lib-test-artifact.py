#!/usr/bin/env python3
"""Select Cargo's exact unit-test artifact and retain native loader paths."""
import json
import os
from pathlib import Path
import sys
import subprocess
from datetime import datetime, timezone


def capture(path, command, output=sys.stdout):
    """Keep Cargo JSON intact while surfacing errors before a failed/cancelled build.

    No retry or second build: the child's exit status remains authoritative.
    Flushed event breadcrumbs survive even when cancellation prevents final upload.
    """
    # A redirected console may use a legacy encoding. Diagnostics must never
    # replace the compiler's exit status because a source snippet has Unicode.
    if hasattr(output, "reconfigure"):
        output.reconfigure(errors="backslashreplace")
    malformed = False
    with open(path, "w", encoding="utf-8") as raw:
        with subprocess.Popen(command, stdout=subprocess.PIPE, text=True, encoding="utf-8") as child:
            for line in child.stdout:
                raw.write(line)
                raw.flush()
                try:
                    event = json.loads(line)
                    if not isinstance(event, dict):
                        raise ValueError("Cargo event must be an object")
                except ValueError:
                    malformed = True
                    print("Invalid Cargo JSON event; retained in artifact", file=output, flush=True)
                    continue
                reason = event.get("reason")
                diagnostic = event.get("message", {})
                if reason == "compiler-message" and diagnostic.get("level") == "error":
                    print(diagnostic.get("rendered") or diagnostic.get("message", "Compiler error"), file=output, flush=True)
                target = event.get("target", {}).get("name")
                if reason == "build-finished" or (reason == "compiler-artifact" and target in ("llama", "continuum_core")):
                    print(f"{datetime.now(timezone.utc).isoformat()} cargo-event reason={reason} target={target} fresh={event.get('fresh')} success={event.get('success')}", file=output, flush=True)
            status = child.wait()
    return status if status else int(malformed)


def environment(messages, manifest_dir, rust_libdir, inherited_library_path):
    manifest_dir = Path(manifest_dir).resolve()
    source = manifest_dir / "src/lib.rs"
    artifacts = {
        Path(message["executable"]).resolve()
        for message in messages
        if message.get("reason") == "compiler-artifact"
        and message.get("target", {}).get("name") == "continuum_core"
        and Path(message["target"]["src_path"]).resolve() == source
        and message.get("profile", {}).get("test") is True
        and message.get("executable")
    }
    if len(artifacts) != 1:
        raise ValueError(f"expected one continuum-core lib test executable, got {len(artifacts)}")
    executable = artifacts.pop()
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise ValueError(f"test executable is missing or not executable: {executable}")
    deps = executable.parent
    profile = deps.parent
    if deps.name != "deps":
        raise ValueError(f"unexpected Cargo test executable directory: {deps}")
    # Cargo includes native build-script search paths only inside its target
    # output. Preserve that boundary; external/system search paths stay external.
    native_paths = set()
    for message in messages:
        if message.get("reason") != "build-script-executed":
            continue
        for linked in message.get("linked_paths", []):
            kind, separator, value = linked.partition("=")
            if separator and kind not in ("native", "all"):
                continue
            path = Path(value if separator else linked).resolve()
            if path.is_relative_to(profile):
                native_paths.add(str(path))
    paths = sorted(native_paths) + [str(deps), str(profile), str(Path(rust_libdir).resolve())]
    if inherited_library_path:
        paths.append(inherited_library_path)
    result = {
        "CONTINUUM_LIB_TEST": str(executable),
        "CONTINUUM_LIB_TEST_LD_LIBRARY_PATH": os.pathsep.join(paths),
        "CARGO_MANIFEST_DIR": str(manifest_dir),
    }
    if any("\n" in value or "\r" in value for value in result.values()):
        raise ValueError("test runtime paths cannot contain newlines")
    return result


if __name__ == "__main__":
    try:
        if len(sys.argv) > 4 and sys.argv[1] == "--capture" and sys.argv[3] == "--":
            # Ask the same Cargo executable for its configured output root; CI
            # and developer shells may override it. Metadata does not compile.
            metadata = subprocess.run(
                [sys.argv[4], "metadata", "--no-deps", "--format-version", "1"],
                check=True, capture_output=True, text=True, encoding="utf-8",
            )
            timings = str(Path(json.loads(metadata.stdout)["target_directory"]) / "cargo-timings")
            if "\n" in timings or "\r" in timings:
                raise ValueError("Cargo timing directory contains a newline")
            if os.environ.get("GITHUB_ENV"):
                with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as env:
                    env.write(f"CONTINUUM_CARGO_TIMINGS={timings}\n")
            sys.exit(capture(sys.argv[2], sys.argv[4:]))
        artifacts_file, manifest_dir, rust_libdir = sys.argv[1:]
        with open(artifacts_file, encoding="utf-8-sig") as stream:
            messages = [json.loads(line) for line in stream if line.strip()]
        for name, value in environment(messages, manifest_dir, rust_libdir, os.environ.get("LD_LIBRARY_PATH", "")).items():
            print(f"{name}={value}")
    except (OSError, ValueError, KeyError) as error:
        sys.exit(f"Cannot reuse Cargo unit-test artifact: {error}")
