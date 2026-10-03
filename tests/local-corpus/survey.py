"""Profiles every PDF under a folder with `pdf-cli survey` (read-only).

    python tests/local-corpus/survey.py "<library folder>" [--out target/test-output/survey/library.jsonl]

Each file is surveyed in its own process with a time limit, so one bad file cannot stop
the run. The folder is only read: pdf-cli opens files for reading and writes nothing next
to them. Output: one JSON object per line (see crates/pdf-cli/src/survey.rs).
"""

import argparse
import concurrent.futures
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
CLI = ROOT / "target" / "release" / ("pdf-cli.exe" if sys.platform == "win32" else "pdf-cli")


def survey(path: pathlib.Path, timeout: float) -> dict:
    try:
        done = subprocess.run(
            [str(CLI), "survey", str(path)],
            capture_output=True,
            timeout=timeout,
            text=True,
            encoding="utf-8",
        )
    except subprocess.TimeoutExpired:
        return {"path": str(path), "error": f"timed out after {timeout:.0f} s"}
    if done.returncode != 0 or not done.stdout.strip():
        return {
            "path": str(path),
            "error": f"exit code {done.returncode}: {done.stderr.strip()[-500:]}",
        }
    return json.loads(done.stdout.strip().splitlines()[-1])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("folder", type=pathlib.Path)
    parser.add_argument(
        "--out", type=pathlib.Path, default=ROOT / "target" / "test-output" / "survey" / "library.jsonl"
    )
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()

    files = sorted(p for p in args.folder.rglob("*") if p.suffix.lower() == ".pdf" and p.is_file())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    print(f"surveying {len(files)} files with {args.jobs} jobs", flush=True)
    with args.out.open("w", encoding="utf-8") as out, concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        for n, result in enumerate(pool.map(lambda p: survey(p, args.timeout), files), 1):
            out.write(json.dumps(result, ensure_ascii=False) + "\n")
            if n % 50 == 0:
                print(f"  {n}/{len(files)}", flush=True)
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
