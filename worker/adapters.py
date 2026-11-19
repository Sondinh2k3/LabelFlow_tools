"""Explicit ONNX schemas. No model downloads, Python plugins, or guessed class IDs."""
import hashlib
import json
import math
from pathlib import Path

MAX_PIXELS = 24_000_000
MAX_RUN_VALUES = 1_000_000


def descriptor(path):
    path = Path(path).resolve(strict=True)
    if path.stat().st_size > 64 * 1024:
        raise ValueError("Model manifest exceeds 64 KiB")
    value = json.loads(path.read_text(encoding="utf-8-sig"))
    required = {"schemaVersion", "id", "name", "version", "license", "file", "sha256",
                "adapter", "layerId", "classMap", "input", "output"}
    if set(value) != required or value["schemaVersion"] != 1:
        raise ValueError("Unsupported model manifest schema")
    for key in ("id", "name", "version", "license", "layerId"):
        if not isinstance(value[key], str) or not 1 <= len(value[key]) <= 256:
            raise ValueError("Invalid model identity")
    filename = value["file"]
    if not isinstance(filename, str) or Path(filename).name != filename or not filename.endswith(".onnx"):
        raise ValueError("ONNX must be a file beside its manifest")
    model = (path.parent / filename).resolve(strict=True)
    if model.parent != path.parent or not 0 < model.stat().st_size <= 1024**3:
        raise ValueError("Invalid model location or size")
    if len(value["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in value["sha256"]):
        raise ValueError("Invalid model SHA-256")
    classes = value["classMap"]
    if (not isinstance(classes, list) or not 1 <= len(classes) <= 255
            or any(not isinstance(c, str) or not 1 <= len(c) <= 256 for c in classes)
            or len(set(classes)) != len(classes)):
        raise ValueError("Class map must contain unique stable class IDs")
    inp = value["input"]
    if set(inp) != {"name", "width", "height", "mean", "std"}:
        raise ValueError("Input schema must declare RGB float32 NCHW stretch preprocessing")
    if any(type(inp[k]) is not int or not 1 <= inp[k] <= 4096 for k in ("width", "height")):
        raise ValueError("Invalid input dimensions")
    if inp["width"] * inp["height"] > 4_194_304:
        raise ValueError("Model input exceeds four megapixels")
    for key in ("mean", "std"):
        if (not isinstance(inp[key], list) or len(inp[key]) != 3
                or any(type(x) not in (float, int) or not math.isfinite(x) for x in inp[key])
                or (key == "std" and any(x <= 0 for x in inp[key]))):
            raise ValueError("Invalid RGB normalization")
    out = value["output"]
    if set(out) != {"name", "format", "resize"} or not all(isinstance(out[k], str) and out[k] for k in out):
        raise ValueError("Invalid output schema")
    adapter = value["adapter"]
    if adapter not in ("onnx_semantic", "pidnet_semantic", "onnx_boxes"):
        raise ValueError("Unsupported adapter")
    if adapter == "onnx_boxes":
        if out["format"] != "boxes_xyxy_score_class" or out["resize"] != "scale_xyxy":
            raise ValueError("Boxes require post-NMS [N,6] input-pixel xyxy,score,class output")
    elif out["format"] not in ("logits_nchw", "labels_nhw"):
        raise ValueError("Semantic output requires logits_nchw or labels_nhw")
    elif out["resize"] != ("bilinear_half_pixel" if out["format"] == "logits_nchw" else "nearest"):
        raise ValueError("Declare bilinear_half_pixel logits resize before argmax, or nearest label resize")
    # Channel count and class order are checked against the project's semantic classes by the
    # desktop; here PIDNet only needs the semantic layer and per-class logits.
    if adapter == "pidnet_semantic" and (out["format"] != "logits_nchw"
            or value["layerId"] != "cityscapes_semantic"):
        raise ValueError("PIDNet requires per-class logits for the semantic layer")
    value["task"] = "boxes" if adapter == "onnx_boxes" else "semantic"
    return value, model


def pack(pixels):
    import numpy as np
    flat = np.asarray(pixels, dtype=np.uint8).reshape(-1)
    changes = np.flatnonzero(flat[1:] != flat[:-1]) + 1
    if (len(changes) + 1) * 2 > MAX_RUN_VALUES:
        raise ValueError("Prediction exceeds suggestion RLE budget")
    starts = np.concatenate(([0], changes))
    counts = np.diff(np.concatenate((starts, [flat.size])))
    result = np.empty(starts.size * 2, dtype=np.int64)
    result[::2], result[1::2] = flat[starts], counts
    return result.tolist()


class OnnxAdapter:
    def __init__(self, manifest):
        import numpy as np
        import onnxruntime as ort
        self.config, path = descriptor(manifest)
        with path.open("rb") as stream:
            model_data = stream.read(1024**3 + 1)
        if len(model_data) > 1024**3 or hashlib.sha256(model_data).hexdigest() != self.config["sha256"]:
            raise ValueError("Model checksum mismatch or size limit exceeded")
        options = ort.SessionOptions()
        options.intra_op_num_threads = 2
        options.inter_op_num_threads = 1
        # Loading bytes prohibits unreviewed external tensor paths in ONNX models.
        self.session = ort.InferenceSession(model_data, sess_options=options,
                                           providers=["CPUExecutionProvider"])
        self.session.disable_fallback()
        inp = self.config["input"]
        inputs = self.session.get_inputs()
        shape = [1, 3, inp["height"], inp["width"]]
        if len(inputs) != 1 or inputs[0].name != inp["name"] or inputs[0].type != "tensor(float)":
            raise ValueError("Expected one declared float32 RGB NCHW input")
        if len(inputs[0].shape) != 4 or any(isinstance(a, int) and a != b for a, b in zip(inputs[0].shape, shape)):
            raise ValueError("Model input shape disagrees with manifest")
        if self.config["output"]["name"] not in [x.name for x in self.session.get_outputs()]:
            raise ValueError("Declared output does not exist")
        # Validate actual output, including PIDNet auxiliary-output selection, before advertising it.
        self.validate(self.run(np.zeros(shape, dtype=np.float32)))

    def run(self, tensor):
        return self.session.run([self.config["output"]["name"]],
                                {self.config["input"]["name"]: tensor})[0]

    def validate(self, output):
        import numpy as np
        fmt = self.config["output"]["format"]
        if output.size > 19 * 4_194_304 or not np.isfinite(output).all():
            raise ValueError("Non-finite or oversized model output")
        if fmt == "boxes_xyxy_score_class":
            if output.ndim == 3 and output.shape[0] == 1:
                output = output[0]
            if output.ndim != 2 or output.shape[1] != 6 or output.shape[0] > 10_000:
                raise ValueError("Expected [N,6] post-NMS detections")
            if len(output):
                cls = output[:, 5]
                inp = self.config["input"]
                if (np.any(cls != np.floor(cls)) or cls.min() < 0 or cls.max() >= len(self.config["classMap"])
                        or np.any(output[:, 4] < 0) or np.any(output[:, 4] > 1)
                        or np.any(output[:, :4] < 0) or np.any(output[:, 2] > inp["width"])
                        or np.any(output[:, 3] > inp["height"]) or np.any(output[:, 0] >= output[:, 2])
                        or np.any(output[:, 1] >= output[:, 3])):
                    raise ValueError("Invalid detection coordinates, score or class")
        elif fmt == "logits_nchw":
            if (output.ndim != 4 or output.shape[0] != 1 or output.shape[1] != len(self.config["classMap"])
                    or not all(0 < d <= 4096 for d in output.shape[2:])):
                raise ValueError("Logit shape/class count disagrees with manifest")
        elif (output.ndim != 3 or output.shape[0] != 1 or not all(0 < d <= 4096 for d in output.shape[1:])
              or np.any(output != np.floor(output)) or output.min() < 0
              or output.max() >= len(self.config["classMap"])):
            raise ValueError("Invalid semantic label map")
        return output

    def predict(self, request):
        import numpy as np
        from PIL import Image
        width, height = request["width"], request["height"]
        if (type(width) is not int or type(height) is not int or width <= 0 or height <= 0
                or width * height > MAX_PIXELS or request["orientation"] != 1):
            raise ValueError("Expected bounded canonical orientation=1 image")
        if request["modelVersion"] != self.config["version"]:
            raise ValueError("Stale model version")
        with Image.open(request["imagePath"]) as image:
            if image.size != (width, height):
                raise ValueError("Canonical image dimensions disagree")
            inp = self.config["input"]
            pixels = np.asarray(image.convert("RGB").resize((inp["width"], inp["height"]),
                                                          Image.Resampling.BILINEAR), dtype=np.float32) / 255
        pixels = (pixels - np.array(inp["mean"], dtype=np.float32)) / np.array(inp["std"], dtype=np.float32)
        output = self.validate(self.run(pixels.transpose(2, 0, 1)[None]))
        result = {"width": width, "height": height, "orientation": 1,
                  "coordinateConvention": "canonical-pixel-xyxy", "classMap": self.config["classMap"],
                  "model": self.config, "boxes": [], "mask": None, "confidence": None}
        if self.config["task"] == "boxes":
            threshold = request["parameters"].get("threshold", 0.5)
            if type(threshold) not in (int, float) or not 0 <= threshold <= 1:
                raise ValueError("Invalid score threshold")
            for x1, y1, x2, y2, score, cls in output:
                if score >= threshold:
                    result["boxes"].append({"classIndex": int(cls), "confidence": float(score),
                        "x": float(x1) * width / inp["width"], "y": float(y1) * height / inp["height"],
                        "width": float(x2 - x1) * width / inp["width"],
                        "height": float(y2 - y1) * height / inp["height"]})
            if result["boxes"]:
                result["confidence"] = sum(b["confidence"] for b in result["boxes"]) / len(result["boxes"])
        else:
            if self.config["output"]["format"] == "logits_nchw":
                logits = output[0].astype(np.float32)
                if not np.isfinite(logits).all():
                    raise ValueError("Logits must fit float32")
                # Resize each logit plane before argmax. Streaming softmax avoids a C*H*W canonical tensor.
                best = np.full((height, width), -np.inf, dtype=np.float32)
                total = np.zeros((height, width), dtype=np.float32)
                labels = np.zeros((height, width), dtype=np.uint8)
                for index, plane in enumerate(logits):
                    plane = np.asarray(Image.fromarray(plane).resize((width, height), Image.Resampling.BILINEAR))
                    higher = plane > best
                    next_best = np.maximum(best, plane)
                    with np.errstate(over="ignore", under="ignore"):
                        total = total * np.exp(best - next_best) + np.exp(plane - next_best)
                    labels[higher] = index
                    best = next_best
                result["confidence"] = float((1 / total).mean())
            else:
                labels = output[0].astype(np.uint8)
                labels = np.asarray(Image.fromarray(labels).resize((width, height), Image.Resampling.NEAREST))
            result["mask"] = pack(labels)
        return result
