#!/usr/bin/env python3
"""Stamp the Docker build copy with the official release version."""

from pathlib import Path
import re
import tomllib


def prepare_release(root: Path, version: str) -> None:
    if not re.fullmatch(r"0\.[1-9][0-9]*\.[0-9]+", version):
        raise ValueError(f"Expected an official stable version, got {version!r}")
    manifest = root / "codex-rs/Cargo.toml"
    lockfile = root / "codex-rs/Cargo.lock"
    text = manifest.read_text()
    workspace = tomllib.loads(text)["workspace"]
    previous = workspace["package"]["version"]
    if previous not in ("0.0.0", version):
        raise ValueError(f"Unexpected workspace version {previous!r}")

    # Only workspace-inherited path packages change; registry/git packages keep
    # their exact versions, checksums and dependency resolution.
    names = set()
    for member in workspace["members"]:
        for directory in manifest.parent.glob(member):
            package = tomllib.loads((directory / "Cargo.toml").read_text())["package"]
            if package.get("version") == {"workspace": True}:
                names.add(package["name"])
    blocks = lockfile.read_text().split("[[package]]")
    for index in range(1, len(blocks)):
        package = tomllib.loads("[[package]]" + blocks[index])["package"][0]
        if package["name"] in names and "source" not in package:
            if package["version"] not in (previous, version):
                raise ValueError(f"Unexpected lockfile version for {package['name']}")
            blocks[index] = re.sub(
                r'^version = "[^"]+"$', f'version = "{version}"',
                blocks[index], count=1, flags=re.MULTILINE,
            )
    lock_text = "[[package]]".join(blocks)
    for name in names:
        lock_text = lock_text.replace(f'"{name} {previous}"', f'"{name} {version}"')
    text, count = re.subn(
        r'(\[workspace\.package\]\s*\nversion\s*=\s*")[^"]+(")',
        lambda match: match[1] + version + match[2],
        text, count=1,
    )
    if count != 1:
        raise ValueError("Could not locate workspace package version")
    # Validate both outputs before writing either build file.
    assert tomllib.loads(text)["workspace"]["package"]["version"] == version
    tomllib.loads(lock_text)
    manifest.write_text(text)
    lockfile.write_text(lock_text)


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[2]
    version = (Path(__file__).parent / "release-version").read_text().strip()
    prepare_release(root, version)
    print(f"Prepared official Codex client version {version}")
