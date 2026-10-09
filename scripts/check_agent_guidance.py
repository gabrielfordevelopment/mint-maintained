"""Validate repository guidance using only Python's standard library."""

from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit


def validate(root: Path) -> list[str]:
    root = root.resolve()
    result = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root, check=True, capture_output=True,
    )
    files = {Path(name) for name in result.stdout.decode("utf-8").split("\0") if name}
    errors = []
    for required in [Path("AGENTS.md"), Path("README.md")]:
        if required not in files or not (root / required).is_file():
            errors.append(f"Missing required document: {required}")
    for path in files:
        if path.name in {"AGENTS.md", "AGENTS.override.md"} and path != Path("AGENTS.md"):
            errors.append(f"Parallel instruction entry point: {path}")
    skill_directories = {
        Path(*path.parts[:3]) for path in files
        if path.parts[:2] == (".agents", "skills") and len(path.parts) >= 4
    }
    for directory in sorted(skill_directories):
        if directory / "SKILL.md" not in files:
            errors.append(f"Missing skill entry point: {directory}")

    documents = sorted(
        path for path in files
        if path.suffix == ".md" and (
            path in {Path("AGENTS.md"), Path("README.md")}
            or path.parts[0] == "docs"
            or path.parts[:2] == (".agents", "skills")
        )
    )
    root_routes = set()
    skills = []
    for relative in documents:
        path = root / relative
        try:
            raw = path.read_bytes()
            text = raw.decode("utf-8")
        except (OSError, UnicodeError) as error:
            errors.append(f"{relative}: cannot read UTF-8 text ({error})")
            continue
        if raw.startswith(b"\xef\xbb\xbf") or not text.endswith("\n"):
            errors.append(f"{relative}: use UTF-8 without BOM and a final newline")
        if any(line != line.rstrip() for line in text.splitlines()):
            errors.append(f"{relative}: trailing whitespace")

        if relative.parts[:2] == (".agents", "skills") and relative.name == "SKILL.md":
            skills.append(relative)
            lines = text.splitlines()
            valid = len(lines) >= 4 and lines[0] == lines[3] == "---"
            name = re.fullmatch(r"name: ([a-z0-9]+(?:-[a-z0-9]+)*)", lines[1]) if valid else None
            description = lines[2].removeprefix("description: ") if valid else ""
            if (
                not name or len(name[1]) > 64 or name[1] != relative.parent.name
                or len(relative.parts) != 4
                or not lines[2].startswith("description: ")
                or not description or len(description) > 1024
                or not description[0].isalpha()
                or description.casefold() in {"null", "true", "false", "yes", "no", "on", "off"}
                or re.search(r":\s|\s#|[<>]", description)
            ):
                errors.append(f"{relative}: expected matching name and plain single-line description frontmatter")

        fence = None
        for number, line in enumerate(text.splitlines(), 1):
            marker = re.match(r"^\s*(`{3,}|~{3,})(.*)$", line)
            if marker:
                token, suffix = marker.groups()
                if fence is None:
                    fence = token
                elif token[0] == fence[0] and len(token) >= len(fence) and not suffix.strip():
                    fence = None
                continue
            if fence:
                continue
            for destination in re.findall(r"!?\[[^\]]*\]\(([^)]+)\)", line):
                destination = destination.strip().removeprefix("<").removesuffix(">")
                try:
                    parsed = urlsplit(destination)
                except ValueError:
                    errors.append(f"{relative}:{number}: invalid link {destination}")
                    continue
                if parsed.scheme or parsed.netloc or not parsed.path:
                    continue
                target = (path.parent / unquote(parsed.path)).resolve()
                if not target.is_relative_to(root) or not target.exists():
                    errors.append(f"{relative}:{number}: missing or out-of-repository link {destination}")
                elif relative == Path("AGENTS.md"):
                    root_routes.add(target)
        if fence:
            errors.append(f"{relative}: unclosed code fence")

    if not skills:
        errors.append("No repository skills found")
    for skill in skills:
        if (root / skill).resolve() not in root_routes:
            errors.append(f"Unrouted skill: {skill}")
    return errors


if __name__ == "__main__":
    findings = validate(Path(__file__).resolve().parents[1])
    if findings:
        print("\n".join(findings))
        sys.exit(1)
    print("Agent guidance checks passed.")
