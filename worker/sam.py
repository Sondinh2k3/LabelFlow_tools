"""Interactive click-to-mask with SAM-family ONNX models (SAM, MobileSAM, SAM-HQ exports).

Encoder: one image input, either HWC [1024,1024,3] float RGB 0-255 (samexporter, the graph
normalizes) or NCHW [1,3,1024,1024] (normalized here with the manifest mean/std, 0-255 scale).
Decoder: the official segment-anything ONNX signature (image_embeddings, point_coords,
point_labels, mask_input, has_mask_input, orig_im_size). The expensive encoder runs once per
image and is cached; each click only runs the small decoder.
"""
import collections
import hashlib
import json
import math
from pathlib import Path

MAX_PIXELS = 24_000_000
MAX_RUN_VALUES = 1_000_000
DECODER_INPUTS = {"image_embeddings", "point_coords", "point_labels", "mask_input",
                  "has_mask_input", "orig_im_size"}


def _model_file(base, spec, what):
    if set(spec) - {"file", "sha256"} or not isinstance(spec.get("file"), str):
        raise ValueError(f"Invalid {what} declaration")
    name = spec["file"]
    if Path(name).name != name or not name.endswith(".onnx"):
        raise ValueError(f"{what} ONNX must be a file beside its manifest")
    path = (base / name).resolve(strict=True)
    if path.parent != base or not 0 < path.stat().st_size <= 2 * 1024**3:
        raise ValueError(f"Invalid {what} location or size")
    sha = spec.get("sha256")
    if not isinstance(sha, str) or len(sha) != 64 or any(c not in "0123456789abcdef" for c in sha):
        raise ValueError(f"Invalid {what} SHA-256")
    return path, sha


def descriptor(manifest):
    path = Path(manifest).resolve(strict=True)
    if path.stat().st_size > 64 * 1024:
        raise ValueError("SAM manifest exceeds 64 KiB")
    value = json.loads(path.read_text(encoding="utf-8-sig"))
    required = {"schemaVersion", "kind", "id", "name", "version", "license", "encoder", "decoder"}
    if not required <= set(value) <= required | {"input"} or value["schemaVersion"] != 1 \
            or value["kind"] != "sam_prompt":
        raise ValueError("Unsupported SAM manifest schema (kind must be sam_prompt)")
    for key in ("id", "name", "version", "license"):
        if not isinstance(value[key], str) or not 1 <= len(value[key]) <= 256:
            raise ValueError("Invalid model identity")
    inp = value.get("input", {})
    if set(inp) - {"size", "mean", "std"}:
        raise ValueError("SAM input accepts only size, mean and std")
    size = inp.get("size", 1024)
    mean = inp.get("mean", [123.675, 116.28, 103.53])
    std = inp.get("std", [58.395, 57.12, 57.375])
    if type(size) is not int or not 256 <= size <= 2048:
        raise ValueError("Invalid SAM input size")
    for v in (mean, std):
        if (not isinstance(v, list) or len(v) != 3
                or any(type(x) not in (int, float) or not math.isfinite(x) for x in v)):
            raise ValueError("Invalid SAM normalization")
    if any(x <= 0 for x in std):
        raise ValueError("SAM std must be positive")
    base = path.parent
    encoder = _model_file(base, value["encoder"], "encoder")
    decoder = _model_file(base, value["decoder"], "decoder")
    config = {k: value[k] for k in ("id", "name", "version", "license")}
    config.update({"kind": "sam_prompt", "inputSize": size})
    return config, encoder, decoder, (size, mean, std)


def _session(path, sha):
    import onnxruntime as ort
    with path.open("rb") as stream:
        data = stream.read(2 * 1024**3 + 1)
    if hashlib.sha256(data).hexdigest() != sha:
        raise ValueError(f"{path.name}: checksum mismatch")
    options = ort.SessionOptions()
    options.intra_op_num_threads = 4
    options.inter_op_num_threads = 1
    session = ort.InferenceSession(data, sess_options=options, providers=["CPUExecutionProvider"])
    session.disable_fallback()
    return session


def pack(flat):
    import numpy as np
    changes = np.flatnonzero(flat[1:] != flat[:-1]) + 1
    if (len(changes) + 1) * 2 > MAX_RUN_VALUES:
        raise ValueError("Mask exceeds RLE budget")
    starts = np.concatenate(([0], changes))
    counts = np.diff(np.concatenate((starts, [flat.size])))
    result = np.empty(starts.size * 2, dtype=np.int64)
    result[::2], result[1::2] = flat[starts], counts
    return result.tolist()


class SamAdapter:
    def __init__(self, manifest):
        import numpy as np
        self.config, encoder, decoder, (self.size, mean, std) = descriptor(manifest)
        self.mean = np.array(mean, dtype=np.float32)
        self.std = np.array(std, dtype=np.float32)
        self.encoder = _session(*encoder)
        self.decoder = _session(*decoder)
        inputs = self.encoder.get_inputs()
        if len(inputs) != 1 or inputs[0].type != "tensor(float)" or len(inputs[0].shape) not in (3, 4):
            raise ValueError("SAM encoder needs one float image input (HWC or NCHW)")
        self.encoder_input = inputs[0].name
        self.hwc = len(inputs[0].shape) == 3
        # A static encoder input size wins over the manifest (exports are fixed-size).
        side = inputs[0].shape[:2] if self.hwc else inputs[0].shape[2:]
        if all(isinstance(d, int) for d in side):
            if side[0] != side[1] or not 64 <= side[0] <= 2048:
                raise ValueError("SAM encoder input must be square")
            self.size = side[0]
            self.config["inputSize"] = self.size
        names = {i.name for i in self.decoder.get_inputs()}
        if names != DECODER_INPUTS:
            raise ValueError("SAM decoder must use the official segment-anything ONNX inputs")
        self.cache = collections.OrderedDict()
        self.last_low_res = {}
        # Dry run proves both graphs execute and agree on embedding shape.
        embedding = self._encode(np.zeros((64, 64, 3), dtype=np.uint8))
        self._decode(embedding, [(32.0, 32.0, 1)], None, 64, 64, None)

    def _encode(self, rgb):
        import numpy as np
        from PIL import Image
        h, w = rgb.shape[:2]
        scale = self.size / max(h, w)
        nh, nw = max(1, round(h * scale)), max(1, round(w * scale))
        resized = np.asarray(Image.fromarray(rgb).resize((nw, nh), Image.Resampling.BILINEAR),
                             dtype=np.float32)
        if self.hwc:
            tensor = np.zeros((self.size, self.size, 3), dtype=np.float32)
            tensor[:nh, :nw] = resized
        else:
            tensor = np.zeros((1, 3, self.size, self.size), dtype=np.float32)
            tensor[0, :, :nh, :nw] = ((resized - self.mean) / self.std).transpose(2, 0, 1)
        embedding = self.encoder.run(None, {self.encoder_input: tensor})[0]
        if embedding.ndim != 4 or embedding.shape[0] != 1 or not np.isfinite(embedding).all():
            raise ValueError("Unexpected SAM embedding")
        return embedding

    def _decode(self, embedding, points, box, width, height, low_res):
        import numpy as np
        scale = self.size / max(width, height)
        coords = [(x * scale, y * scale) for x, y, _ in points]
        labels = [float(label) for _, _, label in points]
        if box:
            x, y, bw, bh = box
            coords += [(x * scale, y * scale), ((x + bw) * scale, (y + bh) * scale)]
            labels += [2.0, 3.0]
        else:
            coords.append((0.0, 0.0))
            labels.append(-1.0)
        feed = {
            "image_embeddings": embedding,
            "point_coords": np.array([coords], dtype=np.float32),
            "point_labels": np.array([labels], dtype=np.float32),
            "mask_input": low_res if low_res is not None
            else np.zeros((1, 1, 256, 256), dtype=np.float32),
            "has_mask_input": np.array([1.0 if low_res is not None else 0.0], dtype=np.float32),
            "orig_im_size": np.array([height, width], dtype=np.float32),
        }
        masks, scores, low = self.decoder.run(None, feed)[:3]
        if masks.ndim != 4 or masks.shape[2:] != (height, width):
            raise ValueError("SAM decoder returned an unexpected mask shape")
        best = int(np.argmax(scores[0])) if scores.size > 1 else 0
        return masks[0, best] > 0, float(scores.reshape(-1)[best]), low[:, best:best + 1]

    def prepare(self, request):
        import numpy as np
        from PIL import Image
        key = request["sourceSha256"]
        width, height = request["width"], request["height"]
        if (type(width) is not int or type(height) is not int or width <= 0 or height <= 0
                or width * height > MAX_PIXELS):
            raise ValueError("Expected a bounded canonical image")
        if key in self.cache:
            self.cache.move_to_end(key)
            return self.cache[key], True
        with Image.open(request["imagePath"]) as image:
            if image.size != (width, height):
                raise ValueError("Canonical image dimensions disagree")
            rgb = np.asarray(image.convert("RGB"))
        embedding = self._encode(rgb)
        self.cache[key] = embedding
        while len(self.cache) > 4:
            old, _ = self.cache.popitem(last=False)
            self.last_low_res.pop(old, None)
        return embedding, False

    def predict(self, request):
        embedding, cached = self.prepare(request)
        if request.get("encodeOnly"):
            return {"cached": cached}
        width, height = request["width"], request["height"]
        points = request.get("points", [])
        box = request.get("box")
        if not isinstance(points, list) or len(points) > 64:
            raise ValueError("At most 64 SAM points")
        parsed = []
        for p in points:
            x, y, positive = p["x"], p["y"], p["positive"]
            if (type(x) not in (int, float) or type(y) not in (int, float) or type(positive) is not bool
                    or not (0 <= x <= width and 0 <= y <= height)):
                raise ValueError("Invalid SAM point")
            parsed.append((float(x), float(y), 1 if positive else 0))
        if box is not None:
            box = (float(box["x"]), float(box["y"]), float(box["width"]), float(box["height"]))
            if box[2] < 1 or box[3] < 1 or not all(math.isfinite(v) for v in box):
                raise ValueError("Invalid SAM box")
        if not parsed and box is None:
            raise ValueError("Click a point or drag a box")
        key = request["sourceSha256"]
        # Refining with the previous low-res mask makes extra clicks converge like the SAM demo.
        low_res = self.last_low_res.get(key) if len(parsed) > 1 and box is None else None
        mask, score, low = self._decode(embedding, parsed, box, width, height, low_res)
        self.last_low_res[key] = low
        import numpy as np
        flat = mask.reshape(-1).astype(np.uint8)
        area = int(flat.sum())
        return {"width": width, "height": height, "runs": pack(flat), "score": score,
                "area": area, "cached": cached}
