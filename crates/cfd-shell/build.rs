use std::env;
use std::path::PathBuf;

#[path = "../../build_res.rs"]
mod build_res;

fn main() {
    build_res::compile_pe_resources(&build_res::PeMetadata {
        internal_name: "cfd_shell",
        original_filename: "cfd_shell.dll",
        file_description: "CustomerFD Explorer Sidebar Shell Extension",
        is_dll: true,
    });

    // COM exports must be PRIVATE (not in the import library), otherwise the MSVC linker warns
    // LNK4104; the .def file also keeps the export table to exactly these four functions.
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let def_file = manifest_dir.join("cfd_shell.def");
    println!("cargo:rerun-if-changed={}", def_file.display());
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_env == "msvc" {
        println!("cargo:rustc-cdylib-link-arg=/DEF:{}", def_file.display());
        println!("cargo:rustc-cdylib-link-arg=/DYNAMICBASE");
        println!("cargo:rustc-cdylib-link-arg=/NXCOMPAT");
        // Record only the PDB file name (not the build machine's absolute path) in the PE.
        println!("cargo:rustc-cdylib-link-arg=/PDBALTPATH:%_PDB%");
        if arch == "x86_64" {
            println!("cargo:rustc-cdylib-link-arg=/HIGHENTROPYVA");
            println!("cargo:rustc-cdylib-link-arg=/CETCOMPAT");
        }
    }
}
