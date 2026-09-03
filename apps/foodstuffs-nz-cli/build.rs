fn main() {
    build_kit::emit::Stamper::new("FSNZ")
        .tag_glob("foodstuffs-nz-cli/v*")
        .emit()
        .expect("stamping the build");
}
