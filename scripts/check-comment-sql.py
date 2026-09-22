#!/usr/bin/env python3
"""Compile comment-cache SQL against a fresh schema and verify offline metadata."""
import argparse
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
CRATE = ROOT / "rust/crates/viewer-comments"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true", help="Regenerate checked-in query metadata")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="nagametv-sql-") as temporary:
        directory = Path(temporary)
        database = directory / "schema.sqlite3"
        with sqlite3.connect(database) as connection:
            for name in ("schema.sql", "schema-v2.sql"):
                connection.executescript((CRATE / "src/cache" / name).read_text())
        metadata = directory / ".sqlx"
        metadata.mkdir()
        env = dict(os.environ, DATABASE_URL=f"sqlite://{database}", SQLX_OFFLINE="false",
                   SQLX_OFFLINE_DIR=str(metadata))
        env.setdefault("CARGO_TARGET_DIR", str(ROOT / "build/cargo"))
        subprocess.run(["cargo", "check", "--manifest-path", str(CRATE / "Cargo.toml"),
                        "--locked", "--features", "network", "--tests"], env=env, check=True, cwd=ROOT)
        generated = {path.name: path.read_bytes() for path in metadata.glob("query-*.json")}
        if not generated:
            raise SystemExit("SQLx generated no query metadata")
        checked = CRATE / ".sqlx"
        existing = {path.name: path.read_bytes() for path in checked.glob("query-*.json")}
        if args.update:
            checked.mkdir(exist_ok=True)
            for name in existing.keys() - generated.keys():
                (checked / name).unlink()
            for name, content in generated.items():
                (checked / name).write_bytes(content)
        elif generated != existing:
            raise SystemExit("Comment SQL metadata is stale; run scripts/check-comment-sql.py --update")
        print(f"Verified {len(generated)} comment-cache SQL queries")


if __name__ == "__main__":
    main()
