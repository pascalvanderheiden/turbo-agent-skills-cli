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


def make_command_skill_archive(skill_directory, version, output_dir):
    if not SEMVER.fullmatch(version):
        raise ValueError("Command Skill version must be a stable vMAJOR.MINOR.PATCH SemVer")
    skill_directory = Path(skill_directory)
    manifest_path = skill_directory / "manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"missing or invalid Command Skill manifest: {error}") from error
    if manifest.get("schemaVersion") != 1 or manifest.get("id") != "tasc-command":
        raise ValueError("Command Skill manifest has an unsupported schema or ID")
    if manifest.get("version") != version[1:]:
        raise ValueError(
            f"Command Skill version {manifest.get('version')} does not match release {version}"
        )
    skill_path = skill_directory / "SKILL.md"
    adapter_path = skill_directory / "adapters" / "copilot.json"
    if not skill_path.is_file() or not adapter_path.is_file():
        raise ValueError("Command Skill package is missing SKILL.md or Copilot metadata")
    skill = skill_path.read_text(encoding="utf-8")
    if not re.search(rf"^  tasc-version: {re.escape(version[1:])}$", skill, re.MULTILINE):
        raise ValueError("Command Skill frontmatter version does not match the release")
    adapter = json.loads(adapter_path.read_text(encoding="utf-8"))
    if adapter.get("schemaVersion") != 1 or adapter.get("adapter") != "copilot":
        raise ValueError("Command Skill Copilot adapter metadata is invalid")

    archive = Path(output_dir) / f"tasc-command-skill-{version}.tar.gz"
    archive.parent.mkdir(parents=True, exist_ok=True)
    with archive.open("wb") as output:
        with gzip.GzipFile(filename="", fileobj=output, mode="wb", mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.GNU_FORMAT) as bundle:
                for path in sorted(skill_directory.rglob("*")):
                    if path.is_symlink():
                        raise ValueError(f"Command Skill package cannot contain symlinks: {path}")
                    if not path.is_file():
                        continue
                    relative = path.relative_to(skill_directory)
                    info = tarfile.TarInfo(f"tasc-command/{relative.as_posix()}")
                    info.size = path.stat().st_size
                    info.mode = 0o644
                    info.uid = 0
                    info.gid = 0
                    info.uname = ""
                    info.gname = ""
                    info.mtime = 0
                    with path.open("rb") as contents:
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

    command_skill_archives = sorted(directory.glob("tasc-command-skill-v*.tar.gz"))
    if version:
        command_skill_path = directory / f"tasc-command-skill-{version}.tar.gz"
        expected_command_skill_version = version.removeprefix("v")
    elif len(command_skill_archives) == 1:
        command_skill_path = command_skill_archives[0]
        match = SEMVER.fullmatch(command_skill_path.name.removeprefix("tasc-command-skill-").removesuffix(".tar.gz"))
        if not match:
            raise ValueError("Command Skill package filename must use stable SemVer")
        expected_command_skill_version = command_skill_path.name.removeprefix(
            "tasc-command-skill-v"
        ).removesuffix(".tar.gz")
    else:
        raise ValueError("release assets must contain exactly one Command Skill package")
    try:
        with tarfile.open(command_skill_path, "r:gz") as command_skill:
            names = set(command_skill.getnames())
            manifest_member = command_skill.extractfile("tasc-command/manifest.json")
            skill_member = command_skill.extractfile("tasc-command/SKILL.md")
            adapter_member = command_skill.extractfile("tasc-command/adapters/copilot.json")
            if not manifest_member or not skill_member or not adapter_member:
                raise ValueError("Command Skill archive is missing a required package file")
            manifest = json.load(manifest_member)
            skill = skill_member.read().decode("utf-8")
            adapter = json.load(adapter_member)
    except (OSError, tarfile.TarError, KeyError, json.JSONDecodeError) as error:
        raise ValueError(f"missing or invalid Command Skill package: {error}") from error
    if (
        len(names) != 3
        or manifest.get("version") != expected_command_skill_version
        or not re.search(
            rf"^  tasc-version: {re.escape(expected_command_skill_version)}$",
            skill,
            re.MULTILINE,
        )
        or adapter.get("schemaVersion") != 1
        or adapter.get("adapter") != "copilot"
    ):
        raise ValueError("Command Skill archive contents or version do not match the release")

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

    command_skill = commands.add_parser("command-skill")
    command_skill.add_argument("--source-dir", default="skills/tasc-command")
    command_skill.add_argument("--version", required=True)
    command_skill.add_argument("--output-dir", default="dist")

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
        elif args.command == "command-skill":
            print(make_command_skill_archive(args.source_dir, args.version, args.output_dir))
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
