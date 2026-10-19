"""Regression: CI must reuse its exact Cargo artifact without losing loader paths."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys
import os
import io
import json

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location(
    "ci_lib_test_artifact", Path(__file__).resolve().parents[1] / "ci-lib-test-artifact.py"
)
artifact = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifact)


class ArtifactReuseTest(unittest.TestCase):
    def test_exact_artifact_loader_paths_and_rejection(self):
        with tempfile.TemporaryDirectory(prefix="continuum artifact ") as temporary:
            root = Path(temporary).resolve()
            manifest = root / "core/continuum-core"
            executable = root / "target/debug/deps/continuum_core-exact"
            executable.parent.mkdir(parents=True)
            executable.write_text("fixture", encoding="utf-8")
            executable.chmod(0o755)
            native = root / "target/debug/build/native/out"
            message = {
                "reason": "compiler-artifact",
                "target": {"name": "continuum_core", "src_path": str(manifest / "src/lib.rs")},
                "profile": {"test": True},
                "executable": str(executable),
            }
            library = dict(message, profile={"test": False}, executable=None)
            messages = [library, message, {"reason": "build-script-executed", "linked_paths": [
                "native=" + str(native), "native=" + str(root / "external"),
                "native=" + str(root / "target/debug-other/not-owned"),
            ]}]
            result = artifact.environment(messages, manifest, root / "rustlib", "/inherited/lib")
            self.assertEqual(result["CONTINUUM_LIB_TEST"], str(executable))
            self.assertEqual(result["CARGO_MANIFEST_DIR"], str(manifest))
            self.assertEqual(result["CONTINUUM_LIB_TEST_LD_LIBRARY_PATH"], os.pathsep.join([
                str(native), str(executable.parent), str(executable.parent.parent),
                str(root / "rustlib"), "/inherited/lib",
            ]))
            other = dict(message, executable=str(executable.with_name("continuum_core-stale")))
            for invalid in ([library], [message, other]):
                with self.assertRaisesRegex(ValueError, "expected one"):
                    artifact.environment(invalid, manifest, root / "rustlib", "")
            executable.unlink()
            with self.assertRaisesRegex(ValueError, "missing or not executable"):
                artifact.environment([message], manifest, root / "rustlib", "")

            # c1280057: compilation fails before artifact selection/warning checks.
            # Preserve the original failure and raw stream, but expose its error now.
            events = [message, {"reason": "compiler-message", "message": {
                "level": "error", "message": "bad constructor", "rendered": "error: bad constructor 日本語 →\n",
            }}, {"reason": "build-finished", "success": False}]
            payload = "".join(json.dumps(event) + "\n" for event in events)
            log = io.StringIO()
            raw = root / "compiler.jsonl"
            code = "import sys; sys.stdout.write(sys.argv[1]); sys.stdout.flush(); sys.exit(7)"
            self.assertEqual(artifact.capture(raw, [sys.executable, "-c", code, payload], log), 7)
            self.assertEqual(raw.read_text(encoding="utf-8"), payload)
            self.assertIn("error: bad constructor", log.getvalue())
            self.assertIn("target=continuum_core", log.getvalue())
            self.assertIn("success=False", log.getvalue())
            encoded = io.BytesIO()
            constrained = io.TextIOWrapper(encoded, encoding="cp1252", errors="strict")
            self.assertEqual(artifact.capture(raw, [sys.executable, "-c", code, payload], constrained), 7)
            constrained.flush()
            self.assertIn(b"error: bad constructor \\u65e5", encoded.getvalue())
            self.assertEqual(raw.read_text(encoding="utf-8"), payload)
            self.assertEqual(artifact.capture(raw, [sys.executable, "-c", "print('invalid')"], io.StringIO()), 1)
            self.assertEqual(artifact.capture(raw, [sys.executable, "-c", "print('[]')"], io.StringIO()), 1)


if __name__ == "__main__":
    unittest.main()
