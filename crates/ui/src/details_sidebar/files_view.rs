use std::{collections::HashMap, sync::OnceLock};

use gpui::SharedString;
use serde::Deserialize;

use crate::details_sidebar::file_tree::FileNode;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaterialIconManifest {
    file_names: HashMap<String, String>,
    file_extensions: HashMap<String, String>,
    folder_names: HashMap<String, String>,
    folder_names_expanded: HashMap<String, String>,
    default_icon: String,
    default_folder_icon: String,
    default_folder_open_icon: String,
}

fn manifest() -> &'static MaterialIconManifest {
    static MANIFEST: OnceLock<MaterialIconManifest> = OnceLock::new();
    MANIFEST.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/file-icons/manifest.json"))
            .expect("bundled Material Icon Theme manifest is valid")
    })
}

pub fn material_icon_name(file_name: &str, is_directory: bool, is_open: bool) -> &'static str {
    let manifest = manifest();
    if is_directory {
        let name = file_name.to_lowercase();
        let resolved = if is_open {
            manifest
                .folder_names_expanded
                .get(&name)
                .or_else(|| manifest.folder_names.get(&name))
                .unwrap_or(&manifest.default_folder_open_icon)
        } else {
            manifest
                .folder_names
                .get(&name)
                .unwrap_or(&manifest.default_folder_icon)
        };
        return resolved.as_str();
    }

    if let Some(icon) = manifest
        .file_names
        .get(file_name)
        .or_else(|| manifest.file_names.get(&file_name.to_lowercase()))
    {
        return icon.as_str();
    }

    if let Some(dot_index) = file_name.find('.') {
        let after_first_dot = file_name[dot_index + 1..].to_lowercase();
        let segments: Vec<_> = after_first_dot.split('.').collect();
        for index in 0..segments.len() {
            let extension = segments[index..].join(".");
            if let Some(icon) = manifest.file_extensions.get(&extension) {
                return icon.as_str();
            }
        }
    }

    manifest.default_icon.as_str()
}

pub fn material_icon_path(file_name: &str, is_directory: bool, is_open: bool) -> SharedString {
    format!(
        "file-icons/{}.svg",
        material_icon_name(file_name, is_directory, is_open)
    )
    .into()
}

pub fn file_glyph(node: &FileNode, expanded: bool) -> SharedString {
    material_icon_path(&node.name, node.is_dir, expanded)
}

#[cfg(test)]
mod tests {
    use super::material_icon_name;

    #[test]
    fn material_icon_resolver_matches_orchestrator_reference() {
        assert_eq!(material_icon_name("README.md", false, false), "readme");
        assert_eq!(material_icon_name("package.json", false, false), "nodejs");
        assert_eq!(material_icon_name("main.rs", false, false), "rust");
        assert_eq!(material_icon_name("app.tsx", false, false), "react_ts");
        assert_eq!(material_icon_name("src", true, false), "folder-src");
        assert_eq!(material_icon_name("src", true, true), "folder-src-open");
        assert_eq!(material_icon_name("unknown", true, false), "folder");
        assert_eq!(material_icon_name("unknown", true, true), "folder-open");
    }
}
