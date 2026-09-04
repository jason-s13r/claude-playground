fn main() {
    build_kit::emit::Stamper::new("WWNZ")
        .tag_glob("woolworths-nz-cli/v*")
        .emit()
        .expect("stamping the build");
}
