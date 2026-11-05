fn main() {
    // Cargo features select dependencies; the config is authoritative. Reject
    // drift instead of advertising a different backend to the webview.
    let config: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("../starter.config.json").expect("starter.config.json must exist"),
    )
    .expect("starter.config.json must be valid JSON");
    let engine = config["database"]["engine"]
        .as_str()
        .expect("database.engine must be sqlcipher or turso");
    let import = config["database"]["importSqlcipher"]
        .as_bool()
        .expect("database.importSqlcipher must be true or false");
    let sqlcipher = std::env::var_os("CARGO_FEATURE_DATABASE_SQLCIPHER").is_some();
    let turso = std::env::var_os("CARGO_FEATURE_DATABASE_TURSO").is_some();
    let legacy = std::env::var_os("CARGO_FEATURE_LEGACY_SQLCIPHER_IMPORT").is_some();
    assert!(
        matches!(engine, "sqlcipher" | "turso"),
        "unknown database engine; run npm run configure -- --database sqlcipher|turso"
    );
    assert!(
        !(import && engine != "turso"),
        "SQLCipher import requires the Turso backend"
    );
    assert!(
        sqlcipher != turso && sqlcipher == (engine == "sqlcipher") && legacy == import,
        "database config/Cargo features disagree; run npm run configure -- --database {engine} --import-sqlcipher {import}"
    );

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
