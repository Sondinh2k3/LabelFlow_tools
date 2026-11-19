"""Độ trùng khớp giữa hai người gán cùng một bộ ảnh (hiệu chuẩn 20 ảnh, gán đôi 5% — Hướng dẫn v3 §10.1).

Cách dùng:
  python tools/agreement.py <thư mục xuất A> <thư mục xuất B> [--min 0.95] [--lop tools/lop.yaml]

Hai thư mục xuất LabelFlow (Semantic + region PNGs) từ hai project/hai máy; ảnh được ghép theo SHA-256
của ảnh gốc trong manifest.json. In ra:
  - Semantic: tỷ lệ pixel trùng lớp, chỉ tính pixel mà CẢ HAI người đều đã tô (khác 255), vì chỗ chưa tô
    và Ignore template đều là 255. Lớp cũ bo_qua (doi_id trong lop.yaml) cũng tính là 255.
  - Road Extent: IoU.
Ảnh dưới ngưỡng --min được đánh dấu để hai người mở cạnh nhau và thống nhất quy tắc.
"""
import argparse
import json
from pathlib import Path

import numpy as np

import lf_common as lf


def index(root):
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    return {img["sourceSha256"]: img for img in manifest["images"]}


def mask(root, layer, img):
    return lf.read_png(root / "masks" / layer / img["split"] / f"{img['id']}.png")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("a", type=Path)
    ap.add_argument("b", type=Path)
    ap.add_argument("--min", type=float, default=0.95, help="đánh dấu ảnh trùng dưới ngưỡng này")
    ap.add_argument("--lop", type=Path, default=None)
    args = ap.parse_args()
    cfg = lf.load_cfg(args.lop)
    sem_layer, road_layer = cfg["labelflow"]["lop_semantic"], cfg["labelflow"]["lop_long_duong"]
    to_ignore = [int(k) for k, v in (cfg["labelflow"].get("doi_id") or {}).items() if int(v) == cfg["bo_qua"]]

    left, right = index(args.a), index(args.b)
    shared = sorted(set(left) & set(right))
    if not shared:
        raise SystemExit("Không có ảnh chung (ghép theo SHA-256 ảnh gốc).")
    total_equal = total_scored = inter = union = 0
    rows = []
    for sha in shared:
        ia, ib = left[sha], right[sha]
        name = Path(ia["source"].replace("\\", "/")).name
        a, b = mask(args.a, sem_layer, ia), mask(args.b, sem_layer, ib)
        score = None
        if a is not None and b is not None and a.shape == b.shape:
            scored = (a != 255) & (b != 255) & ~np.isin(a, to_ignore) & ~np.isin(b, to_ignore)
            n = int(scored.sum())
            if n:
                equal = int(((a == b) & scored).sum())
                total_equal += equal
                total_scored += n
                score = equal / n
        ra, rb = mask(args.a, road_layer, ia), mask(args.b, road_layer, ib)
        iou = None
        if ra is not None and rb is not None and ra.shape == rb.shape:
            i, u = int(((ra == 1) & (rb == 1)).sum()), int(((ra == 1) | (rb == 1)).sum())
            inter, union = inter + i, union + u
            iou = i / u if u else None
        rows.append((score, iou, name))

    print(f"{len(shared)} ảnh chung")
    for score, iou, name in sorted(rows, key=lambda r: (r[0] is None, r[0] or 0)):
        flag = "  <-- dưới ngưỡng" if score is not None and score < args.min else ""
        s = "  n/a " if score is None else f"{score:6.1%}"
        r = "  n/a " if iou is None else f"{iou:6.1%}"
        print(f"  semantic {s}  road IoU {r}  {name}{flag}")
    if total_scored:
        print(f"Semantic trùng (không tính 255): {total_equal / total_scored:.2%} (mục tiêu {args.min:.0%})")
    if union:
        print(f"Road Extent IoU: {inter / union:.2%}")


if __name__ == "__main__":
    main()
