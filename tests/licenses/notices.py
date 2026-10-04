"""Lists the licenses of everything Lectrix ships, checks that each is AGPL-compatible
(AGENTS.md section 2), and writes THIRD_PARTY_LICENSES.md with their texts.

Shipped code: the Rust crates compiled into lectrix.exe for Windows (normal dependencies of
the app crate, from `cargo metadata`), and the npm packages bundled into the frontend
(target/frontend-packages.json, written by `npm run build`).

    python tests/licenses/notices.py           # write THIRD_PARTY_LICENSES.md
    python tests/licenses/notices.py --check   # fail if a license is not allowed or the
                                                # file is out of date (CI)
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "THIRD_PARTY_LICENSES.md"
TARGET = "x86_64-pc-windows-msvc"

# AGPL-3.0-compatible licenses (SPDX ids). Anything else fails the check and needs a
# decision (AGENTS.md section 10: licensing questions go to the user).
ALLOWED = {
    "MIT", "MIT-0", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause",
    "BSD-3-Clause", "ISC", "Zlib", "0BSD", "Unlicense", "CC0-1.0", "BSL-1.0", "MPL-2.0",
    "Unicode-3.0", "Unicode-DFS-2016", "AGPL-3.0", "AGPL-3.0-only", "AGPL-3.0-or-later",
    "LGPL-2.1-or-later", "LGPL-3.0-or-later", "GPL-3.0-or-later",
}

# Packages whose manifest names no license although they ship one: name -> (license,
# where that was checked). Re-check when the version changes.
OVERRIDES = {
    "svelte-toolbelt": ("MIT", "its LICENSE file (MIT License, Hunter Johnston, Thomas G. Lopes), 0.10.6"),
}

LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|unlicense)([-_.].*)?$", re.IGNORECASE)


def allowed(expression: str) -> bool:
    """True if the SPDX expression can be satisfied with allowed licenses."""
    expr = expression.replace("/", " OR ")
    # Keep "X WITH Y" together as one term.
    expr = re.sub(r"\s+WITH\s+", " WITH_", expr)

    def term_ok(term: str) -> bool:
        return term.replace("WITH_", "WITH ").strip("()") in ALLOWED

    for alternative in re.split(r"\s+OR\s+", expr):
        terms = [t for t in re.split(r"\s+AND\s+", alternative.strip("() "))]
        if all(term_ok(t.strip("() ")) for t in terms):
            return True
    return False


def license_texts(directory: Path) -> list[tuple[str, str]]:
    texts = []
    if directory.is_dir():
        for f in sorted(directory.iterdir()):
            if f.is_file() and LICENSE_FILE.match(f.name):
                texts.append((f.name, f.read_text(encoding="utf-8", errors="replace").strip()))
    return texts


def rust_packages() -> list[dict]:
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--filter-platform", TARGET, "--locked"],
            cwd=ROOT, check=True, capture_output=True, encoding="utf-8",
        ).stdout
    )
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "lectrix")
    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            # Only what is compiled into the binary: no dev- or build-dependencies.
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    out = []
    for pid in seen:
        p = packages[pid]
        if p["source"] is None and p["name"] in {"lectrix", "pdf-core"}:
            continue  # Lectrix itself
        directory = Path(p["manifest_path"]).parent
        texts = license_texts(directory)
        if p.get("license_file"):
            lf = directory / p["license_file"]
            if lf.is_file():
                texts.append((lf.name, lf.read_text(encoding="utf-8", errors="replace").strip()))
        out.append({
            "kind": "Rust crate", "name": p["name"], "version": p["version"],
            "license": p.get("license") or "", "texts": texts,
        })
    return out


def npm_packages() -> list[dict]:
    listing = ROOT / "target" / "frontend-packages.json"
    if not listing.exists():
        sys.exit(f"{listing} is missing: run `npm run build` first")
    out = {}
    for entry in json.loads(listing.read_text(encoding="utf-8")):
        manifest = json.loads((Path(entry["dir"]) / "package.json").read_text(encoding="utf-8"))
        lic = manifest.get("license") or ""
        if isinstance(lic, dict):
            lic = lic.get("type", "")
        if not lic and entry["name"] in OVERRIDES:
            lic = OVERRIDES[entry["name"]][0]
        out[(entry["name"], entry["version"])] = {
            "kind": "npm package", "name": entry["name"], "version": entry["version"],
            "license": lic, "texts": license_texts(Path(entry["dir"])),
        }
    return list(out.values())


def render(packages: list[dict]) -> str:
    lines = [
        "# Third-party licenses",
        "",
        "Generated by `python tests/licenses/notices.py` from what Lectrix ships: the Rust",
        f"crates compiled into `lectrix.exe` ({TARGET}) and the npm packages bundled into its",
        "frontend. Do not edit by hand. `THIRD_PARTY_NOTICES.md` summarizes the main components,",
        "including MuPDF and its bundled libraries, which are built from source by `mupdf-sys`.",
        "",
        "| Package | Version | License |",
        "| --- | --- | --- |",
    ]
    for p in packages:
        lines.append(f"| {p['name']} ({p['kind']}) | {p['version']} | {p['license'] or 'see text'} |")
    lines += ["", "## License texts", ""]
    # One copy of each distinct text, with the packages that carry it.
    by_text: dict[str, list[str]] = {}
    for p in packages:
        for _, text in p["texts"]:
            by_text.setdefault(text, []).append(f"{p['name']} {p['version']}")
    for text, owners in sorted(by_text.items(), key=lambda kv: (sorted(kv[1])[0], kv[0])):
        lines.append(f"### {', '.join(sorted(set(owners)))}")
        lines += ["", "```text", text.replace("```", "'''"), "```", ""]
    return "\n".join(lines) + "\n"


def main() -> None:
    check = "--check" in sys.argv
    packages = sorted(rust_packages() + npm_packages(), key=lambda p: (p["name"].lower(), p["version"]))
    problems = [p for p in packages if not allowed(p["license"])]
    for p in problems:
        print(f"not allowed: {p['name']} {p['version']} ({p['kind']}): {p['license'] or 'no license field'}")
    missing = [p for p in packages if not p["texts"]]
    for p in missing:
        print(f"note: no license file in {p['name']} {p['version']} ({p['license']})")
    text = render(packages)
    print(f"{len(packages)} packages, {len(problems)} not allowed, {len(missing)} without a license file")
    if check:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if current != text:
            print(f"{OUT.name} is out of date: run python tests/licenses/notices.py")
            sys.exit(1)
    else:
        OUT.write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {OUT.relative_to(ROOT)}")
    if problems:
        sys.exit(1)


if __name__ == "__main__":
    main()
