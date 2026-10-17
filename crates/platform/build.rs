fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/macos.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .flag("-mmacosx-version-min=14.0")
            .compile("yurelay_native");
        for framework in [
            "AppKit",
            "Foundation",
            "ApplicationServices",
            "Vision",
            "NaturalLanguage",
            "ScreenCaptureKit",
            "Security",
            "CoreServices",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rerun-if-changed=native/macos.m");
    }
}
