fn main() {
    slint_build::compile("ui/app.slint").expect("Slint build failed");
    // icono y datos del .exe (Explorador, barra de tareas, Administrador de tareas)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("FileDescription", "AeTint");
        res.set("ProductName", "AeTint");
        if let Err(e) = res.compile() {
            println!("cargo:warning=no se pudo incrustar el icono: {e}");
        }
    }
}
