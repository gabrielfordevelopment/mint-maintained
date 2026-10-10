"""Validate and package releases; remote writes occur only with create-tag."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tomllib
from urllib.error import HTTPError
from urllib.request import Request, urlopen
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo


TARGETS = {
    "x86_64-pc-windows-msvc": "mint.exe",
    "x86_64-unknown-linux-gnu": "mint",
}
NOTICES = (
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
    "assets/icons/LICENSE",
    "assets/icons/DEPENDENCY_LICENSES.txt",
    "assets/fonts/OFL.txt",
)


def stable_version(version):
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise ValueError("Releases require a stable X.Y.Z workspace version.")
    return tuple(map(int, version.split(".")))


def read_version(root):
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    version = manifest["workspace"]["package"]["version"]
    stable_version(version)
    manifests = [manifest]
    for member in manifest["workspace"]["members"]:
        manifests.append(tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf-8")))
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    for member in manifests:
        package = member["package"]
        if package["version"] != {"workspace": True}:
            raise ValueError(f"{package['name']} must inherit the workspace version.")
        entries = [p for p in lock["package"] if p["name"] == package["name"] and "source" not in p]
        if len(entries) != 1 or entries[0]["version"] != version:
            raise ValueError(f"Refresh Cargo.lock for {package['name']} before releasing.")
    return version


def release_notes(root):
    version = read_version(root)
    changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
    sections = re.split(r"^## \[([^\]]+)\][^\n]*\n", changelog, flags=re.MULTILINE)
    matches = [sections[i + 1] for i in range(1, len(sections), 2) if sections[i] == version]
    if len(matches) != 1:
        raise ValueError(f"Expected one CHANGELOG.md section for {version}.")
    notes = matches[0].split("<!-- next-url -->", 1)[0].strip()
    if not notes:
        raise ValueError(f"Release notes for {version} must not be empty.")
    return notes


def github_api(endpoint, data=None, missing_ok=False):
    repository = os.environ["GH_REPO"]
    request = Request(
        f"https://api.github.com/repos/{repository}/{endpoint}",
        data=json.dumps(data).encode() if data is not None else None,
        headers={
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
            "Accept": "application/vnd.github+json",
            "Content-Type": "application/json",
            "User-Agent": "mint-maintained-release",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urlopen(request, timeout=30) as response:
            return json.load(response)
    except HTTPError as error:
        if missing_ok and error.code == 404:
            return None
        raise RuntimeError(f"GitHub API failed with HTTP {error.code}: {endpoint}") from None


def check_remote(version, commit, api=github_api):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("A full release commit SHA is required.")
    current = stable_version(version)
    tag = f"v{version}"
    page = 1
    while True:
        releases = api(f"releases?per_page=100&page={page}")
        if any(item["tag_name"] == tag for item in releases):
            raise ValueError(f"Release {tag} already exists; it will not be overwritten.")
        if len(releases) < 100:
            break
        page += 1
    latest = api("releases/latest", missing_ok=True)
    if latest is None:
        if current <= (0, 2, 10):
            raise ValueError("The first maintained release must exceed the inherited 0.2.10 version.")
    else:
        if not latest["tag_name"].startswith("v"):
            raise ValueError("The latest release does not use a vX.Y.Z tag.")
        if current <= stable_version(latest["tag_name"][1:]):
            raise ValueError("The release version must be newer than the latest published release.")
    ref = api(f"git/ref/tags/{tag}", missing_ok=True)
    if ref is not None:
        target = ref["object"]
        while target["type"] == "tag":
            target = api(f"git/tags/{target['sha']}")["object"]
        if target["type"] != "commit" or target["sha"] != commit:
            raise ValueError(f"Tag {tag} points elsewhere; it will not be moved.")
    return ref is not None


def create_tag(version, commit, api=github_api):
    if not check_remote(version, commit, api):
        api("git/refs", data={"ref": f"refs/tags/v{version}", "sha": commit})


def package(root, target, output):
    version = read_version(root)
    binary = TARGETS[target]
    files = [(root / "target" / target / "dist" / binary, binary)]
    files.extend((root / notice, notice) for notice in NOTICES)
    for source, _ in files:
        if not source.is_file():
            raise ValueError(f"Required release file is missing: {source}")
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f"mint-v{version}-{target}.zip"
    alias = output / f"mint-{target}.zip"
    if archive.exists() or alias.exists():
        raise ValueError("Release archives already exist; use an empty output directory.")
    with ZipFile(archive, "x", compression=ZIP_DEFLATED) as bundle:
        for source, name in files:
            entry = ZipInfo.from_file(source, arcname=name)
            entry.create_system = 3
            entry.external_attr = (stat.S_IFREG | (0o755 if name == binary else 0o644)) << 16
            entry.compress_type = ZIP_DEFLATED
            with source.open("rb") as contents, bundle.open(entry, "w") as destination:
                shutil.copyfileobj(contents, destination)
    with archive.open("rb") as source, alias.open("xb") as destination:
        shutil.copyfileobj(source, destination)
    return archive, alias


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("metadata")
    commands.add_parser("notes")
    for command in ("check-remote", "create-tag"):
        commands.add_parser(command).add_argument("--commit", required=True)
    packaging = commands.add_parser("package")
    packaging.add_argument("--target", choices=TARGETS, required=True)
    packaging.add_argument("--output", type=Path, default=Path("artifacts"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    version = read_version(root)
    if args.command == "metadata":
        print(f"version={version}\ntag=v{version}")
    elif args.command == "notes":
        print(release_notes(root))
    elif args.command == "check-remote":
        check_remote(version, args.commit)
    elif args.command == "create-tag":
        create_tag(version, args.commit)
    else:
        for archive in package(root, args.target, args.output):
            print(archive)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, RuntimeError, OSError) as error:
        sys.exit(str(error))
