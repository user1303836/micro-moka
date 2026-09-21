#!/usr/bin/env python3
"""Summarize compare.csv medians; 5% bands are descriptive, not significance tests."""
import collections
import csv
import statistics
import sys


def summarize(path):
    samples = collections.defaultdict(lambda: collections.defaultdict(list))
    with open(path, newline="") as source:
        for row in csv.DictReader(source):
            case = tuple(row[k] for k in ("key", "hasher", "capacity", "operation"))
            samples[case][row["library"]].append(float(row["ns_per_op"]))
    counts = collections.Counter()
    print("| Key | Hasher | Capacity | Operation | Micro ns/op | vs v1.2 | Best peer | vs peer | Micro MAD % |")
    print("|---|---|---:|---|---:|---:|---|---:|---:|")
    for case, libs in samples.items():
        medians = {name: statistics.median(values) for name, values in libs.items()}
        micro = medians["micro"]
        base = medians.get("baseline", micro)
        peers = {k: v for k, v in medians.items() if k not in ("micro", "baseline")}
        peer = min(peers, key=peers.get) if peers else "-"
        ratio = micro / peers[peer] if peers else 1.0
        baseline_ratio = micro / base
        mad = statistics.median(abs(v - micro) for v in libs["micro"]) / micro * 100
        counts["baseline_win" if baseline_ratio < .95 else "baseline_loss" if baseline_ratio > 1.05 else "baseline_tie"] += 1
        counts["peer_win" if ratio < .95 else "peer_loss" if ratio > 1.05 else "peer_tie"] += 1
        print(f"| {' | '.join(case)} | {micro:.2f} | {baseline_ratio:.3f}x | {peer} | {ratio:.3f}x | {mad:.1f} |")
    print("\nLower ratios are better. +/-5% is labeled a tie, not statistical equivalence.")
    print(dict(counts))
    return counts


if __name__ == "__main__":
    summarize(sys.argv[1])
