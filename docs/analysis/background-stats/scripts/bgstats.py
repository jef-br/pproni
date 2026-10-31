"""Background (outside-the-segmask) statistics: blobs, colour variance, texture.

usage: python -I bgstats.py <images_dir> <masks_dir> <out_csv> <group_label> [overlay_dir]
Masks are <stem>___3-class_10_SEGMASK.png (white = subject).
All L values are OpenCV 8-bit Lab L (0..255).
"""
import sys, os, glob, csv
import numpy as np
import cv2

img_dir, mask_dir, out_csv, group = sys.argv[1:5]
overlay_dir = sys.argv[5] if len(sys.argv) > 5 else None
if overlay_dir:
    os.makedirs(overlay_dir, exist_ok=True)

MARGIN_FRAC = 0.02      # grow the mask by this share of the long side before taking the background
BLOB_MIN_PX = 4
SIZE_BINS = [(4, 24), (25, 100), (101, 1000), (1001, 10**9)]
DELTAS = [8, 20]        # absolute L levels for a "blob"


def mad_sigma(x):
    if x.size == 0:
        return float('nan')
    med = np.median(x)
    return 1.4826 * np.median(np.abs(x - med))


def norm_blur(img, m, sigma):
    """Gaussian blur that only averages over m==1 pixels."""
    num = cv2.GaussianBlur(img * m, (0, 0), sigma)
    den = cv2.GaussianBlur(m, (0, 0), sigma)
    return num / np.maximum(den, 1e-6), den


def analyse(path, mpath):
    bgr = cv2.imread(path, cv2.IMREAD_COLOR)
    h, w = bgr.shape[:2]
    long_side = max(h, w)
    mk = cv2.imread(mpath, cv2.IMREAD_GRAYSCALE)
    if mk.shape != (h, w):
        mk = cv2.resize(mk, (w, h), interpolation=cv2.INTER_NEAREST)
    fg = (mk > 127).astype(np.uint8)
    r = max(1, int(round(MARGIN_FRAC * long_side)))
    grown = cv2.dilate(fg, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (2 * r + 1, 2 * r + 1)))
    bg = (grown == 0)
    row = {'image': os.path.basename(path), 'group': group, 'w': w, 'h': h,
           'mpix': round(w * h / 1e6, 2),
           'fg_pct': round(100 * fg.mean(), 1), 'bg_pct': round(100 * bg.mean(), 1)}
    nbg = int(bg.sum())
    if nbg < 0.03 * w * h:
        row['note'] = 'no usable background (<3%)'
        return row, None
    row['note'] = 'EMPTY MASK (stats are of the whole picture)' if fg.sum() < 0.002 * w * h else ''

    lab = cv2.cvtColor(bgr, cv2.COLOR_BGR2LAB)
    L = lab[..., 0].astype(np.float32)
    A = lab[..., 1].astype(np.float32) - 128
    B = lab[..., 2].astype(np.float32) - 128

    # ---------- colour ----------
    for name, ch in (('L', L), ('a', A), ('b', B)):
        v = ch[bg]
        row[f'{name}_mean'] = round(float(v.mean()), 1)
        row[f'{name}_std'] = round(float(v.std()), 2)
        row[f'{name}_rstd'] = round(mad_sigma(v), 2)
    p = np.percentile(L[bg], [5, 95])
    row['L_p5_p95'] = round(float(p[1] - p[0]), 1)
    b8 = bgr[bg]
    row['bg_hex'] = '%02X%02X%02X' % tuple(int(np.median(b8[:, i])) for i in (2, 1, 0))
    med = np.median(b8, axis=0)
    row['pct_within_8_of_median_rgb'] = round(100 * float((np.abs(b8.astype(np.int16) - med).max(1) <= 8).mean()), 1)

    # ---------- three-band split of L inside the background ----------
    # working-scale (~800 px) smooth reference: fill the subject with the blurred bg, median, upsample
    m = bg.astype(np.float32)
    s = max(1, long_side // 800)
    fill, den = norm_blur(L, m, max(8, long_side * 0.03))
    Lf = np.where(bg, L, fill).astype(np.float32)
    small = cv2.resize(Lf, (max(1, w // s), max(1, h // s)), interpolation=cv2.INTER_AREA)
    k = 25
    small_med = cv2.medianBlur(np.clip(small, 0, 255).astype(np.uint8), k).astype(np.float32)
    ref = cv2.resize(small_med, (w, h), interpolation=cv2.INTER_LINEAR)       # low band
    mid_img = cv2.GaussianBlur(L, (0, 0), 1.5)
    resid = L - ref                                                           # everything above the low band
    mid = mid_img - ref                                                       # 'mid' = blobs / fabric / dust at >=2 px
    hf = L - mid_img                                                          # 'high' = grain
    bge = cv2.erode(bg.astype(np.uint8), np.ones((5, 5), np.uint8)).astype(bool)
    row['low_std'] = round(float(ref[bg].std()), 2)       # drift / vignette / gradient
    row['mid_rstd'] = round(mad_sigma(mid[bge]), 2)
    row['mid_std'] = round(float(mid[bge].std()), 2)
    row['grain_rstd'] = round(mad_sigma(hf[bge]), 2)
    row['grain_std'] = round(float(hf[bge].std()), 2)
    dx = np.abs(np.diff(L, axis=1))
    both = bg[:, 1:] & bg[:, :-1]
    row['mean_abs_hdiff'] = round(float(dx[both].mean()), 2)

    # ---------- texture: block std + gradient at working scale ----------
    sm_L = cv2.resize(L, (max(1, w // s), max(1, h // s)), interpolation=cv2.INTER_AREA)
    sm_bg = cv2.resize(bg.astype(np.uint8), (sm_L.shape[1], sm_L.shape[0]), interpolation=cv2.INTER_NEAREST).astype(bool)
    sm_ref = small_med
    sm_res = sm_L - sm_ref
    bs = 16
    hb, wb = sm_L.shape[0] // bs, sm_L.shape[1] // bs
    stds, grads = [], []
    gx = cv2.Sobel(cv2.GaussianBlur(sm_L, (0, 0), 1.0), cv2.CV_32F, 1, 0, ksize=3) / 8
    gy = cv2.Sobel(cv2.GaussianBlur(sm_L, (0, 0), 1.0), cv2.CV_32F, 0, 1, ksize=3) / 8
    gm = np.hypot(gx, gy)
    for by in range(hb):
        for bx in range(wb):
            sl = (slice(by * bs, (by + 1) * bs), slice(bx * bs, (bx + 1) * bs))
            if sm_bg[sl].all():
                stds.append(sm_res[sl].std())
                grads.append(gm[sl].mean())
    stds = np.array(stds)
    grads = np.array(grads)
    row['blocks'] = int(stds.size)
    if stds.size:
        row['blk_std_p50'] = round(float(np.percentile(stds, 50)), 2)
        row['blk_std_p90'] = round(float(np.percentile(stds, 90)), 2)
        row['blk_std_p99'] = round(float(np.percentile(stds, 99)), 2)
        row['blk_flat_lt1.5_pct'] = round(100 * float((stds < 1.5).mean()), 1)
        row['blk_busy_gt6_pct'] = round(100 * float((stds > 6).mean()), 1)
        row['grad_p50'] = round(float(np.percentile(grads, 50)), 2)
        row['grad_p95'] = round(float(np.percentile(grads, 95)), 2)

    # ---------- blobs ----------
    smooth = cv2.GaussianBlur(resid, (0, 0), 1.0)
    bg_mp = nbg / 1e6
    row['bg_mpix'] = round(bg_mp, 2)
    kern = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (3, 3))
    keep_overlay = None
    for d in DELTAS:
        for pol, sign in (('dark', -1), ('light', 1)):
            mask = ((sign * smooth >= d) & bge).astype(np.uint8)
            n, lab_img, stats, cent = cv2.connectedComponentsWithStats(mask, connectivity=8)
            areas = stats[1:, cv2.CC_STAT_AREA]
            areas = areas[areas >= BLOB_MIN_PX]
            tag = f'd{d}_{pol}'
            row[f'{tag}_n_per_mp'] = round(len(areas) / bg_mp, 1)
            row[f'{tag}_area_pct'] = round(100 * float(areas.sum()) / nbg, 3) if len(areas) else 0.0
            row[f'{tag}_max_px'] = int(areas.max()) if len(areas) else 0
            row[f'{tag}_med_px'] = int(np.median(areas)) if len(areas) else 0
            if d == DELTAS[0]:
                for lo, hi in SIZE_BINS:
                    row[f'{tag}_n_{lo}-{hi if hi < 10**8 else "inf"}'] = int(((areas >= lo) & (areas <= hi)).sum())
                # elongated (scratch / hair / fibre): long side >= 25 px and aspect >= 4, via min-area-rect
                idx = [i for i in range(1, n) if stats[i, cv2.CC_STAT_AREA] >= 20]
                idx.sort(key=lambda i: -stats[i, cv2.CC_STAT_AREA])
                idx = idx[:3000]
                elong = 0
                for i in idx:
                    x, y, ww, hh = stats[i, 0], stats[i, 1], stats[i, 2], stats[i, 3]
                    sub = (lab_img[y:y + hh, x:x + ww] == i).astype(np.uint8)
                    cs, _ = cv2.findContours(sub, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
                    if not cs:
                        continue
                    (_, _), (rw, rh), _ = cv2.minAreaRect(max(cs, key=cv2.contourArea))
                    lo_, hi_ = min(rw, rh), max(rw, rh)
                    if hi_ >= 25 and hi_ / max(lo_, 1) >= 4:
                        elong += 1
                row[f'{tag}_n_elongated'] = elong
    # self-normalising blob count: 5 x the robust mid-band sigma
    thr = max(2.0, 5 * row['mid_rstd'])
    row['blob_thr_5sigma'] = round(thr, 1)
    msk = ((np.abs(smooth) >= thr) & bge).astype(np.uint8)
    n, _, stats, _ = cv2.connectedComponentsWithStats(msk, connectivity=8)
    areas = stats[1:, cv2.CC_STAT_AREA]
    areas = areas[areas >= BLOB_MIN_PX]
    row['sig5_n_per_mp'] = round(len(areas) / bg_mp, 1)
    row['sig5_area_pct'] = round(100 * float(areas.sum()) / nbg, 3) if len(areas) else 0.0

    # overlay for eyeballing: subject greyed, dark blobs red, light blobs cyan (d=DELTAS[0])
    if overlay_dir:
        ov = bgr.copy()
        ov[~bg] = (ov[~bg] * 0.25 + 128 * 0.75).astype(np.uint8)
        dk = ((-smooth >= DELTAS[0]) & bge)
        lt = ((smooth >= DELTAS[0]) & bge)
        ov[dk] = (0, 0, 255)
        ov[lt] = (255, 255, 0)
        sc = 900 / long_side
        ov = cv2.resize(ov, (int(w * sc), int(h * sc)), interpolation=cv2.INTER_AREA if sc < 1 else cv2.INTER_NEAREST)
        keep_overlay = ov
        cv2.imwrite(os.path.join(overlay_dir, os.path.splitext(os.path.basename(path))[0] + '.jpg'), ov,
                    [cv2.IMWRITE_JPEG_QUALITY, 85])
    return row, keep_overlay


rows = []
paths = [p for p in sorted(glob.glob(os.path.join(img_dir, '*'))) if p.lower().endswith(('.jpg', '.jpeg', '.png'))]
for p in paths:
    stem = os.path.splitext(os.path.basename(p))[0]
    mp = os.path.join(mask_dir, stem + '___3-class_10_SEGMASK.png')
    if not os.path.exists(mp):
        print('no mask for', stem)
        continue
    try:
        row, _ = analyse(p, mp)
    except Exception as e:  # report, do not hide
        print('FAILED', stem, repr(e))
        continue
    rows.append(row)
    print(stem, row.get('bg_pct'), row.get('note', ''))

keys = []
for r in rows:
    for k in r:
        if k not in keys:
            keys.append(k)
with open(out_csv, 'w', newline='') as f:
    wr = csv.DictWriter(f, fieldnames=keys)
    wr.writeheader()
    wr.writerows(rows)
print('wrote', out_csv, len(rows), 'rows')
