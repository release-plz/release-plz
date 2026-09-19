"""Stage dist artifacts and preserve the flat .tar.gz binary downloads."""

import io
import json
from pathlib import Path
import shutil
import sys
import tarfile
import zipfile


def stage(manifest_path, destination):
    manifest = json.loads(Path(manifest_path).read_text())
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    for source in manifest["upload_files"]:
        source = Path(source)
        shutil.copyfile(source, destination / source.name)
        if source.name.endswith(".tar.xz"):
            stem = source.name.removesuffix(".tar.xz")
            binary = "release-plz"
            with tarfile.open(source) as archive:
                data = archive.extractfile(f"{stem}/{binary}").read()
        elif source.name.endswith(".zip"):
            stem = source.stem
            binary = "release-plz.exe"
            with zipfile.ZipFile(source) as archive:
                data = archive.read(binary)
        else:
            continue

        # Existing consumers extract release-plz directly, without stripping a
        # directory. Keep the original dist archive and its checksum unchanged.
        with tarfile.open(destination / f"{stem}.tar.gz", "w:gz") as archive:
            entry = tarfile.TarInfo(binary)
            entry.size = len(data)
            entry.mode = 0o755
            archive.addfile(entry, io.BytesIO(data))


if __name__ == "__main__":
    stage(*sys.argv[1:])
