"""Offline migration contract; synthetic databases only."""
import importlib.util
from contextlib import closing
from pathlib import Path
import sqlite3
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "migration", Path(__file__).with_name("2026-11-12-training-learned.py"))
migration = importlib.util.module_from_spec(spec)
spec.loader.exec_module(migration)


class MigrationTests(unittest.TestCase):
    def test_dry_run_apply_reopen_and_replay_preserve_exact_values(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.db"
            with closing(sqlite3.connect(path)) as db, db:
                db.execute("CREATE TABLE training_trigger_submissions (id TEXT PRIMARY KEY, learned INTEGER, payload TEXT)")
                db.executemany("INSERT INTO training_trigger_submissions VALUES (?, ?, ?)",
                               [("old", None, None), ("zero", 0, "null"),
                                ("done", 4294967295, '{"nested":null}')])
            self.assertEqual(migration.migrate(path)["historical_nulls"], 1)
            self.assertEqual(migration.migrate(path, True)["changed"], 1)
            with closing(sqlite3.connect(path)) as db, db:
                self.assertEqual(db.execute("SELECT * FROM training_trigger_submissions ORDER BY id").fetchall(),
                                 [("done", 4294967295, '{"nested":null}'), ("old", 0, None), ("zero", 0, "null")])
            self.assertEqual(migration.migrate(path, True)["changed"], 0)

    def test_invalid_progress_refuses_without_partial_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.db"
            with closing(sqlite3.connect(path)) as db, db:
                db.execute("CREATE TABLE training_trigger_submissions (learned INTEGER)")
                db.executemany("INSERT INTO training_trigger_submissions VALUES (?)", [(None,), (-1,)])
            with self.assertRaises(ValueError):
                migration.migrate(path, True)
            with closing(sqlite3.connect(path)) as db, db:
                self.assertEqual(db.execute("SELECT learned FROM training_trigger_submissions").fetchall(), [(None,), (-1,)])


if __name__ == "__main__":
    unittest.main()
