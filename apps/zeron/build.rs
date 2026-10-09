fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../dist/macos/Info-unbundled.plist");
        println!("cargo:rerun-if-changed={}", plist.display());
        // Unbundled executable launches can also request microphone access.
        // NSBundle reads privacy declarations from this Mach-O section;
        // the macOS development runner still uses the fork's app bundle.
        println!(
            "cargo:rustc-link-arg-bin=zeron=-Wl,-sectcreate,__TEXT,__info_plist,{}",
            plist.display()
        );
    }
    println!("cargo:rerun-if-changed=../../dist/windows/zeron.rc");
    println!("cargo:rerun-if-changed=../../dist/windows/zeron.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for(
            "../../dist/windows/zeron.rc",
            &["zeron"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("Windows app icon resource compilation failed");
    }
}
