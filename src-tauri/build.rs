fn main() {
    let lib = pkg_config::probe_library("libfido2").expect("libfido2 development files");
    let mut build = cc::Build::new();
    build.file("src/fido.c");
    for p in lib.include_paths {
        build.include(p);
    }
    build.compile("elo_fido");
    tauri_build::build();
}
