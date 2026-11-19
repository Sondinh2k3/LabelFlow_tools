"""Kiểm tra nhãn sau khi chuyển đổi (Hướng dẫn v3 §10.3), xuất qc_report.csv.

Cách dùng:
  python tools/check_labels.py <vnroad> [--lop tools/lop.yaml] [--report <vnroad>/qc_report.csv] [-j 8]

Đọc <vnroad>/images, masks, road (và list/*.lst để biết tập). Ngưỡng lấy từ lop.yaml (nguong_qc).

| Kiểm tra                                                              | Mức      |
| masks chỉ 0-10, 255; road chỉ 0, 1, 255                               | chặn     |
| Đúng một vùng Road Extent; có pixel Mặt đường                         | chặn     |
| Ảnh và hai nhãn cùng kích thước                                       | chặn     |
| Bỏ qua (255) nằm trong Road Extent > 1%  (quên tô, khe hở)            | cảnh báo |
| Mặt đường nằm trong Road Extent < 95%                                 | cảnh báo |
| Khớp mép: lệch vuông góc > 3 px ở > 10% số hàng                       | cảnh báo |
| Mép có khe 255 > 3 px giữa Mặt đường và Vỉa hè/Nhà ở > 10% số hàng    | cảnh báo |
| Dải Khác mảnh sát cạnh Road Extent, dài > 1/3 chiều cao đường         | cảnh báo |
| Số ảnh có mặt của mỗi lớp được miễn (và Khác) < 30                    | thống kê |
Mã thoát 1 nếu có lỗi chặn.
"""
import argparse
import csv
import sys
from functools import partial
from pathlib import Path

import numpy as np
from PIL import Image

import lf_common as lf

COLUMNS = ["anh", "tap", "muc", "bo_qua_trong_long_duong", "vung_bo_qua_lon_nhat_xywh",
           "mat_duong_trong_long_duong", "lech_mep_ty_le_hang", "lech_mep_trung_vi_px", "mep_co_khe_hang",
           "dai_khac_ty_le_chieu_cao", "so_vung_long_duong", "chi_tiet"]


def split_map(root):
    out = {}
    for lst in (root / "list").glob("*.lst"):
        for line in lst.read_text(encoding="utf-8").splitlines():
            if line.strip():
                out[Path(line.split()[0]).name] = lst.stem
    return out


def check_one(name, root, cfg):
    """Trả về (lỗi chặn, cảnh báo, các cột số liệu, tập giá trị lớp có mặt)."""
    q = cfg["nguong_qc"]
    errors, warnings, row = [], [], {}
    sem, road = lf.read_png(root / "masks" / name), lf.read_png(root / "road" / name)
    if sem is None or road is None:
        return ["thiếu masks/ hoặc road/"], [], row, set()
    with Image.open(root / "images" / name) as im:
        size = (im.height, im.width)
    if sem.shape != size or road.shape != size:
        return [f"kích thước khác nhau: ảnh {size}, masks {sem.shape}, road {road.shape}"], [], row, set()
    present = lf.present_values(sem)
    bad_sem = present - lf.valid_semantic(cfg)
    if bad_sem:
        errors.append(f"masks có giá trị lạ {sorted(bad_sem)}")
    bad_road = lf.present_values(road) - {0, 1, 255}
    if bad_road:
        errors.append(f"road có giá trị lạ {sorted(bad_road)}")
    poly = road == 1
    n_poly, _ = lf.components(poly)
    row["so_vung_long_duong"] = n_poly
    if n_poly != 1:
        errors.append(f"có {n_poly} vùng Road Extent (cần đúng 1)")
    if lf.MAT_DUONG not in present:
        errors.append("không có pixel Mặt đường")
    if errors:
        return errors, warnings, row, set()

    ignore_in = (sem == cfg["bo_qua"]) & poly
    t = cfg["labelflow"].get("bam_khung_px", 0)
    if t > 0:   # dải hở sát khung ảnh không phải vật quên tô
        ignore_in[:t] = ignore_in[-t:] = False
        ignore_in[:, :t] = ignore_in[:, -t:] = False
    share = ignore_in.sum() / poly.sum()
    row["bo_qua_trong_long_duong"] = f"{share:.4f}"
    n_blob, stats = lf.components(ignore_in)
    if n_blob:
        x, y, w, h, _ = stats[np.argmax(stats[:, 4])]
        row["vung_bo_qua_lon_nhat_xywh"] = f"{x},{y},{w},{h}"
    if share > q["bo_qua_trong_long_duong"]:
        warnings.append(f"Bỏ qua chiếm {share:.1%} Road Extent (quên tô vật/khe hở?), vùng lớn nhất x,y,w,h="
                        f"{row['vung_bo_qua_lon_nhat_xywh']}")

    road_px = sem == lf.MAT_DUONG
    inside = (road_px & poly).sum() / road_px.sum()
    row["mat_duong_trong_long_duong"] = f"{inside:.4f}"
    if inside < q["mat_duong_trong_long_duong"]:
        warnings.append(f"chỉ {inside:.1%} Mặt đường nằm trong Road Extent")

    m = lf.edge_mismatch(sem, poly, q["lech_mep_px"], cfg["bo_qua"])
    if m["checked"]:
        ratio = m["bad"] / m["checked"]
        row["lech_mep_ty_le_hang"] = f"{ratio:.4f}"
        row["lech_mep_trung_vi_px"] = f"{m['median']:.1f}"
        if ratio > q["lech_mep_ty_le_hang"]:
            warnings.append(f"lệch mép > {q['lech_mep_px']} px ở {ratio:.0%} số hàng có mép nhìn thấy "
                            f"({m['checked']} lượt đo, trung vị {m['median']:.0f} px)")
    else:
        row["lech_mep_ty_le_hang"] = "khong_co_mep"
    row["mep_co_khe_hang"] = m["seam_rows"]
    if m["seam_rows"] > q["lech_mep_ty_le_hang"] * m["rows"]:
        warnings.append(f"{m['seam_rows']} hàng có khe chưa tô (255) giữa Mặt đường và Vỉa hè/Nhà: "
                        "không đo được khớp mép, tô kín khe")

    run, height, start, side = lf.khac_strip(sem, poly, q["dai_khac_be_rong"])
    row["dai_khac_ty_le_chieu_cao"] = f"{run / height:.4f}" if height else ""
    if height and run / height > q["dai_khac_chieu_dai"]:
        warnings.append(f"dải Khác mảnh dọc mép {side} dài {run / height:.0%} chiều cao đường từ hàng {start} "
                        "(lề, rãnh gán nhầm Khác?)")
    return errors, warnings, row, present


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("root", type=Path)
    ap.add_argument("--lop", type=Path, default=None)
    ap.add_argument("--report", type=Path, default=None)
    ap.add_argument("-j", "--so-tien-trinh", type=int, default=lf.default_workers(), help="số tiến trình song song")
    args = ap.parse_args()
    cfg = lf.load_cfg(args.lop)
    q = cfg["nguong_qc"]
    splits = split_map(args.root)
    report = args.report or args.root / "qc_report.csv"

    names = sorted(p.name for p in (args.root / "images").glob("*.png"))
    results = lf.run_parallel(partial(check_one, root=args.root, cfg=cfg), names, args.so_tien_trinh)
    rows, blocking, class_images = [], 0, {c["id"]: 0 for c in cfg["lop"]}
    for name, (errors, warnings, row, present) in zip(names, results):
        blocking += bool(errors)
        for v in present & class_images.keys():
            class_images[v] += 1
        row.update(anh=name, tap=splits.get(name, ""), muc="chan" if errors else "canh_bao" if warnings else "dat",
                   chi_tiet=" | ".join(errors + warnings))
        rows.append(row)

    need = q["so_anh_toi_thieu_moi_lop"]
    for c in cfg["lop"]:
        if c["mien"] or c["id"] == lf.KHAC:
            n = class_images[c["id"]]
            rows.append({"anh": f"(thong ke) {c['id']} {c['ma']}", "muc": "thong_ke" if n >= need else "thong_ke_thieu",
                         "chi_tiet": f"có mặt trong {n} ảnh" + ("" if n >= need else f", dưới {need}: bổ sung ở vòng sau")})
    with report.open("w", encoding="utf-8-sig", newline="") as f:
        w = csv.DictWriter(f, fieldnames=COLUMNS)
        w.writeheader()
        w.writerows(rows)
    counts = {m: sum(1 for r in rows if r["muc"] == m) for m in ("dat", "canh_bao", "chan")}
    print(f"{len(names)} ảnh: đạt {counts['dat']}, cảnh báo {counts['canh_bao']}, chặn {counts['chan']} -> {report}")
    for r in rows:
        if r["muc"] in ("chan", "canh_bao"):
            print(f"  [{r['muc']}] {r['anh']}: {r['chi_tiet']}")
    sys.exit(1 if blocking else 0)


if __name__ == "__main__":
    main()
