fn main() {
    // Ask hybrid-GPU laptops (NVIDIA Optimus / AMD PowerXpress) for the discrete GPU.
    println!("cargo:rustc-link-arg-bins=/EXPORT:NvOptimusEnablement");
    println!("cargo:rustc-link-arg-bins=/EXPORT:AmdPowerXpressRequestHighPerformance");
    // Exe icon (Explorer, taskbar, installer shortcuts).
    println!("cargo:rerun-if-changed=../../assets/evj.ico");
    embed_resource::compile("evj.rc", embed_resource::NONE).manifest_optional().expect("icon resource");
}
