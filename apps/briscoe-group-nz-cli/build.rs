fn main() {
    build_kit::emit::Stamper::new("BGNZ")
        .tag_glob("briscoe-group-nz-cli/v*")
        .emit()
        .expect("stamping the build");
}
