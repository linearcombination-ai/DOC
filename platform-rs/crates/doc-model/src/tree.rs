//! C5 `tree`: the resources as a read-only map of paths to text, so the readers do no I/O.
//!
//! Paths are relative to DOC's assets directory (`RESOURCE_ASSETS_DIR`), `/`-separated,
//! exactly the layout DOC's `glob`/`open`/`exists` calls see. Text is what Python's
//! `open(path, "r")` returns (UTF-8, universal newlines): the shell decodes.

use std::collections::{BTreeSet, HashMap};

use serde::Deserialize;

/// Shell-provided listing: every file path, and the text of the `.md`/`.txt` files
/// (`null` = the file did not decode; DOC would raise when it reads it).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TreeData {
    pub files: Vec<String>,
    pub texts: HashMap<String, Option<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct Tree {
    files: BTreeSet<String>,
    dirs: BTreeSet<String>,
    texts: HashMap<String, Option<String>>,
}

impl Tree {
    /// Merge `data` under the directory `prefix` (e.g. `en_tn`).
    pub fn add_dir(&mut self, prefix: &str, data: &TreeData) {
        if !prefix.is_empty() {
            self.dirs.insert(prefix.to_string());
        }
        let join = |k: &str| if prefix.is_empty() { k.to_string() } else { format!("{prefix}/{k}") };
        for f in &data.files {
            let full = join(f);
            let mut d = full.as_str();
            while let Some(i) = d.rfind('/') {
                d = &d[..i];
                if !self.dirs.insert(d.to_string()) {
                    break;
                }
            }
            self.files.insert(full);
        }
        for (k, v) in &data.texts {
            self.texts.insert(join(k), v.clone());
        }
    }

    /// `os.path.exists`.
    pub fn exists(&self, path: &str) -> bool {
        self.files.contains(path) || self.dirs.contains(path)
    }

    /// Register directories (relative to `prefix`) that hold no listed file, so `scandir` and
    /// `exists` see them (P4 addition).
    pub fn add_dirs(&mut self, prefix: &str, dirs: &[String]) {
        for d in dirs {
            let mut full = if prefix.is_empty() { d.clone() } else { format!("{prefix}/{d}") };
            loop {
                if !self.dirs.insert(full.clone()) {
                    break;
                }
                match full.rfind('/') {
                    Some(i) => full.truncate(i),
                    None => break,
                }
            }
        }
    }

    /// `os.path.isdir` (P4 addition).
    pub fn is_dir(&self, path: &str) -> bool {
        self.dirs.contains(path)
    }

    /// `os.scandir(dir)`: `(name, is_dir)` for every entry, hidden ones included, sorted by name
    /// (Python's order is arbitrary); `None` when `dir` is not a directory, where Python raises
    /// `FileNotFoundError` (P4 addition).
    pub fn scandir(&self, dir: &str) -> Option<Vec<(String, bool)>> {
        if !dir.is_empty() && !self.dirs.contains(dir) {
            return None;
        }
        let base = dir.trim_end_matches('/');
        Some(
            self.children(base)
                .into_iter()
                .map(|name| (name.to_string(), self.dirs.contains(&join(base, name))))
                .collect(),
        )
    }

    /// `open(path).read()`: `Ok(None)` is "file missing", `Err` is a decode failure.
    pub fn read(&self, path: &str) -> Result<Option<&str>, String> {
        match self.texts.get(path) {
            Some(Some(t)) => Ok(Some(t.as_str())),
            Some(None) => Err(format!("UnicodeDecodeError: {path}")),
            None => Ok(None),
        }
    }

    fn children(&self, dir: &str) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        for set in [&self.dirs, &self.files] {
            for p in set.range(prefix.clone()..) {
                if !p.starts_with(&prefix) {
                    break;
                }
                let rest = &p[prefix.len()..];
                if !rest.is_empty() && !rest.contains('/') {
                    out.push(rest);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Python `glob.glob(pattern)` (no `recursive`: `**` behaves like `*`), results sorted
    /// (DOC sorts nearly every result; where it does not, directory order is arbitrary).
    pub fn glob(&self, pattern: &str) -> Vec<String> {
        let segments: Vec<&str> = pattern.split('/').collect();
        let mut current: Vec<String> = vec![String::new()];
        for (idx, seg) in segments.iter().enumerate() {
            let last = idx + 1 == segments.len();
            let mut next = Vec::new();
            for base in &current {
                if !has_magic(seg) {
                    let cand = join(base, seg);
                    if (last && self.exists(&cand)) || (!last && self.dirs.contains(&cand)) {
                        next.push(cand);
                    }
                    continue;
                }
                for name in self.children(base) {
                    if name.starts_with('.') && !seg.starts_with('.') {
                        continue;
                    }
                    if fnmatch(seg, name) {
                        let cand = join(base, name);
                        if last || self.dirs.contains(&cand) {
                            next.push(cand);
                        }
                    }
                }
            }
            current = next;
        }
        current.sort();
        current.dedup();
        current
    }
}

fn join(base: &str, seg: &str) -> String {
    if base.is_empty() {
        seg.to_string()
    } else {
        format!("{base}/{seg}")
    }
}

fn has_magic(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// `fnmatch.fnmatchcase` for one path segment.
pub fn fnmatch(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    fn rec(p: &[char], n: &[char]) -> bool {
        match p.first() {
            None => n.is_empty(),
            Some('*') => (0..=n.len()).any(|k| rec(&p[1..], &n[k..])),
            Some('?') => !n.is_empty() && rec(&p[1..], &n[1..]),
            Some('[') => {
                // find the closing bracket (a `]` right after `[` or `[!` is a member)
                let mut j = 1;
                if j < p.len() && p[j] == '!' {
                    j += 1;
                }
                if j < p.len() && p[j] == ']' {
                    j += 1;
                }
                while j < p.len() && p[j] != ']' {
                    j += 1;
                }
                if j >= p.len() {
                    return !n.is_empty() && n[0] == '[' && rec(&p[1..], &n[1..]);
                }
                if n.is_empty() {
                    return false;
                }
                let mut set = &p[1..j];
                let negate = set.first() == Some(&'!');
                if negate {
                    set = &set[1..];
                }
                let c = n[0];
                let mut hit = false;
                let mut k = 0;
                while k < set.len() {
                    if k + 2 < set.len() && set[k + 1] == '-' {
                        if set[k] <= c && c <= set[k + 2] {
                            hit = true;
                        }
                        k += 3;
                    } else {
                        if set[k] == c {
                            hit = true;
                        }
                        k += 1;
                    }
                }
                hit != negate && rec(&p[j + 1..], &n[1..])
            }
            Some(c) => !n.is_empty() && n[0] == *c && rec(&p[1..], &n[1..]),
        }
    }
    rec(&p, &n)
}

/// `Path(p).name`.
pub fn path_name(p: &str) -> &str {
    p.trim_end_matches('/').rsplit('/').next().unwrap_or(p)
}

/// `Path(p).stem`.
pub fn path_stem(p: &str) -> &str {
    let name = path_name(p);
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// `os.path.join` for relative parts (an absolute later part restarts the path).
pub fn py_join(parts: &[&str]) -> String {
    let mut out = String::new();
    for part in parts {
        if part.starts_with('/') {
            out = (*part).to_string();
        } else if out.is_empty() || out.ends_with('/') {
            out.push_str(part);
        } else {
            out.push('/');
            out.push_str(part);
        }
    }
    out
}
