//! Compiles the C shim in `src/ffi/` against the vendored MuPDF headers, which must match
//! the MuPDF version that `mupdf-sys` builds (checked at runtime by `ffi::check_version`).

fn main() {
    let include = "../../third_party/mupdf-include";
    println!("cargo:rerun-if-changed=src/ffi/shim.c");
    println!("cargo:rerun-if-changed={include}");
    cc::Build::new()
        .file("src/ffi/shim.c")
        .include(include)
        .warnings(true)
        .compile("lectrix_mupdf_shim");
}
