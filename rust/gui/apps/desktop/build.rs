fn main() {
    #[cfg(windows)]
    {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set");
        let icon = std::path::Path::new(&manifest_dir).join("../../../assets/logo/logo.ico");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon(icon.to_string_lossy().as_ref());
        resource
            .compile()
            .expect("failed to compile Windows resources");
    }
}
