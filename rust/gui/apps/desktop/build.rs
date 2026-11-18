fn main() {
    println!("cargo:rerun-if-env-changed=APP_VERSION");
    println!("cargo:rerun-if-changed=build.rs");

    let version = std::env::var("APP_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").trim().to_string());

    #[cfg(windows)]
    {
        const APP_NAME: &str = "短视频解析";
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
        let icon = std::path::Path::new(&manifest_dir).join("../../../assets/logo/logo.ico");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(icon.to_string_lossy().as_ref());
        resource.set("FileDescription", APP_NAME);
        resource.set("ProductName", APP_NAME);
        resource.set("InternalName", APP_NAME);
        resource.set("OriginalFilename", "短视频解析.exe");
        resource.set("CompanyName", APP_NAME);

        let mut parts = version.split('.');
        let major = parts
            .next()
            .and_then(|part| part.parse::<u16>().ok())
            .unwrap_or(0);
        let minor = parts
            .next()
            .and_then(|part| part.parse::<u16>().ok())
            .unwrap_or(0);
        let patch = parts
            .next()
            .and_then(|part| part.parse::<u16>().ok())
            .unwrap_or(0);
        let build = parts
            .next()
            .and_then(|part| part.parse::<u16>().ok())
            .unwrap_or(0);
        let numeric_version =
            (major as u64) << 48 | (minor as u64) << 32 | (patch as u64) << 16 | build as u64;

        resource.set("FileVersion", &version);
        resource.set("ProductVersion", &version);
        resource.set_version_info(winresource::VersionInfo::FILEVERSION, numeric_version);
        resource.set_version_info(winresource::VersionInfo::PRODUCTVERSION, numeric_version);
        resource
            .compile()
            .expect("failed to compile Windows resources");
    }
}
