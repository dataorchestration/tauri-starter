fn main() {
    tauri_build::build();

    // Rebuild when the starter config changes: it is embedded with
    // `include_str!` (src/config.rs) so the app and the frontend read one file.
    println!("cargo:rerun-if-changed=../starter.config.json");

    // Apple frameworks behind the pure-Rust permission checks (src/permissions.rs).
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=ApplicationServices"); // AXIsProcessTrusted
        println!("cargo:rustc-link-lib=framework=CoreGraphics"); // CGPreflightScreenCaptureAccess
        println!("cargo:rustc-link-lib=framework=IOKit"); // IOHIDCheckAccess
        println!("cargo:rustc-link-lib=framework=AVFoundation"); // AVCaptureDevice
    }
}
