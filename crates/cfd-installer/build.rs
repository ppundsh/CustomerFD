//! Packs the release binaries into `OUT_DIR/payload.bin` for `include_bytes!`.
//!
//! Format: `b"CFDPAK1\0"`, `u32 count`, then per file `u16 name_len, name (UTF-8), u64 size,
//! bytes` (little endian). The source folder is `CFD_PAYLOAD_DIR` (default
//! `<workspace>/target/release`); `build.ps1` builds the other crates first and bumps
//! `CFD_PACK_STAMP` so the installer is always repacked.

#[path = "../../build_res.rs"]
mod build_res;

use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Src {
    /// x64 release folder (`CFD_PAYLOAD_DIR`, default `target/release`)
    Payload,
    /// 32-bit release folder (`CFD_PAYLOAD_X86_DIR`, default `target/i686-pc-windows-msvc/release`)
    PayloadX86,
    /// workspace root
    Workspace,
}

/// (source file, name inside the installer, where it comes from). `cfd_shell.x86.dll` is
/// installed as `x86\cfd_shell.dll` (see `install::target_path`).
const FILES: &[(&str, &str, Src)] = &[
    ("cfd-manager.exe", "cfd-manager.exe", Src::Payload),
    ("cfd_shell.dll", "cfd_shell.dll", Src::Payload),
    ("cfd_shell.dll", "cfd_shell.x86.dll", Src::PayloadX86),
    ("LICENSE", "LICENSE.txt", Src::Workspace),
    ("README.md", "README.md", Src::Workspace),
];

fn main() {
    println!("cargo:rerun-if-env-changed=CFD_PAYLOAD_DIR");
    println!("cargo:rerun-if-env-changed=CFD_PAYLOAD_X86_DIR");
    println!("cargo:rerun-if-env-changed=CFD_PACK_STAMP");
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf();
    let payload_dir = std::env::var("CFD_PAYLOAD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target").join("release"));
    let payload_x86_dir = std::env::var("CFD_PAYLOAD_X86_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            root.join("target")
                .join("i686-pc-windows-msvc")
                .join("release")
        });

    let mut out = Vec::new();
    out.extend_from_slice(b"CFDPAK1\0");
    let mut entries: Vec<(&str, Vec<u8>)> = Vec::new();
    let mut complete = true;
    for (src, name, from) in FILES {
        let path = match from {
            Src::Payload => payload_dir.join(src),
            Src::PayloadX86 => payload_x86_dir.join(src),
            Src::Workspace => root.join(src),
        };
        println!("cargo:rerun-if-changed={}", path.display());
        match std::fs::read(&path) {
            Ok(b) => entries.push((name, b)),
            Err(_) => complete = false,
        }
    }
    if !complete {
        println!("cargo:warning=installer payload incomplete ({}); build with build.ps1 to embed the program files", payload_dir.display());
        entries.clear();
    }
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (name, bytes) in &entries {
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("payload.bin"), out).unwrap();

    build_res::compile_pe_resources(&build_res::PeMetadata {
        internal_name: "CustomerFD-Setup",
        original_filename: "CustomerFD-Setup.exe",
        file_description: "CustomerFD Setup",
        is_dll: false,
    });
    build_res::harden_exe_linker();
}
