"""Write a LabelFlow SAM manifest beside an encoder/decoder ONNX pair.

Usage:
  python scripts/make-sam-manifest.py <folder> [--encoder X.onnx] [--decoder Y.onnx]
      [--name "MobileSAM"] [--license "Apache-2.0"]

The folder must contain the two ONNX files, e.g. MobileSAM from
https://huggingface.co/nrl-ai/samexporter-onnx-models (folder mobile_sam) or your own
samexporter export. Without --encoder/--decoder the script picks the files whose names
contain "encoder" and "decoder". The worker detects the encoder layout itself: HWC encoders
normalize inside the graph; NCHW encoders get the standard SAM mean/std.
"""
import argparse
import hashlib
import json
from pathlib import Path


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def pick(folder, explicit, word):
    if explicit:
        path = folder / explicit
        if not path.is_file():
            raise SystemExit(f"{path} not found")
        return path
    found = sorted(p for p in folder.glob("*.onnx") if word in p.name.lower())
    if len(found) != 1:
        raise SystemExit(f"Expected one *{word}*.onnx in {folder}, found {len(found)}; use --{word}")
    return found[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--encoder")
    parser.add_argument("--decoder")
    parser.add_argument("--name", default="SAM")
    parser.add_argument("--license", default="Apache-2.0 (check your model's terms)")
    parser.add_argument("--version", default="1")
    args = parser.parse_args()
    folder = args.folder.resolve()
    encoder, decoder = pick(folder, args.encoder, "encoder"), pick(folder, args.decoder, "decoder")
    manifest = {
        "schemaVersion": 1,
        "kind": "sam_prompt",
        "id": args.name.lower().replace(" ", "-"),
        "name": args.name,
        "version": args.version,
        "license": args.license,
        "encoder": {"file": encoder.name, "sha256": sha256(encoder)},
        "decoder": {"file": decoder.name, "sha256": sha256(decoder)},
    }
    target = folder / "sam.json"
    target.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {target}. In LabelFlow: right panel > Click-to-mask (SAM) > Choose SAM model > pick this file.")


if __name__ == "__main__":
    main()
