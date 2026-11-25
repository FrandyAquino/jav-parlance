fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=src/macos/native.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return Ok(());
    }
    let mut build = cc::Build::new();
    build.file("src/macos/native.m").flag("-fobjc-arc").flag("-fblocks").flag("-mmacosx-version-min=13.3");
    // Rust links with -nodefaultlibs; Clang's @available runtime is otherwise
    // missing from optimized builds that also include Metal availability checks.
    let runtime = build.get_compiler().to_command().arg("--print-runtime-dir").output()?;
    let directory = String::from_utf8(runtime.stdout)?;
    let directory = directory.trim();
    if !runtime.status.success() || !std::path::Path::new(directory).join("libclang_rt.osx.a").is_file() {
        return Err("Xcode's libclang_rt.osx.a was not found; check xcode-select -p".into());
    }
    build.compile("parlance_capture_macos");
    println!("cargo:rustc-link-search=native={directory}");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
    for framework in ["Foundation", "AppKit", "AVFoundation", "CoreAudio", "CoreMedia", "ScreenCaptureKit"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    Ok(())
}
