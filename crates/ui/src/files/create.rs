use std::path::Path;

use zeron_proto::{sibling_name_taken, validate_workspace_create_name};

pub fn create_parent_path(selected: Option<&str>, selected_is_dir: bool) -> String {
    match selected {
        None | Some("") => String::new(),
        Some(path) if selected_is_dir => path.to_string(),
        Some(path) => Path::new(path)
            .parent()
            .and_then(|parent| parent.to_str())
            .filter(|parent| !parent.is_empty())
            .unwrap_or("")
            .to_string(),
    }
}

pub fn validate_create_name(name: &str, sibling_names: &[&str]) -> Result<(), String> {
    validate_workspace_create_name(name).map_err(|error| error.as_str().to_string())?;
    let leaf = name.rsplit('/').next().unwrap_or(name);
    if sibling_name_taken(sibling_names.iter().copied(), leaf) {
        return Err("an entry with that name already exists".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_validation_rejects_empty_dot_and_collision() {
        assert_eq!(
            validate_create_name("", &[]).unwrap_err(),
            "name must not be empty"
        );
        assert_eq!(
            validate_create_name("..", &[]).unwrap_err(),
            "name contains an invalid component"
        );
        assert_eq!(
            validate_create_name("a.txt", &["A.TXT"]).unwrap_err(),
            "an entry with that name already exists"
        );
        assert_eq!(
            validate_create_name("CON", &[]).unwrap_err(),
            "name contains an invalid component"
        );
        assert_eq!(validate_create_name("docs/adr/0001.md", &[]), Ok(()));
    }

    #[test]
    fn create_parent_falls_back_from_file_to_its_directory() {
        assert_eq!(create_parent_path(Some("src"), true), "src");
        assert_eq!(create_parent_path(Some("src/a.rs"), false), "src");
        assert_eq!(create_parent_path(None, false), "");
    }
}
