use std::{fs, path::PathBuf};

fn main() {
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=WebKit");
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
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

    let output =
        PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("material_file_icon_assets.rs");
    fs::write(output, source).expect("write material icon asset table");
}
