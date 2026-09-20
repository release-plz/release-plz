"""Exercise compatibility and publication using cargo-dist's mock build output."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile


sys.dont_write_bytecode = True


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(f"{name}.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


stage = load_script("stage-dist")
publish = load_script("publish-dist")
MANIFEST = Path(sys.argv.pop(1)).resolve()
TARGETS = [
    "aarch64-apple-darwin",
    "aarch64-pc-windows-msvc",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "x86_64-unknown-freebsd",
]


class DistributionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        stage.stage(MANIFEST, self.directory)
        self.manifest = json.loads(MANIFEST.read_text())
        (self.directory / "dist-manifest.json").write_text(json.dumps(self.manifest))
        self.freebsd = self.directory / "release-plz-x86_64-unknown-freebsd.tar.gz"
        with tarfile.open(self.freebsd, "w:gz") as archive:
            binary = tarfile.TarInfo("release-plz")
            binary.mode = 0o755
            archive.addfile(binary)
        self.release = {
            "body": "Changes with `code`, $variables and 'quotes'.\n\n### Contributors\n* @alice",
            "isDraft": True,
            "isPrerelease": False,
        }
        self.commands = []
        self.notes = None
        self.fail_upload = False
        environment = patch.dict(os.environ, {
            "RELEASE_TAG": self.manifest["announcement_tag"],
            "GITHUB_REPOSITORY": "release-plz/release-plz",
        })
        environment.start()
        self.addCleanup(environment.stop)

    def gh(self, command, **kwargs):
        self.commands.append(command)
        if command[2] == "view":
            return json.dumps(self.release)
        if command[2] == "upload" and self.fail_upload:
            raise subprocess.CalledProcessError(1, command)
        if command[2] == "edit":
            self.notes = Path(command[command.index("--notes-file") + 1]).read_text()
        return ""

    def test_existing_download_names_and_layout(self):
        for target in TARGETS:
            binary = "release-plz.exe" if "windows" in target else "release-plz"
            with tarfile.open(self.directory / f"release-plz-{target}.tar.gz") as archive:
                self.assertEqual(archive.getnames(), [binary])
                self.assertEqual(archive.getmember(binary).mode, 0o755)
                compatible_binary = archive.extractfile(binary).read()
            if "windows" in target:
                with zipfile.ZipFile(self.directory / f"release-plz-{target}.zip") as archive:
                    self.assertEqual(archive.read(binary), compatible_binary)
            elif "freebsd" not in target:
                with tarfile.open(self.directory / f"release-plz-{target}.tar.xz") as archive:
                    self.assertEqual(
                        archive.extractfile(f"release-plz-{target}/{binary}").read(),
                        compatible_binary,
                    )

    def test_publish_only_after_upload_and_preserve_notes(self):
        with patch.object(publish.subprocess, "check_output", side_effect=self.gh):
            publish.publish(self.directory)
        self.assertEqual([cmd[2] for cmd in self.commands], ["view", "upload", "edit"])
        self.assertIn("--clobber", self.commands[1])
        self.assertIn("--draft=false", self.commands[2])
        self.assertIn("--latest=true", self.commands[2])
        self.assertTrue(self.notes.startswith(self.release["body"]))
        self.assertIn("## Download release-plz", self.notes)
        self.assertIn(self.freebsd.name, self.notes)
        self.assertNotIn("## Release Notes", self.notes)
        # The manifest and original dist archives still agree on their digests.
        for name, artifact in self.manifest["artifacts"].items():
            for algorithm, digest in artifact.get("checksums", {}).items():
                if algorithm == "sha256":
                    self.assertEqual(hashlib.sha256((self.directory / name).read_bytes()).hexdigest(), digest)
        for line in (self.directory / "sha256.sum").read_text().splitlines():
            digest, name = line.split("  ", 1)
            self.assertEqual(hashlib.sha256((self.directory / name).read_bytes()).hexdigest(), digest)

    def test_failed_upload_never_publishes(self):
        self.fail_upload = True
        with patch.object(publish.subprocess, "check_output", side_effect=self.gh):
            with self.assertRaises(subprocess.CalledProcessError):
                publish.publish(self.directory)
        self.assertEqual([cmd[2] for cmd in self.commands], ["view", "upload"])

    def test_missing_target_never_uploads(self):
        for name in [self.freebsd.name, "release-plz-x86_64-unknown-linux-gnu.tar.xz"]:
            with self.subTest(artifact=name):
                artifact = self.directory / name
                content = artifact.read_bytes()
                artifact.unlink()
                with patch.object(publish.subprocess, "check_output") as gh:
                    with self.assertRaisesRegex(ValueError, "Missing"):
                        publish.publish(self.directory)
                    gh.assert_not_called()
                artifact.write_bytes(content)

    def test_wrong_tag_never_uploads(self):
        with patch.dict(os.environ, {"RELEASE_TAG": "release-plz-v9.9.9"}):
            with patch.object(publish.subprocess, "check_output") as gh:
                with self.assertRaisesRegex(ValueError, "tag"):
                    publish.publish(self.directory)
                gh.assert_not_called()

    def test_published_release_is_not_modified(self):
        self.release["isDraft"] = False
        with patch.object(publish.subprocess, "check_output", side_effect=self.gh):
            with self.assertRaisesRegex(ValueError, "already published"):
                publish.publish(self.directory)
        self.assertEqual([cmd[2] for cmd in self.commands], ["view"])

    def test_prerelease_does_not_become_latest(self):
        self.release["isPrerelease"] = True
        with patch.object(publish.subprocess, "check_output", side_effect=self.gh):
            publish.publish(self.directory)
        self.assertIn("--latest=false", self.commands[-1])

    def test_notes_are_not_duplicated_on_retry(self):
        args = (self.manifest, {self.freebsd.name}, "release-plz/release-plz", "release-plz-v1.2.3")
        notes = publish.release_notes(self.release["body"], *args)
        self.assertEqual(publish.release_notes(notes, *args), notes)


if __name__ == "__main__":
    unittest.main()
