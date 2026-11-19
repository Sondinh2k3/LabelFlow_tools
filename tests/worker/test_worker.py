import copy
import importlib.util
import json
import queue
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "worker"))
from adapters import descriptor  # noqa: E402


class ManifestTests(unittest.TestCase):
    def test_stable_mapping_and_traversal_are_rejected(self):
        source = ROOT / "tests/fixtures/ai/semantic.json"
        value = json.loads(source.read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "model.json"
            for key, invalid in (("file", "../model.onnx"), ("classMap", ["cityscapes-0"] * 2),
                                 ("adapter", "python_plugin"), ("schemaVersion", 9)):
                bad = copy.deepcopy(value)
                bad[key] = invalid
                path.write_text(json.dumps(bad), encoding="utf-8")
                with self.assertRaises((ValueError, FileNotFoundError)):
                    descriptor(path)


@unittest.skipUnless(all(importlib.util.find_spec(m) for m in ("numpy", "PIL", "onnxruntime")),
                     "Install optional worker/requirements.txt for real CPU inference tests")
class WorkerTests(unittest.TestCase):
    def setUp(self):
        self.child = subprocess.Popen([sys.executable, "-I", "-u", "-B", str(ROOT / "worker/main.py")],
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                      text=True, encoding="utf-8")
        self.count = 0

    def tearDown(self):
        self.child.kill()
        self.child.communicate(timeout=10)

    def request(self, op, args=None, protocol=1):
        self.count += 1
        identity = f"test-{self.count}"
        self.child.stdin.write(json.dumps({"protocol_version": protocol, "requestId": identity,
                                           "operation": op, "args": args or {}}) + "\n")
        self.child.stdin.flush()
        received = queue.Queue()
        threading.Thread(target=lambda: received.put(self.child.stdout.readline()), daemon=True).start()
        try:
            line = received.get(timeout=10)
        except queue.Empty:
            self.fail(f"Worker timed out during {op}")
        response = json.loads(line)
        self.assertEqual(response["requestId"], identity)
        self.assertEqual(response["protocol_version"], 1)
        return response

    def load(self, name):
        return self.request("load_model", {"manifest": str(ROOT / f"tests/fixtures/ai/{name}.json")})

    def predict(self, name, task, threshold=.5):
        from PIL import Image
        with tempfile.TemporaryDirectory() as root:
            image = Image.new("RGB", (4, 2))
            image.putdata([(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 0, 0)] * 2)
            path = Path(root) / "canonical.png"
            image.save(path)
            return self.request(f"predict_{task}", {"imagePath": str(path), "width": 4, "height": 2,
                "orientation": 1, "modelVersion": "1.0.0", "parameters": {"threshold": threshold}})

    def test_handshake_version_and_capabilities(self):
        health = self.request("health")["result"]
        self.assertEqual(health["missingDependencies"], [])
        self.assertEqual(health["provider"], "CPUExecutionProvider")
        self.assertNotIn("predict_prompt_mask", health["capabilities"])
        catalog = self.request("health", {"manifest": str(ROOT / "tests/fixtures/ai/semantic.json")})["result"]
        self.assertEqual(catalog["availableModels"][0]["id"], "fixture-semantic")
        self.assertIn("error", self.request("health", protocol=9))

    def test_actual_semantic_pixels_and_pidnet_schema(self):
        for name in ("semantic", "pidnet-schema"):
            self.assertIn("result", self.load(name))
            value = self.predict(name, "semantic")["result"]
            pixels = []
            for value_id, count in zip(value["mask"][::2], value["mask"][1::2]):
                pixels += [value_id] * count
            self.assertEqual(pixels, [0, 1, 2, 0] * 2)
            self.assertTrue(0 < value["confidence"] < 1)
            self.assertEqual(value["processing"]["device"], "cpu")

    def test_pidnet_actual_wrong_channel_schema_is_rejected(self):
        self.assertIn("error", self.load("bad-pidnet"))
        self.assertIn("error", self.predict("bad-pidnet", "semantic"))

    def test_logits_are_resized_before_argmax(self):
        self.assertIn("result", self.load("lowres"))
        value = self.predict("lowres", "semantic")["result"]
        self.assertEqual(value["mask"], [0, 1, 1, 3, 0, 1, 1, 3])

    def test_model_hash_is_checked_on_the_exact_loaded_bytes(self):
        with tempfile.TemporaryDirectory() as root:
            manifest = json.loads((ROOT / "tests/fixtures/ai/semantic.json").read_text(encoding="utf-8"))
            manifest["sha256"] = "0" * 64
            path = Path(root) / "model.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            shutil.copyfile(ROOT / "tests/fixtures/ai/semantic.onnx", Path(root) / "semantic.onnx")
            result = self.request("load_model", {"manifest": str(path)})
            self.assertIn("checksum mismatch", result["error"]["message"])

    def test_boxes_threshold_coordinates_and_no_prompt_fallback(self):
        self.assertIn("result", self.load("boxes"))
        value = self.predict("boxes", "boxes", .8)["result"]
        self.assertEqual(len(value["boxes"]), 1)
        self.assertEqual(value["boxes"][0]["width"], 2)
        self.assertIsNone(value["mask"])
        self.assertIn("error", self.request("predict_prompt_mask"))
        self.assertIn("error", self.request("predict_semantic"))

    def test_wrong_dimensions_version_and_unload(self):
        self.load("semantic")
        args = {"width": 10, "height": 10, "orientation": 1, "modelVersion": "wrong", "imagePath": "missing"}
        self.assertIn("error", self.request("predict_semantic", args))
        self.assertIn("result", self.request("unload"))
        self.assertIn("error", self.predict("semantic", "semantic"))
        self.assertIn("result", self.request("cancel"))

    def test_sam_click_to_mask_scales_points_and_caches_embedding(self):
        from PIL import Image
        loaded = self.request("sam_load", {"manifest": str(ROOT / "tests/fixtures/ai/sam.json")})
        self.assertEqual(loaded["result"]["kind"], "sam_prompt")
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "canonical.png"
            Image.new("RGB", (640, 360), (40, 80, 120)).save(path)
            base = {"imagePath": str(path), "width": 640, "height": 360, "sourceSha256": "a" * 64}
            first = self.request("sam_predict", {**base, "encodeOnly": True})["result"]
            self.assertFalse(first["cached"])
            value = self.request("sam_predict", {**base, "encodeOnly": False,
                                                 "points": [{"x": 320, "y": 180, "positive": True}],
                                                 "box": None})["result"]
            self.assertTrue(value["cached"])
            pixels = []
            for v, n in zip(value["runs"][::2], value["runs"][1::2]):
                pixels += [v] * n
            self.assertEqual(len(pixels), 640 * 360)
            # Radius 20 encoder pixels at scale 256/640 = 50 image pixels around (320, 180).
            self.assertEqual(pixels[180 * 640 + 320], 1)
            self.assertEqual(pixels[180 * 640 + 365], 1)
            self.assertEqual(pixels[180 * 640 + 375], 0)
            self.assertEqual(pixels[100 * 640 + 320], 0)
            self.assertEqual(value["area"], sum(pixels))
            bad = self.request("sam_predict", {**base, "encodeOnly": False,
                                               "points": [{"x": 9999, "y": 1, "positive": True}]})
            self.assertIn("error", bad)
            self.assertIn("error", self.request("sam_predict", {**base, "encodeOnly": False, "points": []}))
        wrong = self.request("sam_load", {"manifest": str(ROOT / "tests/fixtures/ai/semantic.json")})
        self.assertIn("error", wrong)

    def test_parent_pipe_closure_exits_worker(self):
        self.request("health")
        self.child.stdin.close()
        self.child.stdin = None
        self.assertEqual(self.child.wait(timeout=3), 0)


if __name__ == "__main__":
    unittest.main()
