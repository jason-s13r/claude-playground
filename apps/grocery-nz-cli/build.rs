fn main() {
    build_kit::emit::Stamper::new("GSNZ")
        .tag_glob("grocery-nz-cli/v*")
        .emit()
        .expect("stamping the build");
}
