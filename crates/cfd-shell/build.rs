#[path = "../../build_res.rs"]
mod build_res;

fn main() {
    build_res::compile_pe_resources(&build_res::PeMetadata {
        internal_name: "cfd_shell",
        original_filename: "cfd_shell.dll",
        file_description: "CustomerFD Explorer Sidebar Shell Extension",
        is_dll: true,
    });
}
