import csv, numpy as np
def load(f): return list(csv.DictReader(open(f)))
core, shoes = load('core.csv'), load('shoes.csv')
def f(x,k,d=1):
    v=x.get(k,'')
    return '' if v=='' else (f'{float(v):.{d}f}' if d else f'{float(v):.0f}')
def key(x):
    try: return float(x['blk_std_p90'])
    except: return 1e9
def table(rows, name_fn):
    out=['| image | long px | bg % | bg colour | L spread | drift | mid | grain | blk std p50 | blk std p90 | blobs/MP >=8 dark | blobs/MP >=8 light | blobs/MP >=20 | elongated | biggest blob px | note |',
         '|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|']
    for x in sorted(rows,key=key):
        d8=float(x.get('d8_dark_n_per_mp',0) or 0); l8=float(x.get('d8_light_n_per_mp',0) or 0)
        d20=float(x.get('d20_dark_n_per_mp',0) or 0)+float(x.get('d20_light_n_per_mp',0) or 0)
        el=int(float(x.get('d8_dark_n_elongated',0) or 0))+int(float(x.get('d8_light_n_elongated',0) or 0))
        mx=max(int(float(x.get('d8_dark_max_px',0) or 0)),int(float(x.get('d8_light_max_px',0) or 0)))
        out.append(f"| {name_fn(x)} | {max(int(x['w']),int(x['h']))} | {x['bg_pct']} | #{x.get('bg_hex','')} | {f(x,'L_rstd')} | {f(x,'low_std')} | {f(x,'mid_rstd',2)} | {f(x,'grain_rstd',2)} | {f(x,'blk_std_p50',2)} | {f(x,'blk_std_p90',2)} | {d8:.1f} | {l8:.1f} | {d20:.1f} | {el} | {mx} | {x['note']} |")
    return '\n'.join(out)
md=f"""# Background statistics (outside the segmask)

Source: `bgstats.py` in the scratchpad; CSVs: `core.csv` (49 images), `shoes.csv` (13 shoes, 3872 px). Rows sorted flattest first (by block std p90).

## How the numbers are made
* Background = everything outside the BiRefNet mask, after growing the mask by 2% of the long side (drops the halo and contact shadow next to the subject).
* All brightness values are Lab L on a 0-255 scale. Colour spread is the robust spread (MAD x 1.4826), so a few specks do not move it.
* **drift** = std of the smooth background level (vignette, gradient, light falloff).
* **mid** = robust sigma of detail between 2 px and about 3% of the image (fabric, dust, scratches, texture).
* **grain** = robust sigma of detail finer than 2 px (sensor and JPEG noise).
* **blk std** = standard deviation of 16x16 blocks, taken at a common ~800 px scale. p50 = typical, p90 = the busiest tenth.
* **blob** = connected patch of at least 4 px whose brightness differs from the local background by 8 (visible) or 20 (strong) L levels, counted per megapixel of background. **elongated** = long side >= 25 px and aspect >= 4 (scratch or hair shape).
* Blob counts are in native pixels, so a 3872 px shoe and an 800 px image are not directly comparable.

## Shoes (13)
{table(shoes, lambda x:x['image'].replace('.jpg',''))}

## Core set (49)
{table(core, lambda x:x['image'])}
"""
open('bg_report.md','w',encoding='utf-8').write(md)
# quantiles
for k in ['blk_std_p50','blk_std_p90','mid_rstd','grain_rstd','low_std','d8_dark_n_per_mp','d8_light_n_per_mp']:
    for nm,rows in (('core',core),('shoes',shoes)):
        v=np.array([float(x[k]) for x in rows if x.get(k,'')!='' and not x['note']])
        print(nm,k,np.round(np.percentile(v,[0,25,50,75,100]),2))
