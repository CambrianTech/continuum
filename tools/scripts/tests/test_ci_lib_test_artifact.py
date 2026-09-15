"""Regression: CI must reuse its exact Cargo artifact without losing loader paths."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys

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
            self.assertEqual(result["CONTINUUM_LIB_TEST_LD_LIBRARY_PATH"], ":".join([
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


if __name__ == "__main__":
    unittest.main()
