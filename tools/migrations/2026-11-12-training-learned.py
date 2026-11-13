#!/usr/bin/env python3
"""One-time, offline migration before strict ORM decoding; never runtime code.

Stop the database's serving owner before --apply. Pass each database explicitly;
this tool neither discovers homes nor touches AIRC authority. Without --apply it
opens read-only. Only the historical, documented pre-learning NULL is migrated.
"""
import argparse
from contextlib import closing
import json
from pathlib import Path
import sqlite3


def migrate(path: Path, apply: bool = False) -> dict:
    path = path.resolve(strict=True)
    mode = "rw" if apply else "ro"
    with closing(sqlite3.connect(f"{path.as_uri()}?mode={mode}", uri=True)) as db, db:
        # Hold the writer transaction across validation and update. Any failure
        # rolls back, including the post-update cardinality check.
        db.execute("BEGIN IMMEDIATE" if apply else "BEGIN")
        total, pending, invalid = db.execute("""
            SELECT COUNT(*),
                   COUNT(CASE WHEN learned IS NULL THEN 1 END),
                   COUNT(CASE WHEN learned IS NOT NULL AND
                     (typeof(learned) != 'integer' OR learned < 0 OR learned > 4294967295)
                     THEN 1 END)
            FROM training_trigger_submissions
        """).fetchone()
        if invalid:
            raise ValueError(f"refusing {invalid} invalid non-NULL progress values")
        changed = 0
        if apply:
            changed = db.execute("""
                UPDATE training_trigger_submissions SET learned = ? WHERE learned IS NULL
            """, (0,)).rowcount
            remaining = db.execute("""
                SELECT COUNT(*) FROM training_trigger_submissions WHERE learned IS NULL
            """).fetchone()[0]
            if changed != pending or remaining:
                raise RuntimeError("migration cardinality mismatch; transaction rolled back")
        return {"database": str(path), "rows": total, "historical_nulls": pending,
                "changed": changed, "applied": apply}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database", type=Path)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    print(json.dumps(migrate(args.database, args.apply)))
