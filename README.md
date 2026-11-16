# AeTint

Temas completos para After Effects 2026/2027 en Windows, en Rust + Slint.

| Zona de AE | De dónde sale el color | Cómo lo cambia AeTint |
|---|---|---|
| Paneles, botones, campos, listas | `dvaui.dll`: tablas float RGBA por brillo + JSON `DROVER-DNA-VARS` + CSS `SPECTRUM_DARK` | Reescribe los números (mismo tamaño de archivo) |
| Timeline: navegador, regla, área de trabajo, CTI, barras de capa, graph editor… | `AfterFXLib.dll`: JSON `AE-SPECTRUM-COLORTHEMES` (~120 colores `ae-color-*`) | Reescribe el JSON compacto y lo rellena con espacios (mismo tamaño) |
| Pantalla de inicio (Home) | Plugin UXP `UXP\plugins\com.adobe.ccx.start`: hojas Spectrum (CSS y JSON) | Solo cambia los colores de las reglas `spectrum--darkest` / `--dark` y las variables `--color-dark-grey-*`; el modo claro queda igual |
| Menú File/Edit/…, desplegables y barra de título | Windows (menú Win32 + tema "Menu" + DWM) | `aetint_menu.dll` cargada en `AfterFX.exe`; no toca archivos de Adobe |

## Uso

- `AeTint.exe`: interfaz. Con After Effects cerrado, pulsa **Aplicar tema**. La primera vez Windows pide permiso de administrador para escribir en Program Files, y a partir de ahí este usuario puede modificar esos dos archivos sin volver a pedirlo.
- **Restaurar originales** vuelve a poner las copias guardadas en `%LOCALAPPDATA%\AeTint\originals`.
- **Pintar File, Edit… y el título**: los menús los carga el plugin `AeTint.aex`. Solo si alguna versión de AE instalada no tiene el plugin (o lo tiene desactualizado), AeTint arranca `AeTint.exe --watch`, un vigilante sin ventana que duerme hasta que Windows avisa de una ventana nueva y, si es de After Effects, le carga la DLL. Los colores se leen de `%LOCALAPPDATA%\AeTint\menu.json`, y se recargan en vivo.
- `aetint_menu.dll` y `aetint_loader.dll` tienen que estar junto a `AeTint.exe`. AeTint copia la DLL de menús a `%LOCALAPPDATA%\AeTint\`, de donde la cargan el plugin y el vigilante.
- Con los menús activados, **Aplicar tema** instala `AeTint.aex` (el `aetint_loader.dll` renombrado) en `Support Files\Plug-ins`: After Effects lo carga solo al arrancar y este carga la DLL, sin inyección ni vigilante. **Restaurar originales** lo quita.
- Cada tema tiene dos paletas: **Dark** (brillos Darkest y Dark de AE) y **Light** (brillo claro). El selector *Dark | Light* de «Your colours» elige cuál se edita; los presets traen las dos (Catppuccin Mocha/Latte, Tokyo Night/Day, Rosé Pine/Dawn, Nord/Snow Storm, Gruvbox dark/light…). Paneles, timeline, Home, menús y barra de título usan la del brillo que tenga AE; la DLL detecta el brillo por el modo oscuro/claro de la ventana de AE y cambia en vivo.
- **My themes** guarda paletas propias (con sus colores de timeline) en `%LOCALAPPDATA%\AeTint	hemes.json`.
- Al abrir, AeTint carga el tema que tiene instalado la primera versión marcada (o AE original si está sin tocar).

Si un `dvaui.dll` ya tiene un tema de Nothi Theme, AeTint usa el original que guardó Nothi (`%LOCALAPPDATA%\NothiTheme\Originals`).

## Desarrollo

```
cargo build --release --workspace
cargo test
target\release\AeTint.exe --dry-run carpeta   # fabrica los DLL con tema en una carpeta, sin instalar
target\release\menutest.exe --open --dll ruta\aetint_menu.dll   # ventana de prueba con menú Win32
target\release\AeTint.exe --inject <pid>   # carga la DLL en un proceso
```

`AETINT_MENU_LOG=1` hace que la DLL escriba un log en `%TEMP%\aetint_menu.log`.

La interfaz está en inglés por defecto; el selector **EN | ES** de la cabecera la cambia a español (se guarda en `config.json`).

Fuentes incluidas: Inter, Archivo y JetBrains Mono (SIL Open Font License, ver `ui/fonts/*-OFL.txt`). Icono: `assets/icon.ico`, incrustado en el .exe por `build.rs`.

Licencia GPL-3.0 (Slint).
