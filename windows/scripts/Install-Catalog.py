"""Встановити перевірений пакет адресних даних для збірки з коду."""
import argparse
import gzip
import hashlib
import json
import os
import tempfile
import zipfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    expected = json.loads((root / "assets/data/catalog-package.json").read_text(encoding="utf-8"))
    with args.package.open("rb") as source:
        actual = hashlib.file_digest(source, "sha256").hexdigest()
    if actual != expected["sha256"] or args.package.stat().st_size != expected["bytes"]:
        raise SystemExit("Пакет каталогу не пройшов перевірку SHA-256/розміру.")
    index_bytes = (root / "assets/data/address-catalog-manifest.json").read_bytes()
    index = json.loads(index_bytes)
    names = {"index.json", "DATA_LICENSE.md"}
    for entry in index:
        names.update({entry["oblastId"] + ".sqlite3.gz", entry["oblastId"] + ".sqlite3.json"})
    destination = (root / "assets/data/address-catalog").resolve()
    if not destination.is_relative_to(root.resolve()):
        raise SystemExit("Некоректний каталог призначення.")
    with zipfile.ZipFile(args.package) as package, tempfile.TemporaryDirectory(dir=destination.parent) as temporary:
        if set(package.namelist()) != names or package.read("index.json") != index_bytes:
            raise SystemExit("Склад або версія пакета не відповідають коду.")
        staging = Path(temporary)
        for entry in index:
            identifier = entry["oblastId"]
            manifest_name = identifier + ".sqlite3.json"
            if json.loads(package.read(manifest_name)) != entry:
                raise SystemExit("Не збігається обласний маніфест.")
            name = identifier + ".sqlite3.gz"
            if package.getinfo(name).file_size != entry["compressedBytes"]:
                raise SystemExit("Не збігається розмір обласного пакета.")
            archive = staging / name
            archive.write_bytes(package.read(name))
            size = 0
            digest = hashlib.sha256()
            with gzip.open(archive, "rb") as source:
                while chunk := source.read(65536):
                    size += len(chunk)
                    if size > entry["bytes"]:
                        raise SystemExit("Завеликий вміст обласного пакета.")
                    digest.update(chunk)
            if size != entry["bytes"] or digest.hexdigest() != entry["sha256"]:
                raise SystemExit("Обласна база не пройшла перевірку.")
            (staging / manifest_name).write_bytes(package.read(manifest_name))
        (staging / "index.json").write_bytes(index_bytes)
        destination.mkdir(exist_ok=True)
        for path in staging.iterdir():
            os.replace(path, destination / path.name)
    print("27 обласних пакетів перевірено та підготовлено до збірки.")


if __name__ == "__main__":
    main()
