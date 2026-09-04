fn main() {
    build_kit::emit::Stamper::new("TWLNZ")
        .tag_glob("the-warehouse-nz-cli/v*")
        .emit()
        .expect("stamping the build");
}
