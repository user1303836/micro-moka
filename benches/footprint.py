#!/usr/bin/env python3
"""Compare clean build time and stripped size of a small cache-using application."""
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
PROGRAM = r'''
use micro_moka::unsync::Cache;
use std::hint::black_box;
fn main() {
    let capacity = std::env::args().nth(1).unwrap_or_else(|| "1024".into()).parse::<usize>().unwrap();
    let keys: Vec<_> = (0..capacity).map(|i| format!("{i:024}")).collect();
    let mut cache = Cache::builder().max_capacity(capacity as u64).initial_capacity(capacity).build();
    let mut checksum = 0u64;
    for _ in 0..4 {
        for key in &keys { checksum = checksum.wrapping_add(*LOAD); }
        for key in keys.iter().step_by(2) { cache.remove(key); }
        checksum = checksum.wrapping_add(black_box(&cache).iter().fold(0u64, |a, (_, v)| a.wrapping_add(*v)));
        checksum = checksum.wrapping_add(black_box(&cache).iter().count() as u64);
        for (_, value) in black_box(&cache).iter() { checksum = checksum.wrapping_add(*value); }
        cache.invalidate_all();
    }
    println!("{}", black_box(checksum));
}
'''


def main():
    name = sys.argv[1]
    if not name.replace("-", "").replace("_", "").isalnum():
        raise SystemExit("Use an alphanumeric name, with optional dashes/underscores")
    directory = ROOT / "target/footprint" / name
    output = ROOT / "benches/results" / f"{name}-footprint.json"
    if directory.exists() or output.exists():
        raise SystemExit("Refusing to overwrite a prior measurement")
    directory.mkdir(parents=True)
    record = {"rustc": subprocess.check_output(["rustc", "-Vv"], text=True).strip(),
              "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "variants": {}}
    for variant in ["baseline", "candidate"]:
        app = directory / variant
        (app / "src").mkdir(parents=True)
        dependency = 'version = "=1.2.0"' if variant == "baseline" else f'path = {json.dumps(str(ROOT))}'
        manifest = '[package]\nname = "cache-footprint"\nversion = "0.0.0"\nedition = "2021"\n\n[dependencies]\nmicro-moka = { ' + dependency + ' }\n\n[profile.release]\nlto = true\ncodegen-units = 1\nstrip = "symbols"\n'
        load = "cache.get_or_insert_with(key.clone(), || 7)" if variant == "baseline" else "cache.get_or_insert_with_ref(key.as_str(), || 7)"
        source = PROGRAM.replace("LOAD", load)
        (app / "Cargo.toml").write_text(manifest)
        (app / "src/main.rs").write_text(source)
        record["variants"][variant] = {"manifest": manifest, "source": source, "samples": []}
    for sample in range(3):
        for variant in (["baseline", "candidate"] if sample % 2 == 0 else ["candidate", "baseline"]):
            app = directory / variant
            target = app / f"target-{sample}"
            command = ["cargo", "build", "--release", "--offline", "--manifest-path", str(app / "Cargo.toml")]
            start = time.perf_counter()
            subprocess.run(command, env={**os.environ, "CARGO_TARGET_DIR": str(target)}, check=True)
            elapsed = time.perf_counter() - start
            binary = target / "release/cache-footprint"
            result = subprocess.check_output([binary, "128"], text=True).strip()
            record["variants"][variant]["samples"].append({"seconds": elapsed, "bytes": binary.stat().st_size, "checksum": result})
    for data in record["variants"].values():
        data["median_seconds"] = statistics.median(s["seconds"] for s in data["samples"])
    assert len({s["checksum"] for d in record["variants"].values() for s in d["samples"]}) == 1
    output.write_text(json.dumps(record, indent=2) + "\n")
    for variant, data in record["variants"].items():
        print(variant, data["median_seconds"], data["samples"])


if __name__ == "__main__":
    main()
