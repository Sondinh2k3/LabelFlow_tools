"""Điền giay_ro_nhat, hop_bao, da_giac_long_duong, muc_chiem cho test_events.csv (Hướng dẫn v3 §9.2)
từ thư mục xuất LabelFlow của các "khung rõ nhất" trong video test.

Cách dùng:
  python tools/test_events.py <thư mục xuất> <vnroad>/manifest/test_events.csv [--lop tools/lop.yaml]

Cách gán trong LabelFlow (một project riêng cho video test):
  * Mỗi khung rõ nhất: mỗi vật một hộp bao trên layer "Candidate objects" (toàn bộ phần nhìn thấy, kể cả
    phần ngoài lòng đường) và đa giác lòng đường trên layer "Road Extent".
  * Ô Notes của khung: obj_id của từng hộp, cách nhau bằng dấu cách, theo thứ tự hộp từ trái sang phải
    (cạnh trái hộp), ví dụ "OBJ_041 OBJ_042".
  * Export dataset: "YOLO detection + semantic/region PNGs", Train % = 100.

muc_chiem = với mỗi hàng ảnh hộp bao đi qua: bề rộng (hộp ∩ đa giác) / bề rộng đa giác trên hàng đó;
lấy giá trị lớn nhất. da_giac_long_duong = đường dẫn mặt nạ Road Extent trong thư mục xuất (giữ thư mục
xuất cùng bộ dữ liệu). Các cột khác của CSV giữ nguyên; 4 cột trên được ghi đè cho mọi obj_id tìm thấy.
"""
import argparse
import csv
import json
from pathlib import Path

import numpy as np

import lf_common as lf


def occupancy(road, x, y, w, h):
    """Tỷ lệ chiếm lớn nhất theo hàng của hộp (x, y, w, h) trong vùng road."""
    H, W = road.shape
    band = road[max(0, y):min(H, y + h)]
    width = band.sum(axis=1)
    inside = band[:, max(0, x):min(W, x + w)].sum(axis=1)
    ok = width > 0
    return float((inside[ok] / width[ok]).max()) if ok.any() else 0.0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("export", type=Path)
    ap.add_argument("events", type=Path)
    ap.add_argument("--lop", type=Path, default=None)
    args = ap.parse_args()
    road_layer = lf.load_cfg(args.lop)["labelflow"]["lop_long_duong"]
    manifest = json.loads((args.export / "manifest.json").read_text(encoding="utf-8"))
    found = {}
    for img in manifest["images"]:
        ids = (img.get("notes") or "").split()
        if not ids:
            continue
        w, h = img["width"], img["height"]
        name = Path(img["source"].replace("\\", "/")).name
        label_file = args.export / "labels" / img["split"] / f"{img['id']}.txt"
        road_rel = f"masks/{road_layer}/{img['split']}/{img['id']}.png"
        boxes = []
        for line in label_file.read_text().splitlines() if label_file.is_file() else []:
            _, cx, cy, bw, bh = map(float, line.split())
            boxes.append((round((cx - bw / 2) * w), round((cy - bh / 2) * h), round(bw * w), round(bh * h)))
        boxes.sort()
        if len(boxes) != len(ids):
            print(f"{name}: {len(boxes)} hộp bao nhưng Notes có {len(ids)} obj_id — bỏ qua khung này")
            continue
        road = lf.read_png(args.export / road_rel)
        if road is None or not (road == 1).any():
            print(f"{name}: khung chưa có đa giác Road Extent, muc_chiem = 0")
            road, road_rel = np.zeros((h, w), bool), ""
        else:
            road = road == 1
        seconds = (img.get("sourceVideo") or {}).get("timeMs")
        for obj, (x, y, bw, bh) in zip(ids, boxes):
            found[obj] = {"hop_bao": f"{x},{y},{bw},{bh}", "muc_chiem": f"{occupancy(road, x, y, bw, bh):.3f}",
                          "da_giac_long_duong": road_rel,
                          "giay_ro_nhat": f"{seconds / 1000:.1f}" if seconds is not None else ""}

    with args.events.open(encoding="utf-8-sig", newline="") as f:
        reader = csv.DictReader(f)
        rows, fields = list(reader), list(reader.fieldnames or [])
    if "obj_id" not in fields:
        raise SystemExit("test_events.csv cần cột obj_id")
    for col in ("giay_ro_nhat", "hop_bao", "da_giac_long_duong", "muc_chiem"):
        if col not in fields:
            fields.append(col)
    for row in rows:
        row.update(found.pop(row["obj_id"].strip(), {}))
    with args.events.open("w", encoding="utf-8-sig", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)
    if found:
        print("obj_id có trong Notes nhưng không có trong CSV:", ", ".join(sorted(found)))
    print(f"Đã cập nhật {args.events}")


if __name__ == "__main__":
    main()
