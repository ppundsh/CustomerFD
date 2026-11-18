#[path = "../../build_res.rs"]
mod build_res;

fn main() {
    println!("cargo:rerun-if-changed=../../assets/updater.pub");
    println!("cargo:rerun-if-changed=../../assets/release-approvers.pub");
    build_res::compile_pe_resources(&build_res::PeMetadata {
        internal_name: "cfd-manager",
        original_filename: "cfd-manager.exe",
        file_description: "CustomerFD Control Center",
        is_dll: false,
    });
    build_res::harden_exe_linker();
}
