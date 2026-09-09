"""Exercise the CI runner with a real, dependency-free Rust libtest executable."""

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from rust_lib_runner import passing


RUNNER = Path(__file__).with_name("rust_lib_runner.py").resolve()
REPO = RUNNER.parents[2]


class CargoRunnerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.storage = tempfile.TemporaryDirectory(prefix="continuum-ci-runner-")
        cls.root = Path(cls.storage.name)
        cls.binary = cls.root / ("fixture.exe" if os.name == "nt" else "fixture")
        fixture = cls.root / "fixture.rs"
        fixture.write_text(
            '''
#[test]
fn ordinary() {
    assert_eq!(std::env::current_dir().unwrap().canonicalize().unwrap(),
        std::path::Path::new(&std::env::var("EXPECTED_CWD").unwrap()).canonicalize().unwrap());
    assert_eq!(std::env::var("CI_RUNNER_SENTINEL").unwrap(), "inherited environment");
    assert!(std::env::var_os("FAIL_ORDINARY").is_none());
}
#[test]
fn existing_skip() { panic!("the existing skip arguments must survive"); }
mod orm { mod postgres {
    macro_rules! pg {
        ($($name:ident),*) => { $(
            #[test] #[ignore]
            fn $name() {
                assert_eq!(std::env::var("CI_RUNNER_SENTINEL").unwrap(), "inherited environment");
                assert!(std::env::var_os("FAIL_POSTGRES").is_none());
            }
        )* }
    }
    pg!(p0, p1, p2, p3, p4, p5, p6, p7, p8, p9, p10);
}}
''',
            encoding="utf-8",
        )
        subprocess.run(
            ["rustc", "--edition=2021", "--test", str(fixture), "-o", str(cls.binary)],
            check=True,
        )

    @classmethod
    def tearDownClass(cls):
        cls.storage.cleanup()

    def invoke(self, **extra_env):
        output = self.root / self.id().rsplit(".", 1)[-1]
        env = {
            **os.environ,
            "CI_RUNNER_SENTINEL": "inherited environment",
            "EXPECTED_CWD": str(REPO),
            **extra_env,
        }
        result = subprocess.run(
            [sys.executable, str(RUNNER), "--output-dir", str(output),
             str(self.binary), "--skip", "existing_skip"],
            cwd=REPO,
            env=env,
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        self.assertTrue((output / "receipt.json").is_file(), result.stderr)
        receipt = json.loads((output / "receipt.json").read_text(encoding="utf-8"))
        self.assertEqual(receipt["executable_sha256"], hashlib.sha256(self.binary.read_bytes()).hexdigest())
        self.assertTrue(receipt["executable_unchanged"])
        self.assertEqual(receipt["cwd"], str(REPO))
        return result, receipt

    def test_same_executable_keeps_cwd_environment_skips_and_both_suites(self):
        result, receipt = self.invoke()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(receipt["ordinary"]["summary"]["passed"], 1)
        self.assertEqual(receipt["ordinary"]["summary"]["ignored"], 11)
        self.assertEqual(receipt["postgres"]["summary"],
                         {"result": "ok", "passed": 11, "failed": 0, "ignored": 0})
        for suite in ["ordinary", "postgres"]:
            self.assertIn("test result: ok.", Path(receipt[suite]["log"]).read_text())

    def test_ordinary_failure_stays_red_but_postgres_still_runs(self):
        result, receipt = self.invoke(FAIL_ORDINARY="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotEqual(receipt["ordinary"]["exit_code"], 0)
        self.assertEqual(receipt["postgres"]["summary"]["passed"], 11)

    def test_postgres_failure_stays_red(self):
        result, receipt = self.invoke(FAIL_POSTGRES="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(receipt["ordinary"]["exit_code"], 0)
        self.assertEqual(receipt["postgres"]["summary"]["failed"], 11)

    def test_zero_partial_or_missing_postgres_summary_cannot_pass(self):
        for passed in [0, 1, 10]:
            self.assertFalse(passing({"exit_code": 0, "summary": {
                "result": "ok", "passed": passed, "failed": 0, "ignored": 0,
            }}, 11, False))
        self.assertFalse(passing({"exit_code": 0, "summary": None}, 11, False))


class WorkflowGuardTests(unittest.TestCase):
    def bash(self):
        bash = shutil.which("bash")
        if os.name == "nt":
            git = Path(shutil.which("git"))
            git_bash = git.parent.parent / "bin/bash.exe"
            if git_bash.is_file():
                bash = str(git_bash)
        self.assertIsNotNone(bash, "workflow regression requires Bash")
        return bash

    def test_both_actual_relevance_gates_keep_embedded_inputs_and_unknowns(self):
        # Execute the workflow's actual shell predicates, replacing only GitHub
        # expression placeholders and the read-only gh response with fixture data.
        workflow = (REPO / ".github/workflows/continuum-rust-tests.yml").read_text()
        blocks = re.findall(r"          files=\$\(gh pr view[\s\S]+?          fi\n", workflow)
        self.assertEqual(len(blocks), 2)
        cases = [
            ("README.md\ndocs/planning/VIRAL-LAUNCH-PLAN.md", False),
            ("docs/genome/coder-eval.jsonl", True),
            ("docs/genome/future-format.md", True),
            ("core/continuum-core/src/lib.rs", True),
            ("__GH_FAILED__", True),
            ("", True),
            ("\n".join(f"docs/{i}.md" for i in range(100)), True),
        ]
        with tempfile.TemporaryDirectory(prefix="continuum-ci-gate-") as directory:
            output = Path(directory) / "output"
            for block in blocks:
                body = "\n".join(line[10:] for line in block.splitlines())
                body = re.sub(r"\$\{\{.*?\}\}", "fixture", body)
                script = 'set -euo pipefail\ngh() { printf "%s\\n" "$CHANGED_FILES"; }\n' + body
                for files, relevant in cases:
                    output.write_text("")
                    result = subprocess.run(
                        [self.bash(), "-c", script],
                        env={**os.environ, "CHANGED_FILES": files, "GITHUB_OUTPUT": str(output)},
                        capture_output=True, text=True, encoding="utf-8",
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(output.read_text().strip(), f"relevant={str(relevant).lower()}")

    def test_actual_binding_guard_rejects_a_new_untracked_export(self):
        workflow = (REPO / ".github/workflows/continuum-rust-tests.yml").read_text()
        block = re.search(r"          drift=\$\(git status[\s\S]+?          fi\n", workflow)
        self.assertIsNotNone(block)
        script = "set -euo pipefail\n" + "\n".join(line[10:] for line in block[0].splitlines())
        # A hook-provided GIT_DIR must not redirect this isolated fixture into
        # the invoking developer checkout. Only these child processes see this.
        env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        with tempfile.TemporaryDirectory(prefix="continuum-ci-drift-") as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "--quiet", str(root)], env=env, check=True)
            def check():
                return subprocess.run([self.bash(), "-c", script], cwd=root, env=env,
                                      capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(check().returncode, 0)
            new_type = root / "protocol/typescript/NewExport.ts"
            new_type.parent.mkdir(parents=True)
            new_type.write_text("export type NewExport = string;\n")
            old_check = subprocess.run(["git", "diff", "--exit-code", "--", "protocol/typescript/"],
                                       cwd=root, env=env, capture_output=True)
            self.assertEqual(old_check.returncode, 0, "the old detector misses this real export")
            result = check()
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("NewExport.ts", result.stdout)


if __name__ == "__main__":
    unittest.main()
