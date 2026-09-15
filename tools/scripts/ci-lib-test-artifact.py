#!/usr/bin/env python3
"""Select Cargo's exact unit-test artifact and retain its Linux loader paths."""
import json
import os
from pathlib import Path
import sys


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
        "CONTINUUM_LIB_TEST_LD_LIBRARY_PATH": ":".join(paths),
        "CARGO_MANIFEST_DIR": str(manifest_dir),
    }
    if any("\n" in value or "\r" in value for value in result.values()):
        raise ValueError("test runtime paths cannot contain newlines")
    return result


if __name__ == "__main__":
    try:
        artifacts_file, manifest_dir, rust_libdir = sys.argv[1:]
        with open(artifacts_file, encoding="utf-8") as stream:
            messages = [json.loads(line) for line in stream if line.strip()]
        for name, value in environment(messages, manifest_dir, rust_libdir, os.environ.get("LD_LIBRARY_PATH", "")).items():
            print(f"{name}={value}")
    except (OSError, ValueError, KeyError) as error:
        sys.exit(f"Cannot reuse Cargo unit-test artifact: {error}")
