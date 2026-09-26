fn main() {
    // `tauri::generate_context!` requires `frontendDist` to exist even for
    // `cargo check` and unit tests. Trunk populates it for real builds; the
    // platform packaging scripts still fail closed when index.html is absent.
    let frontend =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../target/build/frontend");
    if let Err(error) = std::fs::create_dir_all(&frontend) {
        println!(
            "cargo:warning=could not create {}: {error}",
            frontend.display()
        );
    }
    tauri_build::build()
}
