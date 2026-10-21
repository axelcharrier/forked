fn main() {
    println!("cargo:rerun-if-changed=assets/icon/forked.ico");
    // Embeds the icon and version info in Forked.exe: uses rc.exe from the
    // Windows SDK on Windows, or MinGW's windres when cross-compiling.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/forked.ico")
            .set("ProductName", "Forked")
            .set("FileDescription", "Forked — client Git");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icône non intégrée à l'exécutable : {e}");
        }
    }
}
