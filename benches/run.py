#!/usr/bin/env python3
"""Run a comparison and save raw samples plus source/toolchain provenance."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(path):
    return hashlib.sha256((ROOT / path).read_bytes()).hexdigest()


def main():
    name = sys.argv[1]
    if not name.replace("-", "").replace("_", "").isalnum():
        raise SystemExit("Use an alphanumeric run name with dashes/underscores")
    output = ROOT / "benches" / "results" / f"{name}.csv"
    if output.exists():
        raise SystemExit(f"Refusing to overwrite {output}")
    output.parent.mkdir(exist_ok=True)
    metadata = {
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "commit": command("git", "rev-parse", "HEAD"),
        "status": command("git", "status", "--short"),
        "rustc": command("rustc", "-Vv"),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "settings": {k: v for k, v in os.environ.items() if k.startswith("MM_")},
        "sha256": {p: digest(p) for p in ["src/unsync/cache.rs", "src/unsync/iter.rs", "src/unsync/mod.rs", "benches/compare.rs", "benches/support/mod.rs", "benches/Cargo.lock", "benches/Cargo.toml"]},
    }
    invocation = ["cargo", "run", "--release", "--locked", "--manifest-path", "benches/Cargo.toml", "--bin", "compare"]
    metadata["command"] = invocation
    with output.open("w") as stdout:
        result = subprocess.run(invocation, cwd=ROOT, stdout=stdout, check=False)
    metadata["exit_code"] = result.returncode
    metadata["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    output.with_suffix(".json").write_text(json.dumps(metadata, indent=2) + "\n")
    if result.returncode:
        raise SystemExit(result.returncode)
    with output.with_suffix(".md").open("w") as stdout:
        subprocess.run([sys.executable, "benches/summarize.py", str(output)], cwd=ROOT, stdout=stdout, check=True)


if __name__ == "__main__":
    main()
