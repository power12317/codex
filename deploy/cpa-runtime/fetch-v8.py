#!/usr/bin/env python3
"""Fetch Codex's pinned V8 artifacts using the same trust chain as CI."""

import argparse
import hashlib
from pathlib import Path
import subprocess
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--target",
    required=True,
    choices=[
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
    ],
)
parser.add_argument("--output-dir", required=True, type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
version = subprocess.check_output(
    [
        "python3",
        str(root / ".github/scripts/rusty_v8_bazel.py"),
        "resolved-v8-crate-version",
    ],
    text=True,
).strip()
name = f"rusty_v8_ptrcomp_sandbox_release_{args.target}.sha256"
trusted = (
    root
    / f"third_party/v8/rusty_v8_{version.replace('.', '_')}_release_manifests.sha256"
)
checksums = dict(
    line.split(maxsplit=1)[::-1]
    for line in trusted.read_text().splitlines()
    if line.strip()
)
base = f"https://github.com/openai/codex/releases/download/rusty-v8-v{version}"
args.output_dir.mkdir(parents=True, exist_ok=True)
manifest = urllib.request.urlopen(f"{base}/{name}", timeout=120).read()
if hashlib.sha256(manifest).hexdigest() != checksums[name]:
    raise SystemExit("V8 release manifest checksum mismatch")
entries = [
    line.split(maxsplit=1) for line in manifest.decode().splitlines() if line.strip()
]
expected = {
    f"librusty_v8_ptrcomp_sandbox_release_{args.target}.a.gz",
    f"src_binding_ptrcomp_sandbox_release_{args.target}.rs",
}
if len(entries) != 2 or {entry[1] for entry in entries} != expected:
    raise SystemExit("Unexpected V8 release manifest entries")
for checksum, filename in entries:
    destination = args.output_dir / filename
    if (
        destination.exists()
        and hashlib.sha256(destination.read_bytes()).hexdigest() == checksum
    ):
        continue
    temporary = destination.with_suffix(destination.suffix + ".download")
    digest = hashlib.sha256()
    with (
        urllib.request.urlopen(f"{base}/{filename}", timeout=120) as source,
        temporary.open("wb") as output,
    ):
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
            output.write(chunk)
    if digest.hexdigest() != checksum:
        temporary.unlink()
        raise SystemExit(f"V8 artifact checksum mismatch: {filename}")
    temporary.replace(destination)
print(f"Verified Codex V8 {version} artifacts for {args.target} in {args.output_dir}")
