# Optional local AI worker (protocol v1)

The desktop works without this runtime. Nothing starts until **AI assistance → Start / restart worker**. The executable embeds the two project-owned worker source files and extracts immutable, checksum-keyed copies into the application configuration directory. Python, ONNX Runtime and trained models are never bundled or downloaded automatically.

## Set up an environment

```powershell
python -m venv .cache/ai-venv
.cache/ai-venv/Scripts/python.exe -m pip install -r worker/requirements.txt
npm.cmd run test:ai
```

On Linux use `.cache/ai-venv/bin/python`. `test:ai` also accepts an absolute `LABELFLOW_TEST_PYTHON`. Dependency installation is an explicit development/user operation; runtime inference is offline. Tested on Windows with Python 3.12, ONNX Runtime 1.31.1, NumPy 2.5.4 and Pillow 13.0.0. ONNX 1.23.2 is only used to regenerate fixtures, not to run models.

## Select a model

1. Supply a local `.onnx` and a JSON manifest beside it. Start from [semantic example](../tests/fixtures/ai/semantic.json) or [boxes example](../tests/fixtures/ai/boxes.json). These examples point to tiny synthetic test graphs, **not trained predictors**. Replace identity, version, license, SHA-256, input/output names, dimensions, normalization and class map for your own model.
2. In the desktop, choose a Python executable and then its model manifest using native file pickers. Paths are saved as local application settings, outside the project. Project files cannot choose executables or auto-start workers.
3. Start the worker. It validates the handshake, dependencies, model checksum, graph input/output metadata **and a real dry-run output** before the UI enables prediction. Invalid configuration produces an error; manual labeling stays available.
4. Generate a suggestion, inspect its overlay and provenance, then accept or reject. Accept appends boxes, or fills only semantic pixels currently equal to 255. Existing objects and semantic values survive. Accepted documents become `needs_review`; approving the image itself is still a human review action.
5. A changed document revision makes a prediction stale. Reject it and regenerate. Approved documents must first be reopened for review outside the AI dialog. Cancel kills the worker, discards the in-flight result, and requires an explicit restart. Stop unloads all state.

## Supported explicit schemas

Manifest `schemaVersion: 1`; no Python plugin entry points. `classMap[i]` is a stable project class ID, not a display name, export index guessed from order, or RGB color. Semantic mappings must target the selected semantic namespace; box mappings must target an instance namespace. Ignore is not a predicted class in this version.

- **Input:** one float32 `[1,3,H,W]` RGB tensor. Source is an orientation-normalized canonical PNG supplied by Rust. Pillow bilinear stretch to declared width/height, scale RGB to `[0,1]`, then `(pixel - mean) / std` per channel. No implicit letterboxing or BGR swap. Dynamic graph dimensions are allowed only when the explicit manifest dimensions and dry run work.
- **`onnx_semantic` / `logits_nchw`:** selected output `[1,C,h,w]`; `C == classMap.length`. Declare `output.resize: "bilinear_half_pixel"`. Each float32 logit plane is resized with Pillow bilinear interpolation to canonical dimensions **before argmax**. Softmax confidence is the mean winning probability, computed across the same resized logits. Ties choose the first channel.
- **`onnx_semantic` / `labels_nhw`:** selected output `[1,h,w]` containing integer class indices. Declare `resize: "nearest"`. Confidence is `null` because the graph does not provide scores.
- **`pidnet_semantic`:** the same validated logits path, targeting the semantic layer (`cityscapes_semantic`). `classMap` must list **every** project semantic class ID in export-ID order (the 19 `cityscapes-0`…`cityscapes-18` IDs in a default project, or the project's own class IDs from `classes.json`), one logit channel each. An explicit output name selects the semantic head; auxiliary outputs are not guessed. Models requiring align-corners interpolation, different normalization, multi-input graphs, or other output schemas need a tested adapter extension. This contract has been tested with a 19-channel schema fixture; no trained PIDNet model has been supplied or quality-validated.
- **`onnx_boxes`:** selected float output `[N,6]` or `[1,N,6]`, post-NMS rows `x1,y1,x2,y2,score,class_index`, in declared input-pixel coordinates. Declare `format: "boxes_xyxy_score_class"`, `resize: "scale_xyxy"`. Coordinates scale to canonical image pixels; threshold only filters scores. Raw YOLO heads, normalized coordinates and automatic NMS are intentionally unsupported by this adapter.
- **Prompt masks:** `predict_prompt_mask` is not used. Click-to-mask runs in a **separate** worker process through `sam_load` / `sam_predict` (see below).

## SAM click-to-mask (`kind: sam_prompt`)

A second worker process loads a SAM-family encoder/decoder pair (`worker/sam.py`). Manifest (write it with `scripts/make-sam-manifest.py`):

```json
{
  "schemaVersion": 1,
  "kind": "sam_prompt",
  "id": "mobile-sam",
  "name": "MobileSAM",
  "version": "1",
  "license": "Apache-2.0",
  "input": { "mean": [0, 0, 0], "std": [1, 1, 1] },
  "encoder": { "file": "mobile_sam.encoder.onnx", "sha256": "…" },
  "decoder": { "file": "mobile_sam.decoder.onnx", "sha256": "…" }
}
```

- Encoder: one float input, HWC `[S,S,3]` RGB 0–255 (normalization inside the graph, e.g. samexporter `--use-preprocess`) or NCHW `[1,3,S,S]` normalized with `input.mean/std` (0–255 scale; SAM defaults if omitted). `S` comes from the graph when static, else `input.size` (default 1024). Longest side is resized to `S`, padded bottom/right.
- Decoder: the official segment-anything ONNX inputs `image_embeddings, point_coords, point_labels, mask_input, has_mask_input, orig_im_size`; outputs `masks, iou_predictions, low_res_masks`. Multi-mask outputs pick the highest predicted IoU.
- `sam_predict` args: `imagePath, width, height, sourceSha256, encodeOnly, points[{x,y,positive}], box{x,y,width,height}|null` in canonical pixels. Result: `runs` (0/1 value/count RLE, row-major), `score`, `area`, `cached`, `milliseconds`. Embeddings are cached per SHA-256 (4 images).

## JSON-lines IPC

Tauri launches the selected Python directly with fixed arguments `-I -u -B <embedded-worker>`, with piped stdin/stdout and no shell or TCP listener. `-I` isolates user-site packages and environment import paths: install dependencies in the **selected environment**, not only the user's global site directory. Each request is one UTF-8 line:

```json
{
  "protocol_version": 1,
  "requestId": "unique-request-id",
  "operation": "health",
  "args": {}
}
```

Responses echo the version and request ID and contain exactly one `result` or `error`. Health declares protocol version, capabilities, loaded/available models, CPU provider/device, canonical input geometry and output format. Operations: `health`, `list_models` (selected manifest), `load_model`, `predict_boxes`, `predict_semantic`, `predict_prompt_mask` (unsupported), `cancel`, `unload`.

Prediction requests include canonical local image path, media ID, width/height, orientation=1, source orientation/SHA, model version and parameters. Responses include dimensions/orientation, `canonical-pixel-xyxy` convention, explicit class map, model identity/version/hash/license/input/output contract, boxes or row-major alternating class-index/count mask RLE, confidence and CPU processing time. Rust independently validates every response against the loaded model and project profile before storage.

The worker processes one request at a time. A serial `cancel` request can unload an idle model; **in-flight cancellation is implemented by the Rust supervisor terminating the process**, because a native ORT call need not yield to Python. Generation tokens reject late responses before persistence. Restart never resumes an old request. EOF/parent-pipe closure terminates the worker; on Windows a nonblocking `PeekNamedPipe` watcher avoids a CRT stdin-read/NumPy DLL initialization deadlock. Worker crashes, malformed lines, version/ID mismatch, timeout or inference errors invalidate the worker and leave manual documents unchanged.

Limits: manifest 64 KiB, single-file ONNX 1 GiB (external tensors unsupported), model input ≤4096 per side and ≤4,194,304 pixels, canonical image ≤24 megapixels, output ≤19×4,194,304 elements, ≤10,000 detections, mask ≤1,000,000 RLE values, IPC line ≤16 MiB. Health/catalog timeout 30 s; load/inference timeout 120 s. The Rust receiver buffers at most one response; stderr is drained without retention. These are protocol limits; model loading/execution is a local native dependency and is not an OS memory sandbox. Use models and interpreters you trust.

References: [ONNX Runtime Python InferenceSession API](https://onnxruntime.ai/docs/api/python/api_summary.html), [execution providers](https://onnxruntime.ai/docs/execution-providers/). Only `CPUExecutionProvider` is selected; no GPU/provider fallback is assumed.

## Test fixtures

`tests/fixtures/ai/*.onnx` are project-generated tiny Conv/Constant graphs with no third-party trained weights. `scripts/create-ai-fixtures.py` regenerates them when optional `onnx` is installed. `bad-pidnet` intentionally has the wrong channel count; `lowres` verifies that resizing logits precedes argmax. Regenerated JSON may need `npx prettier --write tests/fixtures/ai/*.json`.
