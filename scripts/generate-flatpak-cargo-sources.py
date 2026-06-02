#!/usr/bin/env python3
"""Generate Flatpak cargo sources from Cargo.lock.

This intentionally covers the crates.io subset used by this project. Local path
patches, such as the vendored pipewire crates, are copied by the Flatpak dir
source and are therefore skipped here.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

CRATES_IO_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
CRATES_IO_BASE = "https://static.crates.io/crates"


def parse_lockfile(path: Path) -> list[dict[str, str]]:
    packages: list[dict[str, str]] = []
    current: dict[str, str] | None = None

    for line in path.read_text(encoding="utf-8").splitlines():
        if line == "[[package]]":
            if current is not None:
                packages.append(current)
            current = {}
            continue

        if current is None:
            continue

        match = re.match(r'^(name|version|source|checksum) = "(.+)"$', line)
        if match:
            current[match.group(1)] = match.group(2)

    if current is not None:
        packages.append(current)

    return packages


def crate_sources(packages: list[dict[str, str]]) -> list[dict[str, str]]:
    sources: list[dict[str, str]] = []
    seen: set[tuple[str, str]] = set()

    for package in packages:
        if package.get("source") != CRATES_IO_SOURCE:
            continue

        name = package["name"]
        version = package["version"]
        checksum = package.get("checksum")
        if checksum is None:
            raise RuntimeError(f"missing checksum for {name} {version}")

        key = (name, version)
        if key in seen:
            continue
        seen.add(key)

        dest = f"cargo/vendor/{name}-{version}"
        sources.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"{CRATES_IO_BASE}/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": dest,
            }
        )
        sources.append(
            {
                "type": "inline",
                "contents": json.dumps(
                    {"package": checksum, "files": {}},
                    separators=(",", ":"),
                ),
                "dest": dest,
                "dest-filename": ".cargo-checksum.json",
            }
        )

    sources.append(
        {
            "type": "inline",
            "contents": (
                '[source.vendored-sources]\n'
                'directory = "cargo/vendor"\n\n'
                '[source.crates-io]\n'
                'replace-with = "vendored-sources"\n'
            ),
            "dest": "cargo",
            "dest-filename": "config.toml",
        }
    )

    return sources


def main() -> int:
    if len(sys.argv) not in {2, 4} or (len(sys.argv) == 4 and sys.argv[2] != "-o"):
        print(
            "usage: generate-flatpak-cargo-sources.py Cargo.lock [-o output.json]",
            file=sys.stderr,
        )
        return 2

    lockfile = Path(sys.argv[1])
    output = Path(sys.argv[3]) if len(sys.argv) == 4 else None
    data = json.dumps(crate_sources(parse_lockfile(lockfile)), indent=2)
    data += "\n"

    if output is None:
        print(data, end="")
    else:
        output.write_text(data, encoding="utf-8")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
