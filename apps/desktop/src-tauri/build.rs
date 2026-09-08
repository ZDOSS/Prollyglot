fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        // Shared speech/ONNX runtimes live beside development binaries and in
        // a private library directory in the Ubuntu package.
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN:$ORIGIN/../lib/prollyglot");
    }
    tauri_build::build()
}
