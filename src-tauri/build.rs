fn main() {
    // tauri-build only watches its config, so a changed icon would not be re-embedded
    // in the exe (taskbar, tray, installer) without this.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
