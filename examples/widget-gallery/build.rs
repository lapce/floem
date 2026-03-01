// build.rs
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=dylib=system_trace");
        println!("cargo:rustc-link-search=native=/usr/lib/system");
    }
}
