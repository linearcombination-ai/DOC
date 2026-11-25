//! A cloned repository on disk as a `RepoReader`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use doc_lookup::reader::{py_decode, ReadError, RepoReader};

/// Files up to three levels deep and directories up to two, `.git` excluded: the deepest DOC's
/// globs (`<dir>**/**/*.txt`, `<dir>/**/manifest.yaml`) and `scandir` calls reach.
pub struct DiskRepo {
    root: PathBuf,
    /// The listing is computed once per version directory: a version never changes after the swap.
    listing: OnceLock<(Vec<String>, Vec<String>)>,
}

impl DiskRepo {
    pub fn new(root: PathBuf) -> DiskRepo {
        DiskRepo { root, listing: OnceLock::new() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn walk(&self) -> (Vec<String>, Vec<String>) {
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        let mut stack: Vec<(PathBuf, String, usize)> = vec![(self.root.clone(), String::new(), 0)];
        while let Some((dir, rel, depth)) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == ".git" {
                    continue;
                }
                let rel_path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    if depth < 2 {
                        dirs.push(rel_path.clone());
                        stack.push((entry.path(), rel_path, depth + 1));
                    }
                } else if depth < 3 {
                    files.push(rel_path);
                }
            }
        }
        files.sort();
        dirs.sort();
        (files, dirs)
    }
}

impl RepoReader for DiskRepo {
    fn files(&self) -> Vec<String> {
        self.listing.get_or_init(|| self.walk()).0.clone()
    }
    fn dirs(&self) -> Vec<String> {
        self.listing.get_or_init(|| self.walk()).1.clone()
    }
    fn read_text(&self, rel: &str) -> Result<Option<String>, ReadError> {
        if rel.split('/').any(|c| c == "..") {
            return Ok(None);
        }
        match fs::read(self.root.join(rel)) {
            Ok(bytes) => py_decode(&bytes).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ReadError::Io(format!("{rel}: {e}"))),
        }
    }
}
