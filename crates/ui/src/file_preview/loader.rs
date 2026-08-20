use std::{
    fs,
    path::{Component, Path},
};

use calamine::{Data, Range, Reader, open_workbook_auto};

use super::model::{PreviewKind, classify_preview_kind};

const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedPreview {
    Text(String),
    Binary(Vec<u8>),
    Table(Vec<Vec<String>>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewLoadError {
    OutsideCheckout,
    Missing,
    TooLarge,
    InvalidUtf8,
    Io(String),
}

pub fn load_preview(root: &Path, relative_path: &Path) -> Result<LoadedPreview, PreviewLoadError> {
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || !relative_path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(PreviewLoadError::OutsideCheckout);
    }
    let root = root.canonicalize().map_err(|_| PreviewLoadError::Missing)?;
    let path = root
        .join(relative_path)
        .canonicalize()
        .map_err(|_| PreviewLoadError::Missing)?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(PreviewLoadError::OutsideCheckout);
    }
    let kind = classify_preview_kind(path.to_string_lossy().as_ref());
    if kind == PreviewKind::Unsupported {
        return Ok(LoadedPreview::Unsupported);
    }
    let metadata = fs::metadata(&path).map_err(|error| PreviewLoadError::Io(error.to_string()))?;
    let binary = matches!(kind, PreviewKind::Image | PreviewKind::Pdf)
        || matches!(kind, PreviewKind::Data)
            && !matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("csv" | "tsv")
            );
    let limit = if binary {
        MAX_BINARY_BYTES
    } else {
        MAX_TEXT_BYTES
    };
    if metadata.len() > limit {
        return Err(PreviewLoadError::TooLarge);
    }
    if kind == PreviewKind::Data && binary {
        let mut workbook =
            open_workbook_auto(&path).map_err(|error| PreviewLoadError::Io(error.to_string()))?;
        let sheet = workbook
            .sheet_names()
            .first()
            .cloned()
            .ok_or_else(|| PreviewLoadError::Io("workbook has no worksheets".into()))?;
        let range = workbook
            .worksheet_range(&sheet)
            .map_err(|error| PreviewLoadError::Io(error.to_string()))?;
        return Ok(LoadedPreview::Table(workbook_rows(&range, 2_000, 100)));
    }
    let bytes = fs::read(path).map_err(|error| PreviewLoadError::Io(error.to_string()))?;
    if binary {
        Ok(LoadedPreview::Binary(bytes))
    } else {
        String::from_utf8(bytes)
            .map(LoadedPreview::Text)
            .map_err(|_| PreviewLoadError::InvalidUtf8)
    }
}

pub fn workbook_rows(range: &Range<Data>, max_rows: usize, max_columns: usize) -> Vec<Vec<String>> {
    range
        .rows()
        .take(max_rows)
        .map(|row| {
            row.iter()
                .take(max_columns)
                .map(ToString::to_string)
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::{LoadedPreview, PreviewLoadError, load_preview};
    use calamine::{Data, Range};

    #[test]
    fn reads_utf8_inside_checkout() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("README.md"), "# Hello").unwrap();
        assert_eq!(
            load_preview(temp.path(), Path::new("README.md")).unwrap(),
            LoadedPreview::Text("# Hello".into())
        );
    }

    #[test]
    fn rejects_parent_traversal_and_external_symlink() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            load_preview(temp.path(), Path::new("../secret")),
            Err(PreviewLoadError::OutsideCheckout)
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hosts", temp.path().join("hosts")).unwrap();
            assert_eq!(
                load_preview(temp.path(), Path::new("hosts")),
                Err(PreviewLoadError::OutsideCheckout)
            );
        }
    }

    #[test]
    fn rejects_oversized_text_before_allocating_view_state() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("large.txt"),
            vec![b'x'; 4 * 1024 * 1024 + 1],
        )
        .unwrap();
        assert_eq!(
            load_preview(temp.path(), Path::new("large.txt")),
            Err(PreviewLoadError::TooLarge)
        );
    }

    #[test]
    fn workbook_rows_are_bounded_and_render_cell_display_values() {
        let mut range = Range::new((0, 0), (1, 1));
        range.set_value((0, 0), Data::String("Name".into()));
        range.set_value((0, 1), Data::String("Value".into()));
        range.set_value((1, 0), Data::String("Total".into()));
        range.set_value((1, 1), Data::Float(42.5));
        assert_eq!(
            super::workbook_rows(&range, 2, 2),
            vec![
                vec!["Name".to_string(), "Value".to_string()],
                vec!["Total".to_string(), "42.5".to_string()],
            ]
        );
    }
}
