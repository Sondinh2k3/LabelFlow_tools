"""LabelFlow stdio JSON-lines protocol v1. stdout is exclusively protocol data."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from adapters import OnnxAdapter, descriptor  # noqa: E402
from sam import SamAdapter  # noqa: E402

PROTOCOL = 1
MAX_LINE = 16 * 1024 * 1024


def serve():
    sys.stdout.reconfigure(encoding="utf-8", errors="strict")
    sys.stdin.reconfigure(encoding="utf-8", errors="strict")
    # Poll pipe liveness without a blocking CRT read on another thread (Windows NumPy loader deadlock).
    if sys.platform == "win32":
        import ctypes
        from ctypes import wintypes
        import msvcrt
        peek = ctypes.WinDLL("kernel32", use_last_error=True).PeekNamedPipe
        peek.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD,
                         ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
        peek.restype = wintypes.BOOL
        handle = msvcrt.get_osfhandle(sys.stdin.fileno())

        def watch_parent():
            while True:
                if not peek(handle, None, 0, None, None, None):
                    if ctypes.get_last_error() in (109, 232, 233):
                        os._exit(0)
                    return
                time.sleep(.1)
    else:
        import select
        poll = select.poll()
        poll.register(sys.stdin, select.POLLHUP | select.POLLERR)

        def watch_parent():
            if poll.poll():
                os._exit(0)

    threading.Thread(target=watch_parent, daemon=True).start()
    model = None
    sam = None
    while True:
        line = sys.stdin.readline(MAX_LINE + 1)
        if not line or len(line) > MAX_LINE or not line.endswith("\n"):
            return
        request_id = None
        try:
            request = json.loads(line)
            request_id = request["requestId"]
            if request["protocol_version"] != PROTOCOL or not isinstance(request_id, str) or len(request_id) > 128:
                raise ValueError("Unsupported protocol or invalid request ID")
            op = request["operation"]
            args = request.get("args", {})
            if op == "health":
                missing = [name for name in ("numpy", "PIL", "onnxruntime") if importlib.util.find_spec(name) is None]
                available = [descriptor(args["manifest"])[0]] if args.get("manifest") else []
                result = {"protocol_version": PROTOCOL, "capabilities": ["health", "list_models", "load_model",
                    "predict_boxes", "predict_semantic", "sam_load", "sam_predict", "cancel", "unload"], "provider": "CPUExecutionProvider",
                    "device": "cpu", "missingDependencies": missing, "models": [model.config] if model else [],
                    "availableModels": available,
                    "inputGeometry": "RGB float32 NCHW; stretch; canonical orientation=1",
                    "outputFormat": "canonical-pixel-xyxy; row-major value/count RLE"}
            elif op == "list_models":
                result = [descriptor(args["manifest"])[0]] if args.get("manifest") else []
            elif op == "load_model":
                model = None
                model = OnnxAdapter(args["manifest"])
                result = model.config
            elif op in ("predict_boxes", "predict_semantic", "predict_prompt_mask"):
                if model is None or op != "predict_" + model.config["task"]:
                    raise ValueError("Selected model does not support this prediction capability")
                started = time.monotonic()
                result = model.predict(args)
                result["processing"] = {"milliseconds": round((time.monotonic() - started) * 1000, 3),
                                        "provider": "CPUExecutionProvider", "device": "cpu"}
            elif op == "sam_load":
                sam = None
                sam = SamAdapter(args["manifest"])
                result = sam.config
            elif op == "sam_predict":
                if sam is None:
                    raise ValueError("Load a SAM model first")
                started = time.monotonic()
                result = sam.predict(args)
                result["milliseconds"] = round((time.monotonic() - started) * 1000, 3)
            elif op in ("cancel", "unload"):
                # The host terminates this process for an in-flight native ORT call.
                model = None
                sam = None
                result = {"unloaded": True}
            else:
                raise ValueError("Unsupported operation")
            response = {"protocol_version": PROTOCOL, "requestId": request_id, "result": result}
        except Exception as error:
            response = {"protocol_version": PROTOCOL, "requestId": request_id,
                        "error": {"code": "worker_error", "message": str(error)[:2000]}}
        encoded = json.dumps(response, separators=(",", ":"), ensure_ascii=True, allow_nan=False)
        if len(encoded) > MAX_LINE:
            encoded = json.dumps({"protocol_version": PROTOCOL, "requestId": request_id,
                                  "error": {"code": "output_limit", "message": "Prediction exceeds IPC budget"}})
        print(encoded, flush=True)


if __name__ == "__main__":
    serve()
