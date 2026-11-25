//! The read port. The core never touches the file system: a repository is a `RepoReader`
//! (a file listing and lazy text reads), several of them form a [`Workspace`] laid out like DOC's
//! assets directory (`<segment>/<path inside the repository>`), over which DOC's `glob`,
//! `scandir`, `exists` and `open` calls are reproduced.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use doc_model::tree::{Tree, TreeData};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// `UnicodeDecodeError` in DOC.
    Decode(String),
    Io(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Decode(m) => write!(f, "decode error: {m}"),
            ReadError::Io(m) => write!(f, "io error: {m}"),
        }
    }
}

/// One cloned repository. `files` are `/`-separated paths relative to the repository root (the
/// `.git` directory excluded), at most three levels deep: the deepest path DOC's globs reach
/// (`<dir>**/**/*.txt` and `<dir>/**/manifest.yaml`).
pub trait RepoReader: Send + Sync {
    fn files(&self) -> Vec<String>;
    /// Directories relative to the root, at most two levels deep (so a directory whose files are
    /// deeper than the listing still exists). Defaults to the parents of `files()`.
    fn dirs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for f in self.files() {
            let mut d = f.as_str();
            while let Some(i) = d.rfind('/') {
                d = &d[..i];
                if !out.iter().any(|x| x == d) {
                    out.push(d.to_string());
                }
            }
        }
        out
    }
    /// Python `open(path).read()`: UTF-8 with universal newlines; `Ok(None)` when the file does
    /// not exist.
    fn read_text(&self, rel: &str) -> Result<Option<String>, ReadError>;
}

/// An in-memory repository (tests and fakes).
#[derive(Clone, Debug, Default)]
pub struct MemRepo {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemRepo {
    pub fn new() -> MemRepo {
        MemRepo::default()
    }
    pub fn with(mut self, path: &str, text: &str) -> MemRepo {
        self.files.insert(path.to_string(), text.as_bytes().to_vec());
        self
    }
    pub fn with_bytes(mut self, path: &str, bytes: &[u8]) -> MemRepo {
        self.files.insert(path.to_string(), bytes.to_vec());
        self
    }
}

/// Python's text-mode decoding: strict UTF-8, `\r\n` and `\r` become `\n`.
pub fn py_decode(bytes: &[u8]) -> Result<String, ReadError> {
    let text = std::str::from_utf8(bytes).map_err(|e| ReadError::Decode(e.to_string()))?;
    if !text.contains('\r') {
        return Ok(text.to_string());
    }
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

impl RepoReader for MemRepo {
    fn files(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }
    fn read_text(&self, rel: &str) -> Result<Option<String>, ReadError> {
        match self.files.get(rel) {
            Some(b) => py_decode(b).map(Some),
            None => Ok(None),
        }
    }
}

/// The repositories of one request, laid out like DOC's assets directory.
pub struct Workspace {
    tree: Tree,
    readers: HashMap<String, Arc<dyn RepoReader>>,
}

impl Workspace {
    /// `entries`: `(directory name, reader)` in clone order. A repeated directory name keeps the
    /// first (DOC clones the first and finds the directory fresh for the second).
    pub fn new(entries: Vec<(String, Arc<dyn RepoReader>)>) -> Workspace {
        let mut tree = Tree::default();
        let mut readers: HashMap<String, Arc<dyn RepoReader>> = HashMap::new();
        for (segment, reader) in entries {
            if readers.contains_key(&segment) {
                continue;
            }
            tree.add_dir(&segment, &TreeData { files: reader.files(), texts: HashMap::new() });
            tree.add_dirs(&segment, &reader.dirs());
            readers.insert(segment, reader);
        }
        Workspace { tree, readers }
    }

    /// Python `glob.glob`, sorted.
    pub fn glob(&self, pattern: &str) -> Vec<String> {
        self.tree.glob(pattern)
    }

    pub fn exists(&self, path: &str) -> bool {
        self.tree.exists(path)
    }

    /// `os.scandir(dir)`: `None` is DOC's `FileNotFoundError`.
    pub fn scandir(&self, dir: &str) -> Option<Vec<(String, bool)>> {
        self.tree.scandir(dir)
    }

    pub fn has_dir(&self, segment: &str) -> bool {
        self.readers.contains_key(segment)
    }

    /// `open(path).read()` for a path `<segment>/<rel>`.
    pub fn read(&self, path: &str) -> Result<Option<String>, ReadError> {
        let Some((segment, rel)) = path.split_once('/') else {
            return Ok(None);
        };
        match self.readers.get(segment) {
            Some(r) => r.read_text(rel),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_glob_double_star_is_one_level() {
        let a: Arc<dyn RepoReader> = Arc::new(MemRepo::new().with("01-GEN.usfm", "\\id GEN").with("sub/x.txt", "t"));
        let b: Arc<dyn RepoReader> = Arc::new(MemRepo::new().with("02-EXO.usfm", "\\id EXO"));
        let ws = Workspace::new(vec![("sw_ulb".into(), a), ("sw_ulb2".into(), b)]);
        // the pattern DOC builds has no separator before `**`, so it also reaches the sibling directory
        assert_eq!(ws.glob("sw_ulb**/*.usfm"), vec!["sw_ulb/01-GEN.usfm", "sw_ulb2/02-EXO.usfm"]);
        assert!(ws.scandir("nope").is_none());
        assert_eq!(ws.read("sw_ulb/01-GEN.usfm").unwrap().as_deref(), Some("\\id GEN"));
    }

    #[test]
    fn universal_newlines() {
        assert_eq!(py_decode(b"a\r\nb\rc").unwrap(), "a\nb\nc");
        assert!(py_decode(&[0xff, 0xfe]).is_err());
    }
}
