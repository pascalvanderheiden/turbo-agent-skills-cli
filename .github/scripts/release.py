#!/usr/bin/env python3

import argparse
import gzip
import hashlib
import json
import re
import sys
import tarfile
from pathlib import Path


TARGETS = {
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
}
SEMVER = re.compile(r"^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as contents:
        for chunk in iter(lambda: contents.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_tag(tag, package_version, protected):
    match = SEMVER.fullmatch(tag)
    if not match:
        raise ValueError("release tag must be a stable vMAJOR.MINOR.PATCH SemVer")
    if tag[1:] != package_version:
        raise ValueError(
            f"release tag {tag} does not match tasc package version {package_version}"
        )
    if not protected:
        raise ValueError("release tag is not protected by a ruleset")


def make_archive(binary, target, version, output_dir):
    if target not in TARGETS:
        raise ValueError(f"unsupported release target: {target}")
    if not SEMVER.fullmatch(version):
        raise ValueError("archive version must be a stable vMAJOR.MINOR.PATCH SemVer")
    binary = Path(binary)
    if not binary.is_file():
        raise ValueError(f"release binary does not exist: {binary}")

    filename = "tasc.exe" if target.endswith("-msvc") else "tasc"
    archive = Path(output_dir) / f"tasc-{version}-{target}.tar.gz"
    archive.parent.mkdir(parents=True, exist_ok=True)
    info = tarfile.TarInfo(filename)
    info.size = binary.stat().st_size
    info.mode = 0o755
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    info.mtime = 0

    with archive.open("wb") as output:
        with gzip.GzipFile(filename="", fileobj=output, mode="wb", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.GNU_FORMAT) as bundle:
                with binary.open("rb") as contents:
                    bundle.addfile(info, contents)
    return archive


def write_checksums(directory):
    directory = Path(directory)
    assets = sorted(
        (path for path in directory.iterdir() if path.is_file() and path.name != "SHA256SUMS.txt"),
        key=lambda path: path.name,
    )
    if not assets:
        raise ValueError("no release assets found")

    lines = []
    for asset in assets:
        lines.append(f"{sha256(asset)}  {asset.name}")
    checksums = directory / "SHA256SUMS.txt"
    checksums.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return checksums


def validate_assets(directory, version):
    directory = Path(directory)
    archives = sorted(directory.glob("tasc-v*-*.tar.gz"))
    if version and (
        len(archives) != len(TARGETS)
        or any(not archive.name.startswith(f"tasc-{version}-") for archive in archives)
    ):
        raise ValueError(f"release archives must contain exactly five assets for {version}")
    targets = {
        target
        for archive in archives
        for target in TARGETS
        if archive.name.endswith(f"-{target}.tar.gz")
    }
    if targets != TARGETS:
        missing = sorted(TARGETS - targets)
        extra = sorted(targets - TARGETS)
        raise ValueError(f"release archive targets mismatch; missing={missing}, extra={extra}")

    sbom_path = directory / "tasc.cdx.json"
    try:
        sbom = json.loads(sbom_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"missing or invalid CycloneDX SBOM: {error}") from error
    if not sbom.get("bomFormat") == "CycloneDX" or not sbom.get("components"):
        raise ValueError("CycloneDX SBOM must declare bomFormat and components")

    checksum_path = directory / "SHA256SUMS.txt"
    if not checksum_path.is_file():
        raise ValueError("SHA256SUMS.txt is missing")
    expected = {
        f"{sha256(asset)}  {asset.name}"
        for asset in directory.iterdir()
        if asset.is_file() and asset.name != checksum_path.name
    }
    actual = checksum_path.read_text(encoding="utf-8").splitlines()
    if len(actual) != len(expected) or set(actual) != expected:
        raise ValueError("SHA256SUMS.txt does not match the release assets")


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)

    validate = commands.add_parser("validate-tag")
    validate.add_argument("--tag", required=True)
    validate.add_argument("--version", required=True)
    validate.add_argument("--protected", action="store_true")

    package = commands.add_parser("package")
    package.add_argument("--binary", required=True)
    package.add_argument("--target", required=True)
    package.add_argument("--version", required=True)
    package.add_argument("--output-dir", default="dist")

    checksums = commands.add_parser("checksums")
    checksums.add_argument("--directory", default="dist")

    assets = commands.add_parser("validate-assets")
    assets.add_argument("--directory", default="dist")
    assets.add_argument("--version")

    args = parser.parse_args()
    try:
        if args.command == "validate-tag":
            validate_tag(args.tag, args.version, args.protected)
        elif args.command == "package":
            print(make_archive(args.binary, args.target, args.version, args.output_dir))
        elif args.command == "checksums":
            print(write_checksums(args.directory))
        else:
            validate_assets(args.directory, args.version)
    except (OSError, ValueError, tarfile.TarError) as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
