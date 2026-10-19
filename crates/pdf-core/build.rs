//! Compiles the C shim in `src/ffi/` against the vendored MuPDF headers, which must match
//! the MuPDF version that `mupdf-sys` builds (checked at runtime by `ffi::check_version`),
//! and against libjpeg's headers from `mozjpeg-sys` (Compress encodes JPEG itself, ADR 0027).

fn main() {
    let include = "../../third_party/mupdf-include";
    println!("cargo:rerun-if-changed=src/ffi/shim.c");
    println!("cargo:rerun-if-changed=src/ffi/shim.h");
    println!("cargo:rerun-if-changed=src/ffi/compress.c");
    println!("cargo:rerun-if-changed={include}");
    let mut build = cc::Build::new();
    build
        .file("src/ffi/shim.c")
        .file("src/ffi/compress.c")
        .include(include)
        .warnings(true);
    // `mozjpeg-sys` (links = "jpeg") publishes its configured and vendored header folders.
    let jpeg =
        std::env::var_os("DEP_JPEG_INCLUDE").expect("mozjpeg-sys publishes DEP_JPEG_INCLUDE");
    for dir in std::env::split_paths(&jpeg) {
        build.include(dir);
    }
    build.compile("lectrix_mupdf_shim");
}
