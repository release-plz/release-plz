"""Exercise draft publication using cargo-dist's mock build output."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


sys.dont_write_bytecode = True


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(f"{name}.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


publish = load_script("publish-dist")
MANIFEST = Path(sys.argv.pop(1)).resolve()


class DistributionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.manifest = json.loads(MANIFEST.read_text())
        for source in self.manifest["upload_files"]:
            shutil.copyfile(source, self.directory / Path(source).name)
        (self.directory / "dist-manifest.json").write_text(json.dumps(self.manifest))
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

    def test_publish_only_after_upload_and_preserve_notes(self):
        artifacts = {path.name: path.read_bytes() for path in self.directory.iterdir()}
        with patch.object(publish.subprocess, "check_output", side_effect=self.gh):
            publish.publish(self.directory)
        self.assertEqual([cmd[2] for cmd in self.commands], ["view", "upload", "edit"])
        self.assertIn("--clobber", self.commands[1])
        self.assertEqual({Path(arg).name for arg in self.commands[1][4:-3]}, set(artifacts))
        self.assertIn("--draft=false", self.commands[2])
        self.assertIn("--latest=true", self.commands[2])
        self.assertTrue(self.notes.startswith(self.release["body"]))
        self.assertIn("## Download release-plz", self.notes)
        self.assertNotIn("## Release Notes", self.notes)
        self.assertEqual(
            {path.name: path.read_bytes() for path in self.directory.iterdir()}, artifacts
        )

    def test_failed_upload_never_publishes(self):
        self.fail_upload = True
        with patch.object(publish.subprocess, "check_output", side_effect=self.gh):
            with self.assertRaises(subprocess.CalledProcessError):
                publish.publish(self.directory)
        self.assertEqual([cmd[2] for cmd in self.commands], ["view", "upload"])

    def test_missing_target_never_uploads(self):
        (self.directory / "release-plz-x86_64-unknown-linux-gnu.tar.xz").unlink()
        with patch.object(publish.subprocess, "check_output") as gh:
            with self.assertRaisesRegex(ValueError, "Missing"):
                publish.publish(self.directory)
            gh.assert_not_called()

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
        notes = publish.release_notes(self.release["body"], self.manifest)
        self.assertEqual(publish.release_notes(notes, self.manifest), notes)


if __name__ == "__main__":
    unittest.main()
