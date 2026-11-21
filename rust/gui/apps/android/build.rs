fn main() {
    println!("cargo:rerun-if-env-changed=APP_VERSION");
    let version = std::env::var("APP_VERSION").unwrap_or_else(|_| "1.0.6".to_string());
    println!("cargo:rustc-env=APP_VERSION={version}");
    tauri_build::build();
}
