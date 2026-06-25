use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("crate is under repo_root/crates")
        .to_path_buf();
    let nikon_sdk_dir = repo_root.join("lib/NikonSDK");
    let lib_dir = nikon_sdk_dir.join("Frameworks");
    let lib_include_dir = nikon_sdk_dir.join("Include");
    let nkfl_header = lib_include_dir.join("Nkfl_Interface.h");
    let wrapper = manifest_dir.join("wrapper.h");
    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set"));

    println!("cargo:rerun-if-changed={}", wrapper.display());
    println!("cargo:rerun-if-changed={}", nkfl_header.display());

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=ImgSDK");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());

    let bindings = bindgen::Builder::default()
        .header(wrapper.display().to_string())
        .clang_arg(format!("-I{}", lib_include_dir.display()))
        .allowlist_function("Nkfl_Entry")
        .allowlist_type("Nkfl.*")
        .allowlist_type("tagNkfl.*")
        .allowlist_type("eNkfl.*")
        .allowlist_type("RECT")
        .allowlist_type("Rect")
        .allowlist_var("kNkfl_.*")
        .generate_comments(false)
        .layout_tests(false)
        .derive_default(true)
        .generate()
        .expect("generate Nikon SDK bindings");

    bindings
        .write_to_file(out_path.join("nkfl_bindings.rs"))
        .expect("write Nikon SDK bindings");
}
