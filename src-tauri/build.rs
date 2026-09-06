fn main() {
    tauri_build::build();

    // Test harness executables get NO embedded manifest (tauri-build only
    // covers the shipped binaries). Without a Common-Controls v6 activation
    // context, comctl32.dll resolves to the ancient v5 copy and any code
    // path that pulls in TaskDialogIndirect (tauri/tao/muda objects) makes
    // the whole test binary fail to load with STATUS_ENTRYPOINT_NOT_FOUND.
    // Embed a minimal manifest into every test executable so the linker-
    // imported functions resolve no matter which test drags them in.
    const MANIFEST: &str = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        "\n",
        r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">"#,
        "\n",
        r#"  <dependency>"#,
        "\n",
        r#"    <dependentAssembly>"#,
        "\n",
        r#"      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls""#,
        "\n",
        r#"        version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />"#,
        "\n",
        r#"    </dependentAssembly>"#,
        "\n",
        r#"  </dependency>"#,
        "\n",
        r#"</assembly>"#,
        "\n"
    );
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let manifest_path = std::path::Path::new(&out_dir).join("test-harness.manifest");
    std::fs::write(&manifest_path, MANIFEST).expect("write test manifest");
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest_path.display()
    );
}
