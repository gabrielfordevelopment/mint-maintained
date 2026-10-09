import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch
from urllib.error import HTTPError
from zipfile import ZipFile


spec = importlib.util.spec_from_file_location(
    "release", Path(__file__).resolve().parents[1] / "release.py"
)
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
COMMIT = "a" * 40


class ReleaseFiles(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(prefix="mint-release-test-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.write("Cargo.toml", '[workspace]\nmembers = ["mint_lib"]\n'
                   '[workspace.package]\nversion = "0.2.11"\n'
                   '[package]\nname = "mint"\nversion.workspace = true\n')
        self.write("mint_lib/Cargo.toml", '[package]\nname = "mint_lib"\nversion.workspace = true\n')
        self.write("Cargo.lock", 'version = 4\n[[package]]\nname = "mint"\nversion = "0.2.11"\n'
                   '[[package]]\nname = "mint_lib"\nversion = "0.2.11"\n')
        for notice in release.NOTICES:
            self.write(notice, "Test license notice\n")
        for target, binary in release.TARGETS.items():
            self.write(f"target/{target}/dist/{binary}", "Disposable binary fixture")

    def write(self, name, contents):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")

    def test_packages_have_versioned_names_licenses_and_identical_updater_aliases(self):
        for target, binary in release.TARGETS.items():
            with self.subTest(target=target):
                archive, alias = release.package(self.root, target, self.root / "output")
                self.assertEqual(archive.name, f"mint-v0.2.11-{target}.zip")
                self.assertEqual(alias.name, f"mint-{target}.zip")
                self.assertEqual(archive.read_bytes(), alias.read_bytes())
                with ZipFile(archive) as bundle:
                    self.assertEqual(set(bundle.namelist()), {binary, *release.NOTICES})
                    self.assertEqual(bundle.read(binary), b"Disposable binary fixture")
                    self.assertEqual(bundle.getinfo(binary).external_attr >> 16 & 0o777, 0o755)
                    for notice in release.NOTICES:
                        self.assertEqual(bundle.read(notice), (self.root / notice).read_bytes())
                with self.assertRaisesRegex(ValueError, "already exist"):
                    release.package(self.root, target, self.root / "output")
                self.assertEqual(archive.read_bytes(), alias.read_bytes())

    def test_missing_notices_prevent_packaging(self):
        (self.root / release.NOTICES[0]).unlink()
        with self.assertRaisesRegex(ValueError, "Required release file"):
            release.package(self.root, "x86_64-pc-windows-msvc", self.root / "output")
        self.assertFalse((self.root / "output").exists())

    def test_stale_lockfile_and_independent_package_version_are_rejected(self):
        path = self.root / "Cargo.lock"
        path.write_text(path.read_text().replace('"0.2.11"', '"0.2.10"'), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Refresh Cargo.lock"):
            release.read_version(self.root)
        self.write("mint_lib/Cargo.toml", '[package]\nname = "mint_lib"\nversion = "0.2.11"\n')
        path.write_text(path.read_text().replace('"0.2.10"', '"0.2.11"'), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "inherit"):
            release.read_version(self.root)

    def test_only_stable_semver_is_accepted(self):
        for version in ("1.2", "v1.2.3", "01.2.3", "1.2.3-rc.1", "1.2.3+dev", "1.2.3\n"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                release.stable_version(version)
        self.assertGreater(release.stable_version("0.2.11"), release.stable_version("0.2.9"))


class RemoteReleaseChecks(unittest.TestCase):
    def api(self, responses):
        return Mock(side_effect=lambda endpoint, **kwargs: responses.get(
            endpoint, [] if endpoint.startswith("releases?") else None
        ))

    def test_first_release_creates_tag_at_exact_tested_commit(self):
        api = self.api({})
        release.create_tag("0.2.11", COMMIT, api)
        api.assert_called_with("git/refs", data={"ref": "refs/tags/v0.2.11", "sha": COMMIT})

    def test_first_release_must_be_newer_than_inherited_clients(self):
        api = self.api({})
        with self.assertRaisesRegex(ValueError, "inherited"):
            release.create_tag("0.2.10", COMMIT, api)
        self.assertFalse(any("data" in call.kwargs for call in api.call_args_list))

    def test_matching_tag_allows_retry_without_recreating_it(self):
        api = self.api({"git/ref/tags/v0.2.11": {"object": {"type": "commit", "sha": COMMIT}}})
        release.create_tag("0.2.11", COMMIT, api)
        self.assertFalse(any("data" in call.kwargs for call in api.call_args_list))

    def test_existing_release_or_draft_is_never_overwritten(self):
        for draft in (True, False):
            api = self.api({"releases?per_page=100&page=1": [{"tag_name": "v0.2.11", "draft": draft}]})
            with self.subTest(draft=draft), self.assertRaisesRegex(ValueError, "already exists"):
                release.create_tag("0.2.11", COMMIT, api)
            self.assertFalse(any("data" in call.kwargs for call in api.call_args_list))

    def test_duplicate_draft_on_later_page_is_rejected(self):
        api = self.api({
            "releases?per_page=100&page=1": [{"tag_name": f"v0.1.{i}"} for i in range(100)],
            "releases?per_page=100&page=2": [{"tag_name": "v0.2.11", "draft": True}],
        })
        with self.assertRaisesRegex(ValueError, "already exists"):
            release.create_tag("0.2.11", COMMIT, api)
        self.assertFalse(any("data" in call.kwargs for call in api.call_args_list))

    def test_conflicting_tag_is_never_moved(self):
        api = self.api({"git/ref/tags/v0.2.11": {"object": {"type": "commit", "sha": "b" * 40}}})
        with self.assertRaisesRegex(ValueError, "will not be moved"):
            release.create_tag("0.2.11", COMMIT, api)
        self.assertFalse(any("data" in call.kwargs for call in api.call_args_list))

    def test_annotated_tag_is_resolved_to_commit(self):
        api = self.api({
            "git/ref/tags/v0.2.11": {"object": {"type": "tag", "sha": "c" * 40}},
            f"git/tags/{'c' * 40}": {"object": {"type": "commit", "sha": COMMIT}},
        })
        self.assertTrue(release.check_remote("0.2.11", COMMIT, api))

    def test_downgrades_and_duplicate_versions_are_rejected(self):
        for latest in ("v0.2.11", "v0.3.0"):
            api = self.api({"releases/latest": {"tag_name": latest}})
            with self.subTest(latest=latest), self.assertRaisesRegex(ValueError, "newer"):
                release.create_tag("0.2.11", COMMIT, api)
        api = self.api({"releases/latest": {"tag_name": "v0.2.9"}})
        self.assertFalse(release.check_remote("0.2.11", COMMIT, api))

    def test_network_and_permission_failures_do_not_mean_missing_release(self):
        for status in (401, 403, 429, 500):
            error = HTTPError("https://api.github.com", status, "Test failure", {}, None)
            with self.subTest(status=status), patch.dict(
                release.os.environ, {"GH_REPO": "example/project", "GH_TOKEN": "test-placeholder"}
            ), patch.object(release, "urlopen", side_effect=error):
                with self.assertRaisesRegex(RuntimeError, str(status)):
                    release.github_api("releases/latest", missing_ok=True)


if __name__ == "__main__":
    unittest.main()
