// Presets de AeTint (los mismos que la app): [nombre, oscuro, claro]
// cada paleta: fondo, paneles, marcos, campos, texto, acento
const PRESETS = [
  ["Catppuccin Mocha", ["#11111B", "#1E1E2E", "#313244", "#181825", "#CDD6F4", "#7A80C2"], ["#DCE0E8", "#EFF1F5", "#CCD0DA", "#E6E9EF", "#4C4F69", "#7287FD"]],
  ["Tokyo Night", ["#101014", "#1A1B26", "#292E42", "#16161E", "#C0CAF5", "#7AA2F7"], ["#CBD0DF", "#E1E2E7", "#B9BDCF", "#D6D8E1", "#3760BF", "#2E7DE9"]],
  ["Rosé Pine", ["#121019", "#1F1D2E", "#26233A", "#191724", "#E0DEF4", "#C4A7E7"], ["#F2E9E1", "#FAF4ED", "#DFDAD9", "#F6EEE7", "#575279", "#907AA9"]],
  ["Nord", ["#20242C", "#2E3440", "#3B4252", "#272C36", "#D8DEE9", "#88C0D0"], ["#D8DEE9", "#ECEFF4", "#C8D0DC", "#E5E9F0", "#2E3440", "#5E81AC"]],
  ["Gruvbox", ["#141617", "#282828", "#3C3836", "#1D2021", "#EBDBB2", "#D79921"], ["#EBDBB2", "#FBF1C7", "#D5C4A1", "#F2E5BC", "#3C3836", "#B57614"]],
  ["Dracula", ["#191A21", "#282A36", "#44475A", "#21222C", "#F8F8F2", "#BD93F9"], ["#DEDCCF", "#FFFBEB", "#CFCFDE", "#F5F1E0", "#1F1F1F", "#644AC9"]],
  ["One Dark", ["#181A1F", "#282C34", "#3E4451", "#21252B", "#ABB2BF", "#61AFEF"], ["#E5E5E6", "#FAFAFA", "#D4D4D6", "#F0F0F1", "#383A42", "#4078F2"]],
  ["Monokai", ["#1E1F1C", "#272822", "#3E3D32", "#22231E", "#F8F8F2", "#A6E22E"], ["#E8E6DA", "#FAFAF8", "#D6D4C8", "#F2F1EC", "#272822", "#6A9C10"]],
  ["Solarized", ["#00212B", "#002B36", "#073642", "#00252F", "#93A1A1", "#268BD2"], ["#EEE8D5", "#FDF6E3", "#E0DAC6", "#F5EFDC", "#586E75", "#268BD2"]],
  ["GitHub", ["#010409", "#0D1117", "#21262D", "#161B22", "#E6EDF3", "#2F81F7"], ["#EAEEF2", "#FFFFFF", "#D0D7DE", "#F6F8FA", "#1F2328", "#0969DA"]],
  ["Everforest", ["#1E2326", "#2D353B", "#3D484D", "#272E33", "#D3C6AA", "#A7C080"], ["#EFEBD4", "#FDF6E3", "#E0DCC7", "#F4F0D9", "#5C6A72", "#8DA101"]],
  ["Kanagawa", ["#16161D", "#1F1F28", "#363646", "#1A1A22", "#DCD7BA", "#7E9CD8"], ["#E5DDB0", "#F2ECBC", "#DCD5AC", "#E7DBA0", "#545464", "#4D699B"]],
  ["Ayu", ["#171B24", "#1F2430", "#2A3040", "#1C212B", "#CCCAC2", "#FFCC66"], ["#E7EAED", "#FCFCFC", "#D8DCE0", "#F3F4F5", "#5C6166", "#FA8D3E"]],
  ["Graphite", ["#0A0A0A", "#171717", "#2A2A2A", "#101010", "#D4D4D4", "#E5484D"], ["#E4E4E4", "#F7F7F7", "#D2D2D2", "#EFEFEF", "#262626", "#E5484D"]],
  ["AE original", ["#000000", "#1D1D1D", "#343434", "#0E0E0E", "#B0B0B0", "#066CE7"], ["#E6E6E6", "#FFFFFF", "#E6E6E6", "#F8F8F8", "#505050", "#0265DC"]],
];

const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];

// ── título: cada palabra en su caja, para que entre una detrás de otra
{
  const h = $("[data-split]");
  let i = 0;
  const wrap = (node) => {
    [...node.childNodes].forEach((n) => {
      if (n.nodeType === 3) {
        const frag = document.createDocumentFragment();
        n.textContent.split(/(\s+)/).forEach((part) => {
          if (!part) return;
          if (/^\s+$/.test(part)) frag.append(part);
          else {
            const s = document.createElement("span");
            s.className = "w";
            s.style.setProperty("--i", i++);
            s.textContent = part;
            frag.append(s);
          }
        });
        n.replaceWith(frag);
      } else if (n.nodeType === 1) wrap(n);
    });
  };
  if (h) wrap(h);
  document.documentElement.classList.add("split");
}

// ── vista previa de paletas
const ae = $("#ae");
const chips = $("#chips");
const label = $("#tryname");
let mode = "dark";
let current = 0;

// ── colores de toda la página a partir de una paleta (fondo, paneles, marcos, campos, texto, acento)
const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const toHex = (c) => "#" + c.map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("");
const mix = (a, b, t) => toHex(hex(a).map((v, i) => v + (hex(b)[i] - v) * t));
const lum = (h) => {
  const [r, g, b] = hex(h).map((v) => {
    v /= 255;
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
let themeTimer = null;
let themed = false; // la primera paleta (al cargar) se pone sin transición, para no pisar la entrada de la portada
function themePage(p, light) {
  const [base, panel, , , text, accent] = p;
  const root = document.documentElement;
  const v = {
    "--bg": light ? base : mix(base, "#000000", 0.35),
    "--bg-2": light ? mix(base, panel, 0.5) : mix(base, "#000000", 0.15),
    "--surface": light ? panel : mix(panel, base, 0.35),
    "--surface-2": light ? mix(panel, "#ffffff", 0.4) : panel,
    "--text": light ? mix(text, "#000000", 0.25) : mix(text, "#ffffff", 0.35),
    "--muted": mix(text, light ? base : panel, light ? 0.3 : 0.38),
    "--dim": mix(text, base, light ? 0.45 : 0.55),
    "--line": light ? "rgb(0 0 0 / 0.09)" : "rgb(255 255 255 / 0.07)",
    "--line-2": light ? "rgb(0 0 0 / 0.15)" : "rgb(255 255 255 / 0.12)",
    "--accent": accent,
    "--accent-2": light ? mix(accent, "#000000", 0.18) : mix(accent, "#ffffff", 0.3),
    "--accent-ink": lum(accent) > 0.35 ? "#0b0b14" : "#ffffff",
  };
  if (themed) root.classList.add("theming");
  themed = true;
  Object.entries(v).forEach(([k, val]) => root.style.setProperty(k, val));
  root.style.colorScheme = light ? "light" : "dark";
  document.querySelector('meta[name="theme-color"]').setAttribute("content", v["--bg"]);
  // icono de la pestaña: el mismo logo con los colores del tema
  const ink = mix(v["--bg"], "#000000", 0.15);
  const cti = mix(accent, "#000000", 0.1);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${mix(accent, "#ffffff", 0.32)}"/><stop offset=".5" stop-color="${accent}"/><stop offset="1" stop-color="${mix(accent, "#000000", 0.45)}"/></linearGradient></defs><rect width="32" height="32" rx="8.96" fill="url(#g)"/><path d="M16 4.5C16 4.5 8.5 12.6 8.5 18.3a7.5 7.5 0 0 0 15 0C23.5 12.6 16 4.5 16 4.5Z" fill="#fff"/><rect x="11" y="15.6" width="7.5" height="1.9" rx=".95" fill="${ink}"/><rect x="13" y="18.6" width="8" height="1.9" rx=".95" fill="${ink}"/><rect x="11.5" y="21.6" width="5.5" height="1.9" rx=".95" fill="${ink}"/><rect x="17.4" y="12.6" width="1.2" height="11.8" fill="${cti}"/><rect x="16.5" y="11.4" width="3" height="2.2" rx=".7" fill="${cti}"/></svg>`;
  let fav = document.querySelector('link[rel="icon"]');
  fav.setAttribute("type", "image/svg+xml");
  fav.setAttribute("href", "data:image/svg+xml," + encodeURIComponent(svg));
  clearTimeout(themeTimer);
  themeTimer = setTimeout(() => root.classList.toggle("theming", false), 700);
}

function paint() {
  const [name, dark, light] = PRESETS[current];
  const p = mode === "dark" ? dark : light;
  ["base", "panel", "frame", "raised", "text", "accent"].forEach((k, i) => ae.style.setProperty(`--a-${k}`, p[i]));
  themePage(p, mode === "light");
  label.textContent = `${name} · ${mode === "dark" ? "Dark" : "Light"}`;
  $$(".chip", chips).forEach((c, i) => {
    c.setAttribute("aria-selected", i === current ? "true" : "false");
    const sw = mode === "dark" ? PRESETS[i][1] : PRESETS[i][2];
    $$("b", c).forEach((b, j) => (b.style.background = [sw[1], sw[4], sw[5]][j]));
  });
}

PRESETS.forEach(([name], i) => {
  const b = document.createElement("button");
  b.className = "chip";
  b.type = "button";
  b.setAttribute("role", "option");
  b.innerHTML = `<i aria-hidden="true"><b></b><b></b><b></b></i>${name}`;
  b.addEventListener("click", () => {
    stopCycle();
    current = i;
    paint();
  });
  chips.append(b);
});
$$(".seg button").forEach((b) =>
  b.addEventListener("click", () => {
    stopCycle();
    mode = b.dataset.mode;
    $$(".seg button").forEach((x) => x.setAttribute("aria-selected", x === b ? "true" : "false"));
    paint();
  })
);
paint();

// los presets cambian solos mientras se ven, hasta que el usuario elige uno
const CYCLE = 2800;
const prog = $("#tryprog");
let timer = null;
let picked = false;
const restartBar = () => {
  prog.classList.remove("run");
  void prog.offsetWidth;
  prog.classList.add("run");
};
function startCycle() {
  if (picked || reduce || timer) return;
  prog.style.setProperty("--cycle", CYCLE + "ms");
  restartBar();
  timer = setInterval(() => {
    current = (current + 1) % PRESETS.length;
    paint();
    restartBar();
  }, CYCLE);
}
function pauseCycle() {
  clearInterval(timer);
  timer = null;
  prog.classList.remove("run");
}
function stopCycle() {
  picked = true;
  pauseCycle();
  prog.classList.add("off");
}
new IntersectionObserver((es) => es.forEach((e) => (e.isIntersecting ? startCycle() : pauseCycle())), { threshold: 0.3 }).observe(ae);

// ── scroll suave
let lenis = null;
if (!reduce && window.Lenis) {
  lenis = new Lenis({ lerp: 0.13, smoothWheel: true, wheelMultiplier: 1.05 });
  const raf = (t) => {
    lenis.raf(t);
    requestAnimationFrame(raf);
  };
  requestAnimationFrame(raf);
}
$$('a[href^="#"]').forEach((a) =>
  a.addEventListener("click", (e) => {
    const id = a.getAttribute("href");
    const el = id === "#top" || id === "#" ? document.body : $(id);
    if (!el) return;
    e.preventDefault();
    if (lenis) lenis.scrollTo(el === document.body ? 0 : el, { offset: -72, duration: 1.5 });
    else if (el === document.body) window.scrollTo({ top: 0, behavior: reduce ? "auto" : "smooth" });
    else el.scrollIntoView({ behavior: reduce ? "auto" : "smooth" });
  })
);

// ── barra superior y captura de la portada que se endereza al bajar
const topBar = $(".top");
const shot = $("#heroShot");
function onScroll() {
  topBar.classList.toggle("scrolled", window.scrollY > 16);
  if (reduce || !shot) return;
  const r = shot.getBoundingClientRect();
  const vh = window.innerHeight;
  const t = Math.min(1, Math.max(0, (vh - r.top) / (vh * 0.8)));
  const e = 1 - Math.pow(1 - t, 3);
  shot.style.setProperty("--tilt", (18 * (1 - e)).toFixed(2) + "deg");
  shot.style.setProperty("--lift", (0.92 + 0.08 * e).toFixed(4));
}
if (lenis) lenis.on("scroll", onScroll);
else window.addEventListener("scroll", onScroll, { passive: true });
window.addEventListener("resize", onScroll);
onScroll();

// ── apariciones en los dos sentidos: entra al llegar a la pantalla y se va al salir
//    (los hermanos de un mismo grupo, uno detrás de otro)
$$("[data-rv]").forEach((el) => {
  const sibs = [...el.parentElement.children].filter((c) => c.hasAttribute("data-rv"));
  el.style.setProperty("--i", Math.min(sibs.indexOf(el), 6));
});
const rv = new IntersectionObserver(
  (es) => es.forEach((e) => e.target.classList.toggle("in", e.isIntersecting)),
  { rootMargin: "0px 0px -10% 0px", threshold: 0.12 }
);
$$("[data-rv]").forEach((el) => rv.observe(el));

// ── cifras que cuentan hasta su valor cada vez que aparecen
const counters = $$("[data-count]");
const count = (el) => {
  const to = +el.dataset.count;
  const t0 = performance.now();
  const dur = 1400;
  const step = (t) => {
    const k = Math.min(1, (t - t0) / dur);
    el.textContent = Math.round(to * (1 - Math.pow(1 - k, 4)));
    if (k < 1) el._raf = requestAnimationFrame(step);
  };
  cancelAnimationFrame(el._raf);
  el._raf = requestAnimationFrame(step);
};
if (!reduce) {
  counters.forEach((el) => (el.textContent = "0"));
  const co = new IntersectionObserver(
    (es) => es.forEach((e) => {
      const el = e.target.querySelector("[data-count]");
      if (e.isIntersecting) count(el);
      else {
        cancelAnimationFrame(el._raf);
        el.textContent = "0";
      }
    }),
    { threshold: 0.4 }
  );
  $$(".stat").forEach((s) => co.observe(s));
}

// ── tarjetas: el brillo sigue al ratón
$$(".card").forEach((c) =>
  c.addEventListener("pointermove", (e) => {
    const r = c.getBoundingClientRect();
    c.style.setProperty("--mx", e.clientX - r.left + "px");
    c.style.setProperty("--my", e.clientY - r.top + "px");
  })
);

// ── cómo funciona: el paso que está a media pantalla manda qué captura se ve
const steps = $$(".how-step");
const imgs = $$(".how-img");
{
  const so = new IntersectionObserver(
    (es) => es.forEach((e) => {
      if (!e.isIntersecting) return;
      const n = +e.target.dataset.step;
      steps.forEach((s, i) => s.classList.toggle("is-active", i === n));
      imgs.forEach((im, i) => im.classList.toggle("is-active", i === n));
    }),
    { rootMargin: "-45% 0px -45% 0px" }
  );
  steps.forEach((s) => so.observe(s));
}

// ── preguntas: se abren y cierran con animación de altura
$$(".faq details").forEach((d) => {
  const sum = $("summary", d);
  const body = $(".faq-a", d);
  let anim = null;
  sum.addEventListener("click", (e) => {
    if (reduce) return;
    e.preventDefault();
    if (anim) anim.cancel();
    if (!d.open) {
      d.open = true;
      d.classList.add("opening");
      const h = body.scrollHeight;
      anim = body.animate([{ height: "0px", opacity: 0 }, { height: h + "px", opacity: 1 }], { duration: 480, easing: "cubic-bezier(0.16, 1, 0.3, 1)" });
      anim.onfinish = () => {
        d.classList.remove("opening");
        anim = null;
      };
    } else {
      d.classList.add("closing");
      const h = body.offsetHeight;
      anim = body.animate([{ height: h + "px", opacity: 1 }, { height: "0px", opacity: 0 }], { duration: 380, easing: "cubic-bezier(0.16, 1, 0.3, 1)" });
      anim.onfinish = () => {
        d.open = false;
        d.classList.remove("closing");
        anim = null;
      };
    }
  });
});

// ── fuera de pantalla no se anima: la luz que recorre los marcos y la cinta de versiones
{
  const po = new IntersectionObserver((es) => es.forEach((e) => e.target.classList.toggle("off", !e.isIntersecting)), { rootMargin: "100px" });
  $$(".frame, .final-card, .marquee").forEach((el) => po.observe(el));
}

// ── imágenes, logos e iconos: sin menú de clic derecho ni arrastrar
$$("img").forEach((im) => im.setAttribute("draggable", "false"));
document.addEventListener("contextmenu", (e) => {
  if (e.target.closest("img, svg, figure, .frame, .brand, .viz, .ae")) e.preventDefault();
});
document.addEventListener("dragstart", (e) => {
  if (e.target.closest && e.target.closest("img, svg, a")) e.preventDefault();
});

// ── entrada de la portada cuando las fuentes y la captura están listas (así nada salta ni aparece tarde);
// como mucho se espera 900 ms. Dos fotogramas de margen para que el estado oculto se pinte antes de animar.
{
  const waits = [];
  if (document.fonts && document.fonts.ready) waits.push(document.fonts.ready);
  const img = $("#heroShot img");
  if (img && img.decode) waits.push(img.decode().catch(() => {}));
  let done = false;
  const ready = () => {
    if (done) return;
    done = true;
    requestAnimationFrame(() => requestAnimationFrame(() => document.documentElement.classList.add("ready")));
    setTimeout(() => document.documentElement.classList.add("ready"), 120);
  };
  Promise.race([Promise.all(waits), new Promise((r) => setTimeout(r, 900))]).then(ready);
}

// ── versión publicada más reciente (si GitHub no responde, se queda la de la página)
fetch("https://api.github.com/repos/xjuanfxx/AeTint/releases/latest")
  .then((r) => (r.ok ? r.json() : null))
  .then((j) => {
    if (j && j.tag_name) $("#ver").textContent = j.tag_name.split(" ")[0];
  })
  .catch(() => {});
