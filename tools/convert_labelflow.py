"""Chuyển thư mục xuất của LabelFlow sang cấu trúc dữ liệu của dự án (Hướng dẫn v3 §8.5, §11).

Cách dùng:
  python tools/convert_labelflow.py <thư mục xuất LabelFlow> [<thư mục xuất 2> ...] --out <vnroad>
         [--videos videos.csv] [--ignore-dir <vnroad>/ignore] [--chi-duyet] [--ban-kinh-mep 4] [-j 8]

Đầu vào: thư mục "LabelFlow-export-<uuid>" (Export dataset -> định dạng Semantic + region PNGs):
  manifest.json, classes.json, images/<tap>/<media-id>.png,
  masks/cityscapes_semantic/<tap>/<media-id>.png, masks/road_extent/<tap>/<media-id>.png

Đầu ra (<out>/):
  images/<tên ảnh gốc>.png
  masks/  file 2: 0-10 và 255 (chưa tô và Ignore template = 255; lớp cũ bo_qua 11 -> 255)
  road/   file 1: 1 = trong Road Extent, 0 = ngoài, 255 = vùng Bỏ qua cố định của camera
  edge/   cạnh đa giác: 1 nhìn thấy, 2 bị che (xe, người, động vật, Khác), 3 không xác định
  list/train.lst, val.lst (test.lst nếu có): mỗi dòng "images/X.png masks/X.png road/X.png"
  lop.yaml (bản sao), convert_log.csv
Chạy lại vào cùng --out: file của ảnh không còn trong lần xuất này bị xóa khỏi images/ masks/ road/ edge/.

Chọn ảnh: bỏ ảnh unlabeled, rejected và ảnh chưa có mặt nạ semantic; --chi-duyet chỉ lấy approved.
Ảnh trùng tên giữa các thư mục xuất (gán đôi): giữ bản trong thư mục xuất đứng trước.
Chia tập: mặc định lấy cột split trong manifest.json. Có --videos thì lấy cột `tap` của videos.csv
(nên dùng, vì LabelFlow chia ngẫu nhiên theo băm nhóm = thư mục ảnh). Một video ở hai tập -> rò rỉ, dừng.

Tên ảnh phải dạng <video_id>_<giây>.png, ví dụ VID4_0611_00232.0.png -> video VID4_0611.
Không sửa thư mục xuất của LabelFlow. Mã thoát 1 nếu có lỗi chặn.
"""
import argparse
import csv
import json
import os
import shutil
import sys
from functools import lru_cache, partial
from pathlib import Path

import numpy as np

import lf_common as lf

SUBDIRS = ("images", "masks", "road", "edge")


def semantic_lut(classes_json, cfg):
    """Bảng 256 phần tử: ID xuất của LabelFlow -> ID dự án (-1 = giá trị lạ)."""
    names = {c["id"]: c["ma"] for c in cfg["lop"]}
    sem_layer = cfg["labelflow"]["lop_semantic"]
    layer = next((l for l in classes_json["layers"] if l["id"] == sem_layer), None)
    if layer is None:
        raise SystemExit(f"classes.json không có layer {sem_layer}")
    ns = layer["namespace"]
    doi = {int(k): int(v) for k, v in (cfg["labelflow"].get("doi_id") or {}).items()}
    lut = np.full(256, -1, np.int16)
    errors, seen = [], set()
    for c in classes_json["classes"]:
        if c["namespace"] != ns:
            continue
        eid = c["exportId"]
        if c["isIgnore"]:
            lut[eid] = cfg["bo_qua"]
        elif eid in names:
            if c["stableKey"] != names[eid]:
                errors.append(f"ID {eid} là '{c['stableKey']}', cần '{names[eid]}'")
            lut[eid] = eid
            seen.add(eid)
        elif eid in doi:
            lut[eid] = doi[eid]
        else:
            errors.append(f"lớp thừa ID {eid} '{c['stableKey']}' (thêm vào labelflow.doi_id hoặc xóa trong app)")
    missing = sorted(set(names) - seen)
    if missing:
        errors.append(f"thiếu ID {missing}")
    lut[255] = cfg["labelflow"]["chua_to"]
    return lut, errors


def read_videos(path):
    if not path:
        return {}
    with Path(path).open(encoding="utf-8-sig", newline="") as f:
        return {r["video_id"].strip(): r for r in csv.DictReader(f)}


def video_of(stem):
    return stem.rsplit("_", 1)[0]


@lru_cache(maxsize=32)
def camera_mask(path):
    """Mặt nạ Bỏ qua cố định của camera (trắng = bỏ qua); đọc một lần mỗi tiến trình."""
    m = lf.read_png(path)
    return (m if m.ndim == 2 else m[..., 0]) > 0


def link_or_copy(src, dst):
    """Liên kết cứng (nhanh, không tốn chỗ) nếu cùng ổ; không được thì chép."""
    try:
        dst.unlink(missing_ok=True)
        os.link(src, dst)
    except OSError:
        shutil.copyfile(src, dst)


def convert_one(job, cfg, out, radius):
    """Chuyển một ảnh. Trả về (dòng log, dòng .lst hoặc None, có lỗi chặn)."""
    root, img, stem, tap, lut, cam_path = job
    sem_layer, road_layer = cfg["labelflow"]["lop_semantic"], cfg["labelflow"]["lop_long_duong"]
    rel = Path(img["split"]) / f"{img['id']}.png"
    sem_raw = lf.read_png(root / "masks" / sem_layer / rel)
    road_raw = lf.read_png(root / "masks" / road_layer / rel)
    src = root / "images" / rel
    errors = []
    if sem_raw is None or not src.is_file():
        errors.append("thiếu ảnh hoặc mặt nạ semantic trong thư mục xuất")
    elif (sem_raw == 255).all():
        return [stem, tap, "bo", "chưa có mặt nạ semantic (toàn 255)"], None, False
    if road_raw is None:
        errors.append("thiếu mặt nạ road_extent")
    if not errors:
        shape = (img["height"], img["width"])
        if sem_raw.shape != shape or road_raw.shape != shape:
            errors.append(f"kích thước nhãn {sem_raw.shape}/{road_raw.shape} khác ảnh {shape}")
        else:
            mapped = lut[sem_raw]
            if (mapped < 0).any():
                errors.append(f"giá trị semantic lạ {sorted(lf.present_values(sem_raw[mapped < 0]))}")
            if not lf.present_values(road_raw) <= {0, 1}:
                errors.append("road_extent không nhị phân")
    if errors:
        return [stem, tap, "chan", "; ".join(errors)], None, True

    masks = mapped.astype(np.uint8)
    poly, added = lf.snap_to_frame(road_raw == 1, cfg["labelflow"].get("bam_khung_px", 0))
    road = poly.astype(np.uint8)
    notes = [f"bám khung +{added} px"] if added else []
    if cam_path:
        fixed = camera_mask(cam_path)
        if fixed.shape != masks.shape:
            return [stem, tap, "chan", f"mặt nạ camera {Path(cam_path).name} khác kích thước"], None, True
        masks[fixed] = cfg["bo_qua"]
        road[fixed] = cfg["bo_qua"]
        notes.append(f"áp {Path(cam_path).name}")
    edge = lf.edge_map(masks, road == 1, cfg["lop_che_mep"], radius)

    name = f"{stem}.png"
    link_or_copy(src, out / "images" / name)
    lf.write_png(out / "masks" / name, masks)
    lf.write_png(out / "road" / name, road)
    lf.write_png(out / "edge" / name, edge)
    n_edge = int((edge > 0).sum())
    if n_edge:
        notes.append(f"cạnh bị che {(edge == lf.EDGE_BI_CHE).sum() / n_edge:.0%}")
    return [stem, tap, "ok", "; ".join(notes)], f"images/{name} masks/{name} road/{name}", False


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("exports", type=Path, nargs="+")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--lop", type=Path, default=None, help="mặc định tools/lop.yaml")
    ap.add_argument("--videos", type=Path, help="videos.csv (video_id, tap[, mat_na_bo_qua]): quyết định chia tập")
    ap.add_argument("--ignore-dir", type=Path, help="mặt nạ Bỏ qua cố định theo camera: <video_id>.png, trắng = bỏ qua")
    ap.add_argument("--chi-duyet", action="store_true", help="chỉ lấy ảnh approved")
    ap.add_argument("--ban-kinh-mep", type=int, default=4, help="bán kính (px) nhìn vào phía trong cạnh khi sinh edge/")
    ap.add_argument("-j", "--so-tien-trinh", type=int, default=lf.default_workers(), help="số tiến trình song song")
    args = ap.parse_args()

    cfg = lf.load_cfg(args.lop)
    videos = read_videos(args.videos)

    # 1. chọn ảnh
    picked, log, seen = [], [], {}
    for root in args.exports:
        manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
        classes_json = json.loads((root / "classes.json").read_text(encoding="utf-8"))
        lut, errors = semantic_lut(classes_json, cfg)
        if errors:
            raise SystemExit(f"{root}: bộ lớp semantic không khớp lop.yaml:\n  " + "\n  ".join(errors))
        for img in manifest["images"]:
            stem = Path(img["source"].replace("\\", "/")).stem
            status = img["reviewStatus"]
            if status in ("unlabeled", "rejected") or (args.chi_duyet and status != "approved"):
                log.append([stem, "", "bo", f"trạng thái {status}"])
            elif stem in seen:
                log.append([stem, "", "bo", f"trùng tên với ảnh trong {seen[stem]}; giữ bản đầu"])
            else:
                seen[stem] = root.name
                picked.append((root, img, stem, lut))

    # 2. chia tập + kiểm tra rò rỉ trước khi ghi
    jobs, video_splits = [], {}
    for root, img, stem, lut in picked:
        vid = video_of(stem)
        row = videos.get(vid) or {}
        if videos:
            tap = row.get("tap", "").strip()
            if not tap:
                raise SystemExit(f"{stem}: không tìm thấy video {vid} trong {args.videos}")
            if tap != img["split"]:
                log.append([stem, tap, "ghi_chu", f"LabelFlow chia vào {img['split']}, dùng {tap} theo videos.csv"])
        else:
            tap = img["split"]
        video_splits.setdefault(vid, set()).add(tap)
        cam = None
        if args.ignore_dir:
            p = args.ignore_dir / (row.get("mat_na_bo_qua", "").strip() or f"{vid}.png")
            cam = str(p) if p.is_file() else None
        jobs.append((root, img, stem, tap, lut, cam))
    leaks = {v: sorted(s) for v, s in video_splits.items() if len(s) > 1}
    if leaks:
        raise SystemExit("Rò rỉ giữa tập (một video ở nhiều tập): "
                         + "; ".join(f"{v}: {s}" for v, s in leaks.items())
                         + "\nDùng --videos videos.csv, hoặc nhập ảnh mỗi video vào một thư mục con riêng.")

    # 3. chuyển (song song)
    for sub in SUBDIRS + ("list",):
        (args.out / sub).mkdir(parents=True, exist_ok=True)
    work = partial(convert_one, cfg=cfg, out=args.out, radius=args.ban_kinh_mep)
    lists, blocking, written = {"train": [], "val": [], "test": []}, 0, set()
    for (root, img, stem, tap, *_), (row, lst, blocked) in zip(jobs, lf.run_parallel(work, jobs, args.so_tien_trinh)):
        log.append(row)
        blocking += blocked
        if lst:
            lists.setdefault(tap, []).append(lst)
            written.add(f"{stem}.png")

    # 4. dọn file của lần chạy trước không còn trong lần xuất này
    stale = 0
    for sub in SUBDIRS:
        for p in (args.out / sub).glob("*.png"):
            if p.name not in written:
                p.unlink()
                stale += sub == "images"

    for tap, rows in lists.items():
        if rows or tap in ("train", "val"):
            (args.out / "list" / f"{tap}.lst").write_text("".join(r + "\n" for r in sorted(rows)), encoding="utf-8")
    shutil.copyfile(cfg["_path"], args.out / "lop.yaml")
    with (args.out / "convert_log.csv").open("w", encoding="utf-8-sig", newline="") as f:
        w = csv.writer(f)
        w.writerow(["anh", "tap", "ket_qua", "chi_tiet"])
        w.writerows(log)
    counts = {k: sum(1 for r in log if r[2] == k) for k in ("ok", "bo", "chan")}
    print(f"Đã chuyển {counts['ok']} ảnh (train {len(lists['train'])}, val {len(lists['val'])}, "
          f"test {len(lists['test'])}); bỏ {counts['bo']}; lỗi chặn {counts['chan']}"
          + (f"; xóa {stale} ảnh cũ" if stale else "") + f" -> {args.out}")
    for r in log:
        if r[2] == "chan":
            print(f"  [chan] {r[0]}: {r[3]}")
    if lists["test"]:
        print(f"Lưu ý: {len(lists['test'])} ảnh thuộc tập test — không dùng để huấn luyện (Hướng dẫn v3 §3.3).")
    sys.exit(1 if blocking else 0)


if __name__ == "__main__":
    main()
