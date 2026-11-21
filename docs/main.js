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

const ae = document.getElementById("ae");
const chips = document.getElementById("chips");
const name = document.getElementById("tryname");
let mode = "dark";
let current = 0;

function paint() {
  const [label, dark, light] = PRESETS[current];
  const p = mode === "dark" ? dark : light;
  ["base", "panel", "frame", "raised", "text", "accent"].forEach((k, i) => ae.style.setProperty(`--a-${k}`, p[i]));
  name.textContent = `${label} · ${mode === "dark" ? "Dark" : "Light"}`;
  chips.querySelectorAll(".chip").forEach((c, i) => {
    c.setAttribute("aria-selected", i === current ? "true" : "false");
    const sw = mode === "dark" ? PRESETS[i][1] : PRESETS[i][2];
    const bs = c.querySelectorAll("b");
    [sw[1], sw[4], sw[5]].forEach((col, j) => (bs[j].style.background = col));
  });
}

PRESETS.forEach(([label], i) => {
  const b = document.createElement("button");
  b.className = "chip";
  b.type = "button";
  b.setAttribute("role", "option");
  b.innerHTML = `<i aria-hidden="true"><b></b><b></b><b></b></i>${label}`;
  b.addEventListener("click", () => {
    current = i;
    paint();
  });
  chips.appendChild(b);
});

document.querySelectorAll(".seg button").forEach((b) => {
  b.addEventListener("click", () => {
    mode = b.dataset.mode;
    document.querySelectorAll(".seg button").forEach((x) => x.setAttribute("aria-selected", x === b ? "true" : "false"));
    paint();
  });
});

paint();

// versión publicada más reciente (si GitHub no responde, se queda la escrita en la página)
fetch("https://api.github.com/repos/xjuanfxx/AeTint/releases/latest")
  .then((r) => (r.ok ? r.json() : null))
  .then((j) => {
    if (j && j.tag_name) document.getElementById("ver").textContent = j.tag_name.split(" ")[0];
  })
  .catch(() => {});
