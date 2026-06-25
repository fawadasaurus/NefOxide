use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("crate is under repo_root/crates");
    let lib_dir = repo_root.join("lib/NikonSDK/Frameworks");

    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
}
