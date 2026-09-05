fn main() {
    build_kit::emit::Stamper::new("KMART")
        .tag_glob("kmart-cli/v*")
        .emit()
        .expect("stamping the build");
}
