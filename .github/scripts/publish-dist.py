"""Upload the complete artifact set, then publish release-plz's draft release."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


MARKER = "<!-- cargo-dist -->"


def release_notes(original, manifest):
    # release-plz owns the changelog and contributor list. Omit dist's duplicate
    # changelog, keeping its generated download table.
    body = manifest["announcement_github_body"]
    changelog = manifest.get("announcement_changelog")
    if changelog:
        body = body.removeprefix(f"## Release Notes\n\n{changelog}\n\n")
    original = original.split(MARKER, 1)[0].rstrip()
    return f"{original}\n\n{MARKER}\n{body.rstrip()}\n"


def publish(directory):
    directory = Path(directory)
    tag = os.environ["RELEASE_TAG"]
    repository = os.environ["GITHUB_REPOSITORY"]
    manifest = json.loads((directory / "dist-manifest.json").read_text())
    if manifest["announcement_tag"] != tag:
        raise ValueError("dist manifest does not match the release tag")

    expected = set(manifest["artifacts"]) | {"dist-manifest.json"}
    for name in expected:
        if Path(name).name != name or not (directory / name).is_file():
            raise ValueError(f"Missing or invalid release artifact: {name}")

    def gh(*args):
        return subprocess.check_output(
            ["gh", "release", *args, "--repo", repository], text=True
        )

    release = json.loads(gh("view", tag, "--json", "body,isDraft,isPrerelease"))
    if not release["isDraft"]:
        raise ValueError("Refusing to replace binaries in an already published release")
    notes = release_notes(release["body"], manifest)

    # --clobber lets a failed upload be retried while the release remains draft.
    gh("upload", tag, *[str(directory / name) for name in sorted(expected)], "--clobber")
    with tempfile.TemporaryDirectory() as temporary:
        notes_path = Path(temporary) / "notes.md"
        notes_path.write_text(notes)
        gh(
            "edit", tag, "--notes-file", str(notes_path), "--draft=false",
            f"--latest={str(not release['isPrerelease']).lower()}",
        )


if __name__ == "__main__":
    publish(sys.argv[1])
