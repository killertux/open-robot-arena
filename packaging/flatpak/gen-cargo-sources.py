#!/usr/bin/env python3
"""Regenerate cargo-sources.json for the Flatpak build.

A Flatpak sandbox starts with an empty CARGO_HOME, so `cargo build --offline`
cannot resolve a single crate unless every crate is already unpacked in a
vendored directory.  flatpak-builder can do that: each crates.io archive is
fetched as a source and extracted under `cargo/vendor/`, and
`cargo/config` points Cargo at it.

This is the crates.io-only subset of flatpak-builder-tools'
`flatpak-cargo-generator.py` (MIT).  The workspace's Cargo.lock has no git
dependencies, so no git checkout or network lookup is needed to produce the
list: the URL is derived from name/version and the sha256 is the `checksum`
Cargo already recorded in the lockfile.

    ./packaging/flatpak/gen-cargo-sources.py            # write the file
    ./packaging/flatpak/gen-cargo-sources.py --check    # fail if stale

Run it after `cargo update` changes engine/Cargo.lock.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
LOCK = ROOT / "engine" / "Cargo.lock"
OUT = pathlib.Path(__file__).resolve().with_name("cargo-sources.json")

# Keep in step with flatpak-cargo-generator.py: sources are extracted into the
# build directory's cargo/ and the generated cargo/config is CARGO_HOME/config.
CARGO_CRATES = "cargo/vendor"


def generate() -> list[dict]:
    lock = tomllib.loads(LOCK.read_text(encoding="utf-8"))
    sources: list[dict] = []
    crates = 0
    for package in lock["package"]:
        source = package.get("source")
        if source is None:
            continue  # a workspace crate, built from this source tree
        if not source.startswith("registry+"):
            raise SystemExit(
                f"gen-cargo-sources.py: {package['name']} comes from {source}; "
                "only crates.io registry sources are supported"
            )
        name = package["name"]
        version = package["version"]
        checksum = package.get("checksum")
        if not checksum:
            raise SystemExit(
                f"gen-cargo-sources.py: {name} {version} has no checksum in Cargo.lock"
            )
        dest = f"{CARGO_CRATES}/{name}-{version}"
        sources.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": dest,
            }
        )
        sources.append(
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": dest,
                "dest-filename": ".cargo-checksum.json",
            }
        )
        crates += 1

    if not sources:
        raise SystemExit("gen-cargo-sources.py: Cargo.lock has no registry packages")

    sources.append(
        {
            "type": "inline",
            "contents": (
                '[source.vendored-sources]\n'
                f'directory = "{CARGO_CRATES}"\n'
                "\n"
                "[source.crates-io]\n"
                'replace-with = "vendored-sources"\n'
            ),
            "dest": "cargo",
            "dest-filename": "config",
        }
    )
    print(f"gen-cargo-sources.py: {crates} crates.io crates -> {OUT.name}", file=sys.stderr)
    return sources


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="do not write; fail if cargo-sources.json is out of date",
    )
    args = parser.parse_args()

    rendered = json.dumps(generate(), indent=4) + "\n"
    if args.check:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if current != rendered:
            print(
                "gen-cargo-sources.py: cargo-sources.json is stale; rerun without --check",
                file=sys.stderr,
            )
            return 1
        print("gen-cargo-sources.py: cargo-sources.json is up to date", file=sys.stderr)
        return 0

    OUT.write_text(rendered, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
