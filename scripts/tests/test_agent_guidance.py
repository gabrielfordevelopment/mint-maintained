from pathlib import Path
import importlib.util
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "check_agent_guidance", Path(__file__).resolve().parents[1] / "check_agent_guidance.py"
)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class GuidanceChecks(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="mint-guidance-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        self.skill = ".agents/skills/mint-example/SKILL.md"
        self.write("AGENTS.md", f"# Guidance\n\n[Example]({self.skill})\n")
        self.write("README.md", "# Project\n")
        self.write(self.skill, "---\nname: mint-example\ndescription: Exercise the example workflow.\n---\n\n# Example\n")

    def write(self, path, content):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")

    def test_valid_untracked_documents_and_external_links(self):
        self.write("docs/example.md", "# Example\n\n[Root](../AGENTS.md)\n[Web](https://example.org)\n\n```text\n[Placeholder](not-a-file)\n```\n")
        self.assertEqual(checker.validate(self.root), [])

    def test_broken_local_links_and_escaping_paths_fail(self):
        self.write("docs/example.md", "[Missing](missing.md)\n[Outside](../../outside.md)\n")
        findings = checker.validate(self.root)
        self.assertEqual(len(findings), 2)
        self.assertTrue(all("link" in item for item in findings))

    def test_skill_requires_root_routing(self):
        self.write("AGENTS.md", "# Guidance\n")
        self.assertTrue(any("Unrouted skill" in item for item in checker.validate(self.root)))

    def test_skill_directory_requires_an_entry_point(self):
        self.write(".agents/skills/incomplete/notes.md", "# Notes\n")
        self.assertTrue(any("Missing skill entry point" in item for item in checker.validate(self.root)))

    def test_invalid_metadata_fails(self):
        for content in [
            "# Missing metadata\n",
            "---\nname: wrong-name\ndescription: Example.\n---\n",
            "---\nname: mint-example\ndescription: Example: invalid plain YAML.\n---\n",
            "---\nname: mint-example\ndescription: null\n---\n",
        ]:
            with self.subTest(content=content):
                self.write(self.skill, content)
                self.assertTrue(any("frontmatter" in item for item in checker.validate(self.root)))

    def test_parallel_instructions_and_invalid_document_format_fail(self):
        self.write("src/AGENTS.md", "# Nested\n")
        self.write("docs/example.md", "\ufeff# Example  \n\n```text\nunfinished")
        findings = checker.validate(self.root)
        for reason in ["Parallel instruction", "UTF-8", "trailing whitespace", "unclosed code fence"]:
            self.assertTrue(any(reason in item for item in findings), reason)


if __name__ == "__main__":
    unittest.main()
