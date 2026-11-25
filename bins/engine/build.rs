fn main() {
    println!("cargo:rerun-if-changed=../../apps/desktop/src-tauri/icons/icon.ico");
    #[cfg(target_os = "windows")]
    {
        let mut res = tauri_winres::WindowsResource::new();
        res.set_icon("../../apps/desktop/src-tauri/icons/icon.ico");
        res.set("ProductName", "TrueShow");
        res.set("FileDescription", "TrueShow Real-Time GPU Compositor Engine");
        if let Err(err) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon resource in trueshow-engine: {err}");
        }
    }
}
