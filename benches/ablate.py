#!/usr/bin/env python3
"""Replay independent production patches and compare all eight feature subsets."""
import argparse
import datetime
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
BASE = "4a3888649e89a0216586a36fadcdb9dc76a6efa0"
PATCHES = [
    ("borrowed", "86d88c9", "be8e2de"),
    ("clear", "be8e2de", "78f03bb"),
    ("iterator", "78f03bb", "bade8b4"),
]


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name")
    parser.add_argument("--variants", default="000,100,010,001,110,101,011,111")
    parser.add_argument("--harness-ref", help="use benchmark sources from a Git revision instead of the working tree")
    args = parser.parse_args()
    if not args.name.replace("-", "").replace("_", "").isalnum():
        parser.error("name must be alphanumeric, with optional dashes/underscores")
    variants = args.variants.split(",")
    valid = {"".join(bits) for bits in itertools.product("01", repeat=3)}
    if len(set(variants)) != len(variants) or any(v not in valid for v in variants):
        parser.error("variants must be distinct three-bit values: borrowed, clear, iterator")
    results = ROOT / "benches/results"
    results.mkdir(exist_ok=True)
    source_root = ROOT / "target/ablation-sources"
    source_root.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ)
    environment.setdefault("MM_SAMPLES", "5")
    environment.setdefault("MM_MILLIS", "3")
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/ablation-build")
    patches = [(name, git("rev-parse", before).decode().strip(), git("rev-parse", after).decode().strip(),
                git("diff", before, after, "--", "src")) for name, before, after in PATCHES]
    source_files = git("ls-tree", "-r", "--name-only", BASE, "src").decode().splitlines()
    for bits in variants:
        name = f"{args.name}-{bits}"
        source = source_root / name
        output = results / f"{name}.csv"
        if source.exists() or any(results.glob(f"{name}.*")):
            raise SystemExit(f"Refusing to overwrite {name}; choose a new name")
        source.mkdir()
        for path in source_files:
            dest = source / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(git("show", f"{BASE}:{path}"))
        for path in ["Cargo.toml", "README.md"]:
            shutil.copy2(ROOT / path, source / path)
        for enabled, (_, _, _, patch) in zip(bits, patches):
            if enabled == "1":
                subprocess.run(["git", "apply", "--directory", str(source.relative_to(ROOT))],
                               cwd=ROOT, input=patch, check=True)
        if args.harness_ref:
            for path in git("ls-tree", "-r", "--name-only", args.harness_ref, "benches").decode().splitlines():
                if Path(path).suffix in {".rs", ".py", ".toml", ".lock"}:
                    dest = source / path
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    dest.write_bytes(git("show", f"{args.harness_ref}:{path}"))
        else:
            for path in (ROOT / "benches").rglob("*"):
                relative = path.relative_to(ROOT / "benches")
                if path.is_file() and relative.parts[0] not in {"target", "results", "__pycache__"}:
                    dest = source / "benches" / relative
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(path, dest)
        # Run the same invariant model on every variant, using the owned loader
        # where the additive borrowed API is intentionally absent.
        model_path = source / "src/unsync/cache/model.rs"
        model_path.parent.mkdir(parents=True, exist_ok=True)
        model = git("show", f"{patches[2][2]}:src/unsync/cache/model.rs").decode()
        cache_path = source / "src/unsync/cache.rs"
        cache = cache_path.read_text()
        if "mod model;" not in cache:
            cache_path.write_text(cache + "\n#[cfg(test)]\nmod model;\n")
        if bits[0] == "0":
            adapter = source / "benches/support/mod.rs"
            text = adapter.read_text()
            old = "get_or_insert_with_ref::<K::Query, _>(key.borrow(), || 7)"
            assert text.count(old) == 1
            adapter.write_text(text.replace(old, "get_or_insert_with(key.clone(), || 7)"))
            model = model.replace("get_or_insert_with_ref(&key, || value)", "get_or_insert_with(key, || value)")
        model_path.write_text(model)
        paths = [p for p in source.rglob("*") if p.is_file() and p.suffix in {".rs", ".toml", ".lock"}]
        metadata = {
            "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "orchestrator_commit": git("rev-parse", "HEAD").decode().strip(),
            "base": BASE,
            "harness_ref": git("rev-parse", args.harness_ref).decode().strip() if args.harness_ref else "working-tree",
            "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "patches": [{"feature": n, "before": b, "after": a} for bit, (n, b, a, _) in zip(bits, patches) if bit == "1"],
            "variant": bits,
            "rustc": subprocess.check_output(["rustc", "-Vv"], text=True).strip(),
            "platform": platform.platform(),
            "machine": platform.machine(),
            "settings": {k: v for k, v in environment.items() if k.startswith("MM_")},
            "sha256": {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
        }
        commands = [
            ["cargo", "test", "--lib", "--all-features"],
            ["cargo", "test", "--release", "--lib", "--all-features"],
            ["cargo", "test", "--locked", "--manifest-path", "benches/Cargo.toml", "--bin", "compare"],
        ]
        metadata["test_commands"] = commands
        print(f"Testing and comparing {name}", flush=True)
        try:
            with (results / f"{name}-tests.txt").open("w") as log:
                for command in commands:
                    subprocess.run(command, cwd=source, env=environment, stdout=log, stderr=log, check=True)
            command = ["cargo", "run", "--release", "--locked", "--manifest-path", "benches/Cargo.toml", "--bin", "compare"]
            metadata["benchmark_command"] = command
            with output.open("w") as stdout:
                subprocess.run(command, cwd=source, env=environment, stdout=stdout, check=True)
            metadata["exit_code"] = 0
        except subprocess.CalledProcessError as error:
            metadata["exit_code"] = error.returncode
            raise
        finally:
            metadata["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
            output.with_suffix(".json").write_text(json.dumps(metadata, indent=2) + "\n")
        with output.with_suffix(".md").open("w") as stdout:
            subprocess.run([sys.executable, ROOT / "benches/summarize.py", output], stdout=stdout, check=True)


if __name__ == "__main__":
    main()
