fn main() {
    let resource =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("desktop.gresource");
    for file in [
        "desktop.gresource.xml",
        "32x32.png",
        "128x128.png",
        "icon.png",
    ] {
        println!("cargo:rerun-if-changed=icons/{file}");
    }
    let status = std::process::Command::new("glib-compile-resources")
        .args([
            "--sourcedir=icons",
            "icons/desktop.gresource.xml",
            "--target",
        ])
        .arg(resource)
        .status()
        .expect("glib-compile-resources is required with the GTK development tools");
    assert!(status.success(), "Could not compile embedded desktop icons");
    let lib = pkg_config::probe_library("libfido2").expect("libfido2 development files");
    let mut build = cc::Build::new();
    build.file("src/fido.c");
    for p in lib.include_paths {
        build.include(p);
    }
    build.compile("elo_fido");
    tauri_build::build();
}
