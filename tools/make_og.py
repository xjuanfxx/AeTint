# python tools/make_og.py  ->  docs/og-card.png
# Tarjeta para compartir (og:image) de 1200x630 en colores de Catppuccin Mocha. Se dibuja al doble y se reduce.
import numpy as np
import os
from PIL import Image, ImageDraw, ImageFont, ImageFilter

S = 2
W, H = 1200 * S, 630 * S
DOCS = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'docs')
F = os.path.join(DOCS, 'fonts')
OUT = os.path.join(DOCS, 'og-card.png')

crust = (17, 17, 27); text = (205, 214, 244); sub = (166, 173, 200); overlay = (127, 132, 156)
mauve = (203, 166, 247); lavender = (180, 190, 254); pink = (245, 194, 231); green = (166, 227, 161)

yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
img = np.zeros((H, W, 3), np.float32) + np.array(crust, np.float32)


def glow(cx, cy, rx, ry, col, a):
    d = np.sqrt(((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2)
    k = np.clip(1 - d, 0, 1) ** 2 * a
    return k[..., None] * (np.array(col, np.float32) - img)


img += glow(-40 * S, -20 * S, 560 * S, 460 * S, mauve, 0.42)
img += glow(1220 * S, 700 * S, 620 * S, 480 * S, lavender, 0.30)

# cuadrícula tenue que se desvanece desde arriba a la izquierda
grid = ((xx % (48 * S) < S) | (yy % (48 * S) < S)).astype(np.float32)
fade = np.clip(1 - np.sqrt(((xx - 260 * S) / (760 * S)) ** 2 + ((yy - 180 * S) / (520 * S)) ** 2), 0, 1)
img += (grid * fade * 0.06)[..., None] * (np.array(text, np.float32) - img)

# tramado fino para que los degradados no hagan bandas
img += (np.random.default_rng(1).random((H, W, 1)) - 0.5) * 2.0
base = Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), 'RGB').convert('RGBA')

# ── captura de la app en un marco de vidrio, en perspectiva y saliendo por la derecha
shot = Image.open(os.path.join(DOCS, 'app-dark.png')).convert('RGBA')
fw = 900 * S
fh = int(fw * shot.height / shot.width)
pad = 10 * S
frame = Image.new('RGBA', (fw + 2 * pad, fh + 2 * pad), (0, 0, 0, 0))
fd = ImageDraw.Draw(frame)
fd.rounded_rectangle([0, 0, frame.width - 1, frame.height - 1], 24 * S, fill=(205, 214, 244, 26), outline=(205, 214, 244, 46), width=S)
sh = shot.resize((fw, fh), Image.LANCZOS)
m = Image.new('L', sh.size, 0)
ImageDraw.Draw(m).rounded_rectangle([0, 0, fw - 1, fh - 1], 15 * S, fill=255)
frame.paste(sh, (pad, pad), m)


def coeffs(src, dst):
    # coeficientes de Image.transform(PERSPECTIVE): de cada punto de destino al de origen
    A, B = [], []
    for (x, y), (u, v) in zip(dst, src):
        A += [[x, y, 1, 0, 0, 0, -u * x, -u * y], [0, 0, 0, x, y, 1, -v * x, -v * y]]
        B += [u, v]
    return np.linalg.solve(np.array(A, float), np.array(B, float)).tolist()


w0, h0 = frame.size
# lado izquierdo más alto que el derecho: gira hacia la derecha
dst = [(0, 0), (w0 * 0.86, h0 * 0.07), (w0 * 0.86, h0 * 0.93), (0, h0)]
out_w, out_h = int(w0 * 0.86) + 2, h0
persp = frame.transform((out_w, out_h), Image.PERSPECTIVE, coeffs([(0, 0), (w0, 0), (w0, h0), (0, h0)], dst), Image.BICUBIC)

ox, oy = 655 * S, 96 * S
# sombra y luz violeta detrás del marco
shadow = Image.new('RGBA', base.size, (0, 0, 0, 0))
sd = ImageDraw.Draw(shadow)
sd.rounded_rectangle([ox + 30 * S, oy + 60 * S, ox + out_w, oy + out_h + 40 * S], 30 * S, fill=(0, 0, 0, 200))
shadow = shadow.filter(ImageFilter.GaussianBlur(40 * S))
base = Image.alpha_composite(base, shadow)
halo = Image.new('RGBA', base.size, (0, 0, 0, 0))
ImageDraw.Draw(halo).ellipse([ox - 120 * S, oy + 40 * S, ox + 260 * S, oy + out_h], fill=mauve + (90,))
halo = halo.filter(ImageFilter.GaussianBlur(70 * S))
base = Image.alpha_composite(base, halo)
base.alpha_composite(persp, (ox, oy))

# ── textos
d = ImageDraw.Draw(base)
f_brand = ImageFont.truetype(os.path.join(F, 'ArchivoExpanded-ExtraBold.ttf'), 30 * S)
f_h1 = ImageFont.truetype(os.path.join(F, 'ArchivoExpanded-ExtraBold.ttf'), 60 * S)
f_body = ImageFont.truetype(os.path.join(F, 'Inter-Medium.ttf'), 22 * S)
f_pill = ImageFont.truetype(os.path.join(F, 'Inter-Medium.ttf'), 17 * S)
f_mono = ImageFont.truetype(os.path.join(F, 'JetBrainsMono-Medium.ttf'), 15 * S)
X = 72 * S

# logo de la app (mismo dibujo que en la web), con su resplandor
L = 52 * S
lg = Image.new('RGBA', (L, L), (0, 0, 0, 0))
g = np.zeros((L, L, 4), np.float32)
t = (np.mgrid[0:L, 0:L].sum(0) / (2 * L)).astype(np.float32)
c0, c1, c2 = np.array((226, 207, 251), np.float32), np.array(mauve, np.float32), np.array((125, 90, 168), np.float32)
col = np.where(t[..., None] < 0.5, c0 + (c1 - c0) * (t[..., None] * 2), c1 + (c2 - c1) * ((t[..., None] - 0.5) * 2))
g[..., :3] = col
g[..., 3] = 255
lg = Image.fromarray(g.astype(np.uint8), 'RGBA')
lm = Image.new('L', (L, L), 0)
ImageDraw.Draw(lm).rounded_rectangle([0, 0, L - 1, L - 1], int(L * 0.28), fill=255)
lg.putalpha(lm)
ld = ImageDraw.Draw(lg)
u = L / 32
ld.ellipse([8.5 * u, 10.8 * u, 23.5 * u, 25.8 * u], fill='white')
ld.polygon([(16 * u, 4.5 * u), (8.9 * u, 16.5 * u), (23.1 * u, 16.5 * u)], fill='white')
for bx, by, bw in [(11, 15.6, 7.5), (13, 18.6, 8), (11.5, 21.6, 5.5)]:
    ld.rounded_rectangle([bx * u, by * u, (bx + bw) * u, (by + 1.9) * u], u, fill=crust)
ld.rectangle([17.4 * u, 12.6 * u, 18.6 * u, 24.4 * u], fill=(178, 143, 224))
ld.rounded_rectangle([16.5 * u, 11.4 * u, 19.5 * u, 13.6 * u], 0.7 * u, fill=(178, 143, 224))
gl = Image.new('RGBA', base.size, (0, 0, 0, 0))
ImageDraw.Draw(gl).rounded_rectangle([X, 66 * S, X + L, 66 * S + L], 14 * S, fill=mauve + (150,))
gl = gl.filter(ImageFilter.GaussianBlur(18 * S))
base = Image.alpha_composite(base, gl)
base.alpha_composite(lg, (X, 62 * S))
d = ImageDraw.Draw(base)
d.text((X + L + 16 * S, 62 * S + L // 2), 'AeTint', font=f_brand, fill=text, anchor='lm')

# píldora con la versión
py = 150 * S
tag = 'v0.4.2'
tw = d.textlength(tag, font=f_mono)
label = 'Free & open source'
lw = d.textlength(label, font=f_pill)
pw = int(6 * S + tw + 22 * S + 12 * S + lw + 16 * S)
glass = Image.new('RGBA', base.size, (0, 0, 0, 0))
ImageDraw.Draw(glass).rounded_rectangle([X, py, X + pw, py + 38 * S], 19 * S, fill=(205, 214, 244, 14), outline=(205, 214, 244, 40), width=S)
base = Image.alpha_composite(base, glass)
d = ImageDraw.Draw(base)
d.rounded_rectangle([X + 6 * S, py + 6 * S, X + 6 * S + tw + 22 * S, py + 32 * S], 13 * S, fill=mauve)
d.text((X + 6 * S + 11 * S, py + 19 * S), tag, font=f_mono, fill=crust, anchor='lm')
d.text((X + 6 * S + tw + 22 * S + 12 * S, py + 19 * S), label, font=f_pill, fill=sub, anchor='lm')

# título: tres líneas, la última con degradado lavanda → mauve → rosa
lines = ['Theme every', 'corner of', 'After Effects.']
ty = 214 * S
lh = 64 * S
for i, ln in enumerate(lines[:2]):
    d.text((X, ty + i * lh), ln, font=f_h1, fill=text)
gy = ty + 2 * lh
gw = int(d.textlength(lines[2], font=f_h1)) + 4 * S
mask = Image.new('L', (gw, lh + 20 * S), 0)
ImageDraw.Draw(mask).text((0, 0), lines[2], font=f_h1, fill=255)
gx = np.linspace(0, 1, gw, dtype=np.float32)[None, :, None]
stops = [(0, lavender), (0.6, mauve), (1, pink)]
grad = np.zeros((mask.height, gw, 3), np.float32)
for (p0, a0), (p1, a1) in zip(stops, stops[1:]):
    k = np.clip((gx - p0) / (p1 - p0), 0, 1)
    sel = (gx >= p0) & (gx <= p1)
    grad = np.where(sel, np.array(a0, np.float32) + (np.array(a1, np.float32) - np.array(a0, np.float32)) * k, grad)
gimg = Image.fromarray(np.clip(grad, 0, 255).astype(np.uint8), 'RGB').convert('RGBA')
gimg.putalpha(mask)
base.alpha_composite(gimg, (X, gy))
d = ImageDraw.Draw(base)

# texto
by = gy + lh + 30 * S
for i, ln in enumerate(['Panels, timeline, Home screen, menus and', 'title bar, recoloured from one palette.']):
    d.text((X, by + i * 32 * S), ln, font=f_body, fill=sub)

# datos al pie
fy = 630 * S - 58 * S
fx = X
for item in ['Windows 10 & 11', 'After Effects 2020–2027']:
    d.ellipse([fx, fy - 5 * S, fx + 10 * S, fy + 5 * S], fill=green)
    d.text((fx + 20 * S, fy), item, font=f_pill, fill=overlay, anchor='lm')
    fx += int(20 * S + d.textlength(item, font=f_pill) + 30 * S)

final = base.convert('RGB').resize((1200, 630), Image.LANCZOS)
final.save(OUT, optimize=True)
print(OUT, final.size)
