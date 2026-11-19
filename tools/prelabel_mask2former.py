"""Nhãn sơ bộ vòng 1 (Hướng dẫn v3 §8.3, Phụ lục B): chạy Mask2Former Swin-L Mapillary Vistas
NGOÀI LabelFlow, ánh xạ về 11 lớp dự án (0-10) + Bỏ qua (255), ghi mỗi ảnh một PNG một kênh
TRÙNG TÊN với ảnh. Nhập vào LabelFlow: Import mask folder -> layer Semantic -> ánh xạ giữ nguyên
(0->0 ... 10->10, 255->255) -> chế độ Replace.

Cách dùng (trên máy có GPU):
  pip install torch transformers pillow numpy pyyaml
  python prelabel_mask2former.py --tu-kiem                       # kiểm tra bảng ánh xạ, không cần mô hình
  python prelabel_mask2former.py <thư mục ảnh> <thư mục ra> --xem-anh-xa   # in ánh xạ theo config mô hình
  python prelabel_mask2former.py <thư mục ảnh> <thư mục ra> [--model <id HF hoặc thư mục>] [--fp16]

Chạy --xem-anh-xa trước và đối chiếu từng lớp Vistas với Phụ lục B (tên lớp lấy từ id2label
trong config.json của mô hình; tên không có trong bảng -> Khác). Thử 5 ảnh trước khi chạy cả lô.
Chưa chạy thử với mô hình thật trong repo này (máy này không có GPU/mô hình).
"""
import argparse
import sys
from pathlib import Path

import numpy as np
from PIL import Image

for _stream in (sys.stdout, sys.stderr):  # console Windows cp1252 không in được tiếng Việt
    try:
        _stream.reconfigure(encoding="utf-8")
    except (AttributeError, ValueError):
        pass

MODEL = "facebook/mask2former-swin-large-mapillary-vistas-semantic"
BO_QUA, KHAC = 255, 10
# Phụ lục B: (ID dự án, tên lớp Vistas viết thường; '*' cuối = tiền tố)
RULES = [
    (0, ["road", "lane marking*", "crosswalk - plain", "manhole", "catch basin", "pothole", "bike lane",
         "service lane", "parking"]),
    (1, ["sidewalk", "curb", "curb cut", "pedestrian area", "terrain", "sand"]),
    (2, ["building", "wall", "fence", "bridge", "tunnel"]),
    (3, ["guard rail", "barrier"]),          # sửa tay: khối bê tông tạm của công trình -> Khác
    (4, ["motorcycle", "bicycle", "motorcyclist", "bicyclist", "other rider"]),
    (5, ["car"]),
    (6, ["truck", "bus", "trailer", "caravan", "on rails"]),
    (7, ["wheeled slow", "other vehicle"]),
    (8, ["person"]),
    (9, ["ground animal", "bird"]),
    (BO_QUA, ["ego vehicle", "car mount", "unlabeled"]),
]
NAMES = {0: "mat_duong", 1: "via_he_le", 2: "nha_tuong", 3: "ket_cau_co_dinh", 4: "xe_may_xe_dap", 5: "o_to",
         6: "xe_tai_xe_buyt", 7: "xe_tho_so", 8: "nguoi", 9: "dong_vat", 10: "khac", 255: "bo_qua"}
# 66 lớp Mapillary Vistas v1.2, để tự kiểm tra bảng ánh xạ khi chưa có mô hình
VISTAS_12 = ["Bird", "Ground Animal", "Curb", "Fence", "Guard Rail", "Barrier", "Wall", "Bike Lane",
             "Crosswalk - Plain", "Curb Cut", "Parking", "Pedestrian Area", "Rail Track", "Road", "Service Lane",
             "Sidewalk", "Bridge", "Building", "Tunnel", "Person", "Bicyclist", "Motorcyclist", "Other Rider",
             "Lane Marking - Crosswalk", "Lane Marking - General", "Mountain", "Sand", "Sky", "Snow", "Terrain",
             "Vegetation", "Water", "Banner", "Bench", "Bike Rack", "Billboard", "Catch Basin", "CCTV Camera",
             "Fire Hydrant", "Junction Box", "Mailbox", "Manhole", "Phone Booth", "Pothole", "Street Light", "Pole",
             "Traffic Sign Frame", "Utility Pole", "Traffic Light", "Traffic Sign (Back)", "Traffic Sign (Front)",
             "Trash Can", "Bicycle", "Boat", "Bus", "Car", "Caravan", "Motorcycle", "On Rails", "Other Vehicle",
             "Trailer", "Truck", "Wheeled Slow", "Car Mount", "Ego Vehicle", "Unlabeled"]


def project_id(name):
    key = name.lower().strip().replace("_", " ")
    for target, names in RULES:
        for n in names:
            if key == n or (n.endswith("*") and key.startswith(n[:-1])):
                return target
    return KHAC  # trời, cây, cột, biển báo, thùng rác, nón, rào tạm... (Phụ lục B: còn lại -> Khác)


def build_table(id2label):
    table = np.full(256, KHAC, np.uint8)
    for i, name in id2label.items():
        table[int(i)] = project_id(name)
    return table


def show(id2label, table):
    for i, name in sorted(id2label.items(), key=lambda kv: int(kv[0])):
        t = int(table[int(i)])
        print(f"{int(i):3d} {name:32s} -> {t:3d} {NAMES[t]}")


def self_test():
    labels = dict(enumerate(VISTAS_12))
    table = build_table(labels)
    show(labels, table)
    expect = {"Road": 0, "Lane Marking - General": 0, "Lane Marking - Crosswalk": 0, "Curb": 1, "Terrain": 1,
              "Building": 2, "Barrier": 3, "Motorcyclist": 4, "Car": 5, "Bus": 6, "Wheeled Slow": 7,
              "Person": 8, "Bird": 9, "Sky": 10, "Pole": 10, "Ego Vehicle": 255, "Car Mount": 255}
    bad = [n for n, t in expect.items() if table[VISTAS_12.index(n)] != t]
    print("TỰ KIỂM TRA:", "đạt" if not bad else f"SAI {bad}")
    return not bad


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("images", type=Path, nargs="?")
    ap.add_argument("out", type=Path, nargs="?")
    ap.add_argument("--model", default=MODEL, help="id Hugging Face hoặc thư mục có config.json + model.safetensors")
    ap.add_argument("--xem-anh-xa", action="store_true", help="in bảng ánh xạ theo config mô hình rồi thoát")
    ap.add_argument("--tu-kiem", action="store_true", help="kiểm tra bảng ánh xạ với 66 lớp Vistas v1.2, không cần mô hình")
    ap.add_argument("--fp16", action="store_true", help="suy luận nửa độ chính xác trên GPU")
    args = ap.parse_args()
    if args.tu_kiem:
        raise SystemExit(0 if self_test() else 1)
    if not args.images or not args.out:
        ap.error("cần <thư mục ảnh> <thư mục ra>")

    import torch
    from transformers import AutoImageProcessor, Mask2FormerForUniversalSegmentation

    processor = AutoImageProcessor.from_pretrained(args.model)
    model = Mask2FormerForUniversalSegmentation.from_pretrained(args.model, use_safetensors=True)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    dtype = torch.float16 if args.fp16 and device == "cuda" else torch.float32
    model.to(device, dtype=dtype).eval()
    table = build_table(model.config.id2label)
    if args.xem_anh_xa:
        show(model.config.id2label, table)
        return

    args.out.mkdir(parents=True, exist_ok=True)
    files = sorted(p for p in args.images.iterdir() if p.suffix.lower() in {".png", ".jpg", ".jpeg"})
    for n, path in enumerate(files, 1):
        image = Image.open(path).convert("RGB")
        inputs = processor(images=image, return_tensors="pt").to(device)
        inputs["pixel_values"] = inputs["pixel_values"].to(dtype)
        with torch.no_grad():
            outputs = model(**inputs)
        seg = processor.post_process_semantic_segmentation(outputs, target_sizes=[image.size[::-1]])[0]
        mask = table[seg.cpu().numpy().astype(np.uint8)]
        Image.fromarray(mask, mode="L").save(args.out / f"{path.stem}.png")   # trùng tên ảnh
        print(f"[{n}/{len(files)}] {path.name}  lớp: {sorted(np.unique(mask).tolist())}")


if __name__ == "__main__":
    main()
