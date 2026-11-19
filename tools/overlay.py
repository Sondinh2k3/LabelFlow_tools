"""Vẽ nhãn chồng lên ảnh để duyệt nhanh.

Cách dùng:
  python tools/overlay.py <vnroad> [--out <vnroad>/overlay] [--qc <vnroad>/qc_report.csv --chi-loi]
                          [--anh VID4_0611_00232.0.png ...] [--rong 1600] [--do-mo 0.45] [-j 8]

Mỗi ảnh ra một JPG gồm hai khung trên-dưới:
  trên: ảnh gốc + mặt nạ 11 lớp (màu theo lop.yaml) + cạnh Road Extent
        (xanh lơ = cạnh nhìn thấy, vàng = cạnh bị che, đỏ = cạnh không xác định)
  dưới: chỉ ảnh gốc + vùng Bỏ qua (255) NẰM TRONG Road Extent tô hồng đậm (chỗ quên tô / khe hở)
        + vùng Bỏ qua ngoài Road Extent tô xám + cạnh Road Extent
Có --qc thì ghi mức kiểm tra và chi tiết cảnh báo lên đầu ảnh.
Thư mục ra mặc định (<vnroad>/overlay) được xóa JPG cũ trước khi vẽ, để không lẫn ảnh của lần trước.
"""
import argparse
import csv
from functools import partial
from pathlib import Path

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFont

import lf_common as lf

PINK, GREY = (255, 0, 200), (128, 128, 128)
EDGE_COLORS = {lf.EDGE_NHIN_THAY: (0, 230, 255), lf.EDGE_BI_CHE: (255, 230, 0), lf.EDGE_KHONG_RO: (255, 40, 40)}


def font(size):
    for f in ("C:/Windows/Fonts/arial.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


def blend(base, mask, color, alpha):
    """Trộn màu vào các pixel của mask. color: (r, g, b) hoặc ảnh màu cùng kích thước base."""
    color = np.asarray(color, np.uint8)
    if color.ndim == 1:
        color = np.broadcast_to(color, base.shape)
    mixed = cv2.addWeighted(base, 1 - alpha, np.ascontiguousarray(color), alpha, 0)
    return np.where(mask[..., None], mixed, base)


def palette(cfg):
    """Bảng màu 256 x 3 theo ID lớp (một lần tra thay cho trộn từng lớp)."""
    lut = np.zeros((256, 3), np.uint8)
    for c in cfg["lop"]:
        lut[c["id"]] = lf.hex_rgb(c["mau"])
    return lut


def draw_edges(img, edge, poly):
    if edge is None:
        edge = np.where(lf.polygon_boundary(poly), lf.EDGE_NHIN_THAY, 0).astype(np.uint8)
    for value, color in EDGE_COLORS.items():
        m = lf.dilate(edge == value, 2)
        img[m] = color
    return img


def render(name, root, out, cfg, alpha, width, qc):
    draw(root, name, cfg, alpha, width, qc.get(name)).save(out / (Path(name).stem + ".jpg"), quality=88)


def draw(root, name, cfg, alpha, width, qc_row):
    rgb = np.asarray(Image.open(root / "images" / name).convert("RGB"))
    sem = lf.read_png(root / "masks" / name)
    road = lf.read_png(root / "road" / name)
    edge = lf.read_png(root / "edge" / name)
    poly = road == 1
    labeled = sem < len(cfg["lop"])
    top = draw_edges(blend(rgb, labeled, palette(cfg)[sem], alpha), edge, poly)
    ignore = sem == cfg["bo_qua"]
    bottom = blend(rgb, ignore & ~poly, GREY, 0.6)
    bottom = blend(bottom, ignore & poly, PINK, 0.9)
    bottom = draw_edges(bottom, edge, poly)

    scale = width / rgb.shape[1]
    h = int(rgb.shape[0] * scale)
    panels = [Image.fromarray(p).resize((width, h), Image.BILINEAR) for p in (top, bottom)]
    f = font(max(14, width // 80))
    share = (qc_row or {}).get("bo_qua_trong_long_duong")   # cùng số với qc_report (đã bỏ dải sát khung)
    share = float(share) if share else (ignore & poly).sum() / max(poly.sum(), 1)
    header_lines = [f"{name}   Bỏ qua trong Road Extent: {share:.2%}"]
    if qc_row:
        header_lines.append(f"QC: {qc_row['muc']}  {qc_row['chi_tiet']}"[:220])
    legend_h = f.size * 2 + 16
    header_h = (f.size + 6) * len(header_lines) + 8
    canvas = Image.new("RGB", (width, header_h + 2 * h + legend_h), (20, 20, 20))
    d = ImageDraw.Draw(canvas)
    for i, line in enumerate(header_lines):
        color = (255, 120, 120) if i and qc_row and qc_row["muc"] != "dat" else (240, 240, 240)
        d.text((8, 4 + i * (f.size + 6)), line, fill=color, font=f)
    canvas.paste(panels[0], (0, header_h))
    canvas.paste(panels[1], (0, header_h + h))
    x, y = 8, header_h + 2 * h + 6
    items = [(c["mau"], f"{c['id']} {c['ten']}") for c in cfg["lop"]]
    items += [(PINK, "255 trong lòng đường"), (EDGE_COLORS[1], "cạnh thấy"),
              (EDGE_COLORS[2], "cạnh bị che"), (EDGE_COLORS[3], "cạnh không rõ")]
    for color, label in items:
        color = lf.hex_rgb(color) if isinstance(color, str) else color
        tw = d.textlength(label, font=f)
        if x + tw + 30 > width:
            x, y = 8, y + f.size + 4
        d.rectangle([x, y + 2, x + f.size, y + f.size + 2], fill=color, outline=(255, 255, 255))
        d.text((x + f.size + 4, y), label, fill=(240, 240, 240), font=f)
        x += int(tw) + f.size + 18
    return canvas


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("root", type=Path)
    ap.add_argument("--out", type=Path)
    ap.add_argument("--lop", type=Path)
    ap.add_argument("--qc", type=Path, help="qc_report.csv để ghi kết quả kiểm tra lên ảnh")
    ap.add_argument("--chi-loi", action="store_true", help="chỉ vẽ ảnh có cảnh báo hoặc lỗi trong --qc")
    ap.add_argument("--anh", nargs="*", help="chỉ vẽ các ảnh này")
    ap.add_argument("--rong", type=int, default=1600)
    ap.add_argument("--do-mo", type=float, default=0.45)
    ap.add_argument("-j", "--so-tien-trinh", type=int, default=lf.default_workers(), help="số tiến trình song song")
    args = ap.parse_args()
    cfg = lf.load_cfg(args.lop)
    out = args.out or args.root / "overlay"
    out.mkdir(parents=True, exist_ok=True)
    if not args.out and not args.anh:
        for old in out.glob("*.jpg"):
            old.unlink()
    qc = {}
    if args.qc and args.qc.is_file():
        with args.qc.open(encoding="utf-8-sig", newline="") as f:
            qc = {r["anh"]: r for r in csv.DictReader(f)}
    names = sorted(p.name for p in (args.root / "images").glob("*.png"))
    if args.anh:
        names = [n for n in names if n in set(args.anh)]
    if args.chi_loi:
        names = [n for n in names if qc.get(n, {}).get("muc") in ("canh_bao", "chan")]
    lf.run_parallel(partial(render, root=args.root, out=out, cfg=cfg, alpha=args.do_mo, width=args.rong, qc=qc),
                    names, args.so_tien_trinh)
    print(f"Đã vẽ {len(names)} ảnh -> {out}")


if __name__ == "__main__":
    main()
