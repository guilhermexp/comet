use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=WebKit");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let icons_dir = manifest_dir.join("assets/file-icons");
    println!("cargo:rerun-if-changed={}", icons_dir.display());

    let mut names: Vec<String> = fs::read_dir(&icons_dir)
        .expect("read material icon assets")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.ends_with(".svg").then_some(name)
        })
        .collect();
    names.sort();

    let mut source =
        String::from("pub fn load(path: &str) -> Option<&'static [u8]> {\n    match path {\n");
    for name in names {
        source.push_str(&format!(
            "        \"file-icons/{name}\" => Some(include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/assets/file-icons/{name}\")).as_slice()),\n"
        ));
    }
    source.push_str("        _ => None,\n    }\n}\n");

    let output = PathBuf::from(env::var("OUT_DIR").unwrap()).join("material_file_icon_assets.rs");
    fs::write(output, source).expect("write material icon asset table");

    let avatars_dir = manifest_dir.join("assets/icons/subagents/blobatar");
    println!("cargo:rerun-if-changed={}", avatars_dir.display());
    let mut avatar_names: Vec<String> = fs::read_dir(&avatars_dir)
        .expect("read Blobatar subagent avatar assets")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.ends_with(".svg").then_some(name)
        })
        .collect();
    avatar_names.sort();

    let mut avatar_source =
        String::from("pub fn load(path: &str) -> Option<&'static [u8]> {\n    match path {\n");
    for name in &avatar_names {
        avatar_source.push_str(&format!(
            "        \"icons/subagents/blobatar/{name}\" => Some(include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/assets/icons/subagents/blobatar/{name}\")).as_slice()),\n"
        ));
    }
    avatar_source.push_str("        _ => None,\n    }\n}\n\npub const PATHS: &[&str] = &[\n");
    for name in &avatar_names {
        avatar_source.push_str(&format!("    \"icons/subagents/blobatar/{name}\",\n"));
    }
    avatar_source.push_str("];\n");

    let avatar_output =
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("blobatar_subagent_avatar_assets.rs");
    fs::write(avatar_output, avatar_source).expect("write Blobatar subagent avatar asset table");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=src/dictation/permission.m");
        let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
        let object = out.join("voice-permission.o");
        assert!(
            Command::new("clang")
                .args(["-fobjc-arc", "-c", "src/dictation/permission.m", "-o"])
                .arg(&object)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("ar")
                .arg("crus")
                .arg(out.join("libvoice-permission.a"))
                .arg(object)
                .status()
                .unwrap()
                .success()
        );
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-lib=static=voice-permission");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
    }

    build_linux_browser_helper();
}

fn build_linux_browser_helper() {
    println!("cargo:rerun-if-changed=src/browser/linux/helper.c");
    println!("cargo:rerun-if-changed=src/browser/linux/wpe.c");
    println!("cargo:rerun-if-env-changed=ZERON_WPE");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    build_wpe_helper();
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("zeron-webkit");
    let flags = Command::new("pkg-config")
        .args(["--cflags", "--libs", "webkit2gtk-4.1", "json-glib-1.0"])
        .output()
        .expect("pkg-config is required to build the Linux browser helper");
    assert!(
        flags.status.success(),
        "The Linux browser requires WebKitGTK 4.1 and JSON-GLib development files \
         discoverable by pkg-config (webkit2gtk-4.1 and json-glib-1.0). \
         See docs/reference/linux-browser.md for distribution-specific installation commands.\n{}",
        String::from_utf8_lossy(&flags.stderr)
    );
    let status = Command::new(env::var("CC").unwrap_or_else(|_| "cc".into()))
        .args([
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Wno-unused-parameter",
            "src/browser/linux/helper.c",
            "-o",
        ])
        .arg(&output)
        .args(String::from_utf8(flags.stdout).unwrap().split_whitespace())
        .status()
        .expect("C compiler is required to build the Linux browser helper");
    assert!(status.success(), "Linux browser helper compilation failed");
}

/// The WPE WebKit helper is optional: without WPE WebKit 2.50+ development
/// files, with ones it does not compile against (an older GBM), or with
/// `ZERON_WPE=0`, the build embeds an empty helper and the browser uses
/// WebKitGTK.
fn build_wpe_helper() {
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("zeron-wpe");
    let packages = [
        "wpe-webkit-2.0 >= 2.50",
        "wpe-platform-2.0",
        "json-glib-1.0",
        "gbm",
        "egl",
        "glesv2",
        "xkbcommon",
    ];
    let flags = (env::var("ZERON_WPE").as_deref() != Ok("0"))
        .then(|| {
            Command::new("pkg-config")
                .arg("--cflags")
                .arg("--libs")
                .args(packages)
                .output()
                .ok()
        })
        .flatten()
        .filter(|flags| flags.status.success());
    let Some(flags) = flags else {
        println!(
            "cargo:warning=WPE WebKit 2.50+ development files not found; the Linux browser will use WebKitGTK"
        );
        std::fs::write(&output, []).unwrap();
        return;
    };
    let status = Command::new(env::var("CC").unwrap_or_else(|_| "cc".into()))
        .args([
            "-std=gnu11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Wno-unused-parameter",
            "src/browser/linux/wpe.c",
            "-o",
        ])
        .arg(&output)
        .args(String::from_utf8(flags.stdout).unwrap().split_whitespace())
        .arg("-lm")
        .status()
        .expect("C compiler is required to build the Linux browser helper");
    if !status.success() {
        println!(
            "cargo:warning=WPE WebKit helper compilation failed; the Linux browser will use WebKitGTK"
        );
        std::fs::write(&output, []).unwrap();
    }
}
