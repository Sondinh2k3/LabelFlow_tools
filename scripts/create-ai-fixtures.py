"""Generate tiny, project-owned ONNX protocol fixtures, not trained model weights."""
import hashlib
import json
from pathlib import Path
import numpy as np
import onnx
from onnx import TensorProto, helper, numpy_helper


def generate(root):
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    for name, adapter, channels in (("semantic", "onnx_semantic", 3),
                                    ("pidnet-schema", "pidnet_semantic", 19),
                                    ("bad-pidnet", "pidnet_semantic", 2),
                                    ("lowres", "onnx_semantic", 3),
                                    ("boxes", "onnx_boxes", 2)):
        x = helper.make_tensor_value_info("image", TensorProto.FLOAT, [1, 3, 2, 4])
        if adapter == "onnx_boxes":
            data = np.array([[0, 0, 2, 1, .9, 0], [2, 1, 4, 2, .75, 1]], dtype=np.float32)
            nodes = [helper.make_node("Constant", [], ["output"], value=numpy_helper.from_array(data))]
            initializers = []
            shape = [2, 6]
        elif name == "lowres":
            data = np.array([[[[2, 0]], [[0, 10]], [[-10, -10]]]], dtype=np.float32)
            nodes = [helper.make_node("Constant", [], ["output"], value=numpy_helper.from_array(data))]
            initializers = []
            shape = [1, 3, 1, 2]
        else:
            weights = np.zeros((channels, 3, 1, 1), dtype=np.float32)
            for i in range(min(3, channels)):
                weights[i, i, 0, 0] = 1
            initializers = [numpy_helper.from_array(weights, "weights")]
            nodes = [helper.make_node("Conv", ["image", "weights"], ["output"], kernel_shape=[1, 1])]
            shape = [1, channels, 2, 4]
        y = helper.make_tensor_value_info("output", TensorProto.FLOAT, shape)
        graph = helper.make_graph(nodes, name, [x], [y], initializer=initializers)
        model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)], ir_version=9,
                                  producer_name="LabelFlow synthetic schema fixture")
        onnx.checker.check_model(model)
        data = model.SerializeToString()
        (root / f"{name}.onnx").write_bytes(data)
        classes = ([f"candidate-{i}" for i in range(2)] if adapter == "onnx_boxes" else
                   [f"cityscapes-{i}" for i in range(19 if adapter == "pidnet_semantic" else channels)])
        manifest = {"schemaVersion": 1, "id": f"fixture-{name}", "name": f"Fixture {name} (not trained)",
                    "version": "1.0.0", "license": "Project-owned test fixture; no third-party weights",
                    "file": f"{name}.onnx", "sha256": hashlib.sha256(data).hexdigest(), "adapter": adapter,
                    "layerId": "candidate_objects" if adapter == "onnx_boxes" else "cityscapes_semantic",
                    "classMap": classes, "input": {"name": "image", "width": 4, "height": 2,
                        "mean": [0, 0, 0], "std": [1, 1, 1]},
                    "output": {"name": "output", "format": "boxes_xyxy_score_class" if adapter == "onnx_boxes" else "logits_nchw",
                               "resize": "scale_xyxy" if adapter == "onnx_boxes" else "bilinear_half_pixel"}}
        (root / f"{name}.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")


def generate_sam(root, size=256, radius=20.0):
    """Tiny SAM-signature pair. The 'decoder' returns a square of +-radius (in encoder-input
    pixels) around the first point, so tests can verify coordinate scaling end to end."""
    root = Path(root)
    f = TensorProto.FLOAT
    image = helper.make_tensor_value_info("input_image", f, [size, size, 3])
    embed = helper.make_tensor_value_info("image_embeddings", f, [1, 3, size // 16, size // 16])
    encoder = helper.make_graph([
        helper.make_node("Transpose", ["input_image"], ["chw"], perm=[2, 0, 1]),
        helper.make_node("Unsqueeze", ["chw", "zero"], ["nchw"]),
        helper.make_node("AveragePool", ["nchw"], ["image_embeddings"], kernel_shape=[16, 16], strides=[16, 16]),
    ], "sam-encoder", [image], [embed],
        initializer=[numpy_helper.from_array(np.array([0], dtype=np.int64), "zero")])
    c = lambda name, value: numpy_helper.from_array(np.array(value, dtype=np.float32), name)  # noqa: E731
    i = lambda name, value: numpy_helper.from_array(np.array(value, dtype=np.int64), name)  # noqa: E731
    inputs = [helper.make_tensor_value_info("image_embeddings", f, [1, 3, None, None]),
              helper.make_tensor_value_info("point_coords", f, [1, None, 2]),
              helper.make_tensor_value_info("point_labels", f, [1, None]),
              helper.make_tensor_value_info("mask_input", f, [1, 1, 256, 256]),
              helper.make_tensor_value_info("has_mask_input", f, [1]),
              helper.make_tensor_value_info("orig_im_size", f, [2])]
    outputs = [helper.make_tensor_value_info("masks", f, [1, 1, None, None]),
               helper.make_tensor_value_info("iou_predictions", f, [1, 1]),
               helper.make_tensor_value_info("low_res_masks", f, [1, 1, 256, 256])]
    nodes = [
        helper.make_node("Gather", ["orig_im_size", "i0"], ["h"], axis=0),
        helper.make_node("Gather", ["orig_im_size", "i1"], ["w"], axis=0),
        helper.make_node("Max", ["h", "w"], ["longest"]),
        helper.make_node("Div", ["size", "longest"], ["scale"]),
        helper.make_node("Range", ["half", "h", "one"], ["ys"]),
        helper.make_node("Range", ["half", "w", "one"], ["xs"]),
        helper.make_node("Mul", ["ys", "scale"], ["ys_s"]),
        helper.make_node("Mul", ["xs", "scale"], ["xs_s"]),
        helper.make_node("Gather", ["point_coords", "i0"], ["first_b"], axis=1),
        helper.make_node("Reshape", ["first_b", "two"], ["first"]),
        helper.make_node("Gather", ["first", "i0"], ["px"], axis=0),
        helper.make_node("Gather", ["first", "i1"], ["py"], axis=0),
        helper.make_node("Sub", ["ys_s", "py"], ["dy"]),
        helper.make_node("Sub", ["xs_s", "px"], ["dx"]),
        helper.make_node("Abs", ["dy"], ["ady"]),
        helper.make_node("Abs", ["dx"], ["adx"]),
        helper.make_node("Less", ["ady", "radius"], ["iny"]),
        helper.make_node("Less", ["adx", "radius"], ["inx"]),
        helper.make_node("Unsqueeze", ["iny", "i1"], ["col"]),
        helper.make_node("Unsqueeze", ["inx", "i0"], ["row"]),
        helper.make_node("And", ["col", "row"], ["inside"]),
        helper.make_node("Cast", ["inside"], ["inside_f"], to=f),
        helper.make_node("Sub", ["inside_f", "half"], ["logits"]),
        helper.make_node("Unsqueeze", ["logits", "axes01"], ["masks"]),
        helper.make_node("Constant", [], ["iou_predictions"], value=c("iou", [[0.9]])),
        helper.make_node("Constant", [], ["low_res_masks"], value=c("low", np.zeros((1, 1, 256, 256)))),
    ]
    decoder = helper.make_graph(nodes, "sam-decoder", inputs, outputs, initializer=[
        c("size", float(size)), c("half", 0.5), c("one", 1.0), c("radius", radius),
        i("i0", 0), i("i1", 1), i("two", [2]), i("axes01", [0, 1])])
    manifest = {"schemaVersion": 1, "kind": "sam_prompt", "id": "fixture-sam",
                "name": "Fixture SAM (not trained)", "version": "1.0.0",
                "license": "Project-owned test fixture; no third-party weights",
                "input": {"size": size}}
    for graph, key in ((encoder, "encoder"), (decoder, "decoder")):
        model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)], ir_version=9,
                                  producer_name="LabelFlow synthetic schema fixture")
        onnx.checker.check_model(model)
        data = model.SerializeToString()
        (root / f"sam-{key}.onnx").write_bytes(data)
        manifest[key] = {"file": f"sam-{key}.onnx", "sha256": hashlib.sha256(data).hexdigest()}
    (root / "sam.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    generate(Path(__file__).resolve().parents[1] / "tests/fixtures/ai")
    generate_sam(Path(__file__).resolve().parents[1] / "tests/fixtures/ai")
