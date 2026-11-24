//! Build-time step that shapes the final exe: stamp the Windows app icon (`Pepperoni.ico`). The
//! two ONNX models are not handled here: `birefnet.rs` and `maria_antonieta.rs` embed their file
//! from `models/` with `include_bytes!`. See docs/architecture/modules/build.md.

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=Pepperoni.ico");
        winresource::WindowsResource::new()
            .set_icon("Pepperoni.ico")
            .compile()
            .expect("embedding Pepperoni.ico failed");
    }
}
