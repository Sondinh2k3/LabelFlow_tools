"""Phần dùng chung cho các script trong tools/ (không chạy trực tiếp).

Mọi ID lớp, màu, tên layer và ngưỡng đọc từ lop.yaml (cùng thư mục), không viết cứng trong script.
"""
import os
import sys
from concurrent.futures import ProcessPoolExecutor
from functools import lru_cache
from pathlib import Path

import cv2
import numpy as np
import yaml
from PIL import Image

for _stream in (sys.stdout, sys.stderr):  # console Windows mặc định cp1252, không in được tiếng Việt
    try:
        _stream.reconfigure(encoding="utf-8")
    except (AttributeError, ValueError):
        pass

HERE = Path(__file__).resolve().parent
MAT_DUONG, VIA_HE, NHA = 0, 1, 2
KHAC = 10
# Giá trị trong edge/: 0 không phải cạnh; 1 cạnh nhìn thấy (phía trong là Mặt đường);
# 2 cạnh bị che (phía trong là xe, người, động vật, Khác); 3 cạnh không xác định
# (phía trong là Vỉa hè, Nhà, Kết cấu hoặc Bỏ qua: thường là đa giác lệch mép).
EDGE_NHIN_THAY, EDGE_BI_CHE, EDGE_KHONG_RO = 1, 2, 3
KERNEL3 = np.ones((3, 3), np.uint8)


# ---------- cấu hình, đọc ghi, chạy song song ----------

def load_cfg(path=None):
    path = Path(path) if path else HERE / "lop.yaml"
    cfg = yaml.safe_load(path.read_text(encoding="utf-8"))
    cfg["_path"] = str(path)
    ids = [c["id"] for c in cfg["lop"]]
    if ids != list(range(len(ids))):
        raise SystemExit(f"{path}: lop phải có id liên tục từ 0, đang là {ids}")
    return cfg


def valid_semantic(cfg):
    return set(range(len(cfg["lop"]))) | {cfg["bo_qua"]}


def exempt_ids(cfg):
    return [c["id"] for c in cfg["lop"] if c["mien"]]


def hex_rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def read_png(path):
    path = Path(path)
    if not path.is_file():
        return None
    with Image.open(path) as im:
        return np.array(im)


def write_png(path, array):
    Image.fromarray(np.ascontiguousarray(array, dtype=np.uint8)).save(path)


def present_values(array):
    """Các giá trị có mặt trong mảng uint8 (bincount nhanh hơn np.unique nhiều lần)."""
    return set(np.flatnonzero(np.bincount(array.ravel(), minlength=256)).tolist())


def default_workers():
    return max(1, min(8, (os.cpu_count() or 2) - 1))


def run_parallel(fn, items, workers):
    """Như map(fn, items), giữ thứ tự. Ít ảnh thì chạy tuần tự (khởi động tiến trình trên Windows mất ~0,5 s)."""
    items = list(items)
    if workers <= 1 or len(items) < 8:
        return [fn(x) for x in items]
    workers = min(workers, len(items))
    with ProcessPoolExecutor(max_workers=workers) as ex:
        return list(ex.map(fn, items, chunksize=max(1, len(items) // (workers * 4))))


# ---------- hình học ----------

@lru_cache(maxsize=8)
def disk(r):
    return cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * r + 1, 2 * r + 1))


def dilate(mask, r):
    return cv2.dilate(mask.astype(np.uint8), disk(r)) > 0


def bbox(mask, pad, shape):
    """Khung chứa mask, nới pad px (cắt vùng xử lý cho nhanh). None nếu mask rỗng."""
    ys, xs = np.flatnonzero(mask.any(axis=1)), np.flatnonzero(mask.any(axis=0))
    if ys.size == 0:
        return None
    return (slice(max(0, ys[0] - pad), min(shape[0], ys[-1] + pad + 1)),
            slice(max(0, xs[0] - pad), min(shape[1], xs[-1] + pad + 1)))


def inner_edge(poly):
    """Pixel cạnh trong của vùng (gồm cả phần trùng khung ảnh)."""
    p = poly.astype(np.uint8)
    return (p == 1) & (cv2.erode(p, KERNEL3, borderType=cv2.BORDER_REPLICATE) == 0)


def polygon_boundary(poly):
    """Pixel cạnh trong của vùng lòng đường, bỏ phần trùng khung ảnh (không phải mép đường)."""
    boundary = inner_edge(poly)
    boundary[:2, :] = boundary[-2:, :] = False
    boundary[:, :2] = boundary[:, -2:] = False
    return boundary


def edge_map(sem, poly, occluders, radius=4):
    """edge/: phân loại từng pixel cạnh đa giác theo cái nhìn thấy ngay phía trong (bán kính radius)."""
    out = np.zeros(sem.shape, np.uint8)
    box = bbox(poly, radius + 2, sem.shape)
    if box is None:
        return out
    s, p = sem[box], poly[box]
    boundary = polygon_boundary(poly)[box]
    road_near = dilate((s == MAT_DUONG) & p, radius)
    occ_near = dilate(np.isin(s, occluders) & p, radius)
    o = out[box]
    o[boundary & road_near] = EDGE_NHIN_THAY
    o[boundary & ~road_near & occ_near] = EDGE_BI_CHE
    o[boundary & ~road_near & ~occ_near] = EDGE_KHONG_RO
    return out


def components(mask):
    """Số vùng liên thông (8 hướng) và bảng thống kê (x, y, w, h, diện tích) của từng vùng."""
    n, _, stats, _ = cv2.connectedComponentsWithStats(mask.astype(np.uint8), connectivity=8)
    return n - 1, stats[1:]


def row_extent(poly):
    """Các hàng có Road Extent và x trái/phải của vùng trên từng hàng đó."""
    rows = np.flatnonzero(poly.any(axis=1))
    sub = poly[rows]
    left = np.argmax(sub, axis=1)
    right = poly.shape[1] - 1 - np.argmax(sub[:, ::-1], axis=1)
    return rows, left, right


def snap_to_frame(poly, t):
    """Kéo Road Extent ra sát khung ảnh ở chỗ nó dừng cách khung <= t px (đỉnh đa giác click hơi lọt vào
    trong ảnh). Nếu không, dải hở sát khung bị coi là mép đường trong edge/ và khi đo khớp mép.
    Trả về (vùng mới, số pixel thêm)."""
    if t <= 0 or not poly.any():
        return poly, 0
    out = poly.copy()
    for p in (out, out.T):                          # theo hàng (trái, phải) rồi theo cột (trên, dưới)
        n = p.shape[1]
        rows, first, last = row_extent(p)
        cols = np.arange(n)[None, :]
        near_first, near_last = first <= t, last >= n - 1 - t
        p[rows[near_first]] |= cols < first[near_first, None]
        p[rows[near_last]] |= cols > last[near_last, None]
    return out, int(out.sum() - poly.sum())


def run_lengths(mask):
    """Độ dài đoạn liên tiếp theo hàng ngang chứa mỗi pixel True (0 ở pixel False)."""
    h, w = mask.shape
    pad = np.zeros((h, w + 2), np.int8)
    pad[:, 1:-1] = mask
    d = np.diff(pad, axis=1).ravel()
    lengths = np.flatnonzero(d == -1) - np.flatnonzero(d == 1)
    out = np.zeros(h * w, np.int32)
    out[mask.ravel()] = np.repeat(lengths, lengths)
    return out.reshape(h, w)


def longest_run(flags):
    """(độ dài, vị trí bắt đầu) của đoạn True liên tiếp dài nhất; đoạn đầu tiên nếu bằng nhau."""
    best, best_start, run = 0, None, 0
    for i, f in enumerate(flags):
        run = run + 1 if f else 0
        if run > best:
            best, best_start = run, i - run + 1
    return best, best_start


# ---------- kiểm tra chất lượng ----------

def edge_mismatch(sem, poly, tol, ignore=255, max_seam=3, wide_seam=80):
    """Khớp mép (Hướng dẫn v3 §6.2). Trên mỗi hàng của Road Extent, tìm biên nhìn thấy
    Vỉa hè/Nhà -> Mặt đường ở nửa trái (Mặt đường -> Vỉa hè/Nhà ở nửa phải): hai lớp kề nhau hoặc
    cách nhau khe chưa tô (255) <= max_seam px. Đo khoảng cách VUÔNG GÓC (distance transform) từ biên đó
    tới cạnh Road Extent (đo theo hàng ngang phóng đại nhiều lần ở cạnh xiên thoải). Một hàng có nhiều
    biên (miệng ngõ...) thì lấy biên gần cạnh cùng bên nhất. Bỏ phía mà vùng chạm khung ảnh.
    Hàng có khe 255 rộng hơn (<= wide_seam px) không đo được mép: đếm riêng là "mép có khe".
    Tính cho mọi hàng cùng lúc (không lặp theo hàng). Trả về dict: checked, bad, median, seam_rows, rows."""
    rows, left, right = row_extent(poly)
    result = {"checked": 0, "bad": 0, "median": 0.0, "seam_rows": 0, "rows": int(rows.size)}
    if rows.size == 0:
        return result
    W = sem.shape[1]
    sub = sem[rows].ravel()
    idx = np.flatnonzero(sub != ignore)                    # pixel có nhãn, theo thứ tự hàng rồi cột
    vals, r = sub[idx], idx // W
    # chỉ giữ cặp pixel có nhãn liên tiếp, cùng hàng, KHÁC lớp (biên chỉ có ở đó) -> mảng nhỏ
    pair = np.flatnonzero((vals[1:] != vals[:-1]) & (r[1:] == r[:-1]))
    rr, c0, c1 = r[pair], idx[pair] - r[pair] * W, idx[pair + 1] - r[pair] * W
    a, b = vals[pair], vals[pair + 1]
    gap = c1 - c0 - 1                                      # số pixel 255 giữa hai pixel đó
    mid = (c0 + c1 + 1) / 2                                # vị trí biên = giữa khe
    middle = (left[rr] + right[rr]) / 2
    side_a, side_b = (a == VIA_HE) | (a == NHA), (b == VIA_HE) | (b == NHA)
    to_edge = cv2.distanceTransform((~inner_edge(poly)).astype(np.uint8), cv2.DIST_L2, 5)
    dists, seam = [], 0
    for trans, target in ((side_a & (b == MAT_DUONG) & (mid < middle) & (left[rr] > 1), left[rr]),
                          ((a == MAT_DUONG) & side_b & (mid >= middle) & (right[rr] < W - 2), right[rr])):
        narrow = trans & (gap <= max_seam)
        xs = np.clip(np.rint(mid[narrow]).astype(int), 0, W - 1)
        ys = rr[narrow]
        order = np.lexsort((np.abs(xs - target[narrow]), ys))   # theo hàng, rồi biên gần cạnh nhất
        pick = order[np.unique(ys[order], return_index=True)[1]]
        dists.append(to_edge[rows[ys[pick]], xs[pick]].astype(np.float64))
        seam += np.setdiff1d(rr[trans & (gap <= wide_seam)], ys).size
    d = np.concatenate(dists)
    result.update(checked=int(d.size), bad=int((d > tol).sum()),
                  median=float(np.median(d)) if d.size else 0.0, seam_rows=int(seam))
    return result


def khac_strip(sem, poly, max_width_share):
    """Dải Khác mảnh sát cạnh Road Extent, chạy dài theo mép (dấu hiệu gán nhầm lề, rãnh thành Khác).
    Một hàng "có dải" khi pixel Khác đầu tiên trong cửa sổ ±3 px quanh cạnh thuộc đoạn Khác ngang
    không rộng quá max_width_share bề rộng vùng trên hàng đó (tối thiểu 2 px).
    Trả về (số hàng liên tiếp dài nhất có dải, số hàng của vùng đường, hàng bắt đầu, bên)."""
    rows, left, right = row_extent(poly)
    if rows.size == 0:
        return 0, 0, None, None
    W = sem.shape[1]
    khac = sem == KHAC
    runs = run_lengths(khac)
    limit = np.maximum(2, (max_width_share * (right - left + 1)).astype(int))
    offsets = np.arange(-3, 4)
    best = (0, None, None)
    for side, edge in (("trai", left), ("phai", right)):
        xs = edge[:, None] + offsets
        valid = (xs >= 0) & (xs < W)
        xs = np.clip(xs, 0, W - 1)
        hit_win = khac[rows[:, None], xs] & valid
        x0 = xs[np.arange(rows.size), np.argmax(hit_win, axis=1)]
        hit = hit_win.any(axis=1) & (runs[rows, x0] <= limit)
        run, start = longest_run(hit.tolist())
        if run > best[0]:
            best = (run, int(rows[start]), side)
    return best[0], int(rows.size), best[1], best[2]
