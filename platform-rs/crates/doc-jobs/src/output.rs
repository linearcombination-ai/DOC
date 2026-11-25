//! Output placement and scratch space. Scratch lives under the output directory so the final move is a
//! same-filesystem `rename`, which is atomic: a reader sees the old file, no file, or the whole new file.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rand::RngExt;
use tokio::io::AsyncWriteExt;

use crate::model::JobError;
use crate::seam::validate_docx_file;

#[derive(Clone, Debug)]
pub struct Dirs {
    pub output: PathBuf,
}

impl Dirs {
    pub fn new(output: PathBuf) -> Dirs {
        Dirs { output }
    }
    pub fn docx_path(&self, key: &str) -> PathBuf {
        self.output.join(format!("{key}.docx"))
    }
    pub fn html_path(&self, key: &str) -> PathBuf {
        self.output.join(format!("{key}.html"))
    }
    pub fn pdf_path(&self, key: &str) -> PathBuf {
        self.output.join(format!("{key}.pdf"))
    }
    pub fn path_for(&self, kind: &str, key: &str) -> PathBuf {
        if kind == "html" { self.html_path(key) } else { self.docx_path(key) }
    }
    pub fn findings_path(&self, key: &str) -> PathBuf {
        self.output.join(format!("{key}.findings.json"))
    }
    fn scratch_root(&self) -> PathBuf {
        self.output.join(".scratch")
    }

    /// A directory no other job shares: task id, attempt and a random suffix. (Celery's shared
    /// `working_temp` names files by book and resource, so two requests for the same book collide.)
    pub fn new_scratch(&self, task_id: &str, attempt: u32) -> Result<Scratch, JobError> {
        let suffix: u32 = rand::rng().random();
        let path = self.scratch_root().join(format!("{task_id}-a{attempt}-{suffix:08x}"));
        std::fs::create_dir_all(&path).map_err(|e| JobError::transient("scratch_io", format!("{}: {e}", path.display())))?;
        Ok(Scratch { path })
    }

    /// Earlier attempts of this task are dead (their lease was lost or they failed): their scratch goes.
    pub fn remove_stale_attempts(&self, task_id: &str, attempt: u32) -> usize {
        let Ok(rd) = std::fs::read_dir(self.scratch_root()) else { return 0 };
        let mine = format!("{task_id}-a{attempt}-");
        let mut n = 0;
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("{task_id}-a")) && !name.starts_with(&mine) && std::fs::remove_dir_all(e.path()).is_ok() {
                n += 1;
            }
        }
        n
    }

    /// At worker start: remove scratch directories a dead worker left behind (older than `older_than`).
    pub fn sweep_scratch(&self, older_than: Duration) -> usize {
        let Ok(rd) = std::fs::read_dir(self.scratch_root()) else { return 0 };
        let mut n = 0;
        for e in rd.flatten() {
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age >= older_than);
            if old && std::fs::remove_dir_all(e.path()).is_ok() {
                n += 1;
            }
        }
        n
    }

    /// DOC's `file_needs_update`, tightened: the file must exist, be younger than `max_age`, and be a
    /// whole zip (a torn or empty file from an earlier crash is regenerated, not served).
    pub async fn fresh_docx(&self, key: &str, max_age: Duration) -> bool {
        self.fresh("docx", key, max_age).await
    }

    /// The same for either output kind; an HTML file must end with the closing tags DOC's template writes.
    pub async fn fresh(&self, kind: &str, key: &str, max_age: Duration) -> bool {
        let path = self.path_for(kind, key);
        let Ok(meta) = tokio::fs::metadata(&path).await else { return false };
        let young = meta.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age < max_age);
        if !young {
            return false;
        }
        if kind == "html" {
            return tokio::fs::read(&path).await.is_ok_and(|b| b.ends_with(b"</body></html>"));
        }
        if kind == "pdf" {
            return crate::pdf::validate_pdf_file(&path).await.is_ok();
        }
        validate_docx_file(&path).await.is_ok()
    }

    /// Free bytes on the output filesystem (`statvfs`), or `None` when it cannot be read.
    pub fn free_bytes(&self) -> Option<u64> {
        let st = nix::sys::statvfs::statvfs(&self.output).ok().or_else(|| nix::sys::statvfs::statvfs(self.output.parent()?).ok())?;
        Some(u64::from(st.blocks_available()).saturating_mul(u64::from(st.fragment_size())))
    }

    /// Remove finished outputs (documents, findings sidecars) older than `older_than`: DOC's cache expiry. A file
    /// still younger than the freshness window is served again by the cache check, so `older_than` must be at least
    /// that window; the caller adds a grace so a user still downloading a document keeps it. Returns the number removed.
    pub fn expire_outputs(&self, older_than: Duration) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.output) else { return 0 };
        let mut n = 0;
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let ours = name.ends_with(".docx") || name.ends_with(".html") || name.ends_with(".pdf") || name.ends_with(".findings.json") || name.ends_with(".parts.json");
            if !ours || !e.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age >= older_than);
            if old && std::fs::remove_file(e.path()).is_ok() {
                n += 1;
            }
        }
        n
    }
}

pub struct Scratch {
    pub path: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// fsync `src`, rename it to `dst`, fsync the directory: after this returns the document is durable and
/// complete under its final name.
pub async fn place_atomic(src: &Path, dst: &Path) -> Result<(), JobError> {
    let io = |what: &str, e: std::io::Error| JobError::transient("output_io", format!("{what} {}: {e}", dst.display()));
    let f = tokio::fs::OpenOptions::new().write(true).open(src).await.map_err(|e| io("open", e))?;
    f.sync_all().await.map_err(|e| io("fsync", e))?;
    drop(f);
    tokio::fs::rename(src, dst).await.map_err(|e| io("rename", e))?;
    if let Some(dir) = dst.parent() {
        if let Ok(d) = tokio::fs::File::open(dir).await {
            let _ = d.sync_all().await;
        }
    }
    Ok(())
}

/// Write `bytes` to `dst` through a scratch file in the same directory, then rename.
pub async fn write_atomic(dst: &Path, bytes: &[u8]) -> Result<(), JobError> {
    let suffix: u32 = rand::rng().random();
    let tmp = dst.with_extension(format!("tmp-{suffix:08x}"));
    let io = |e: std::io::Error| JobError::transient("output_io", format!("{}: {e}", tmp.display()));
    let mut f = tokio::fs::File::create(&tmp).await.map_err(io)?;
    f.write_all(bytes).await.map_err(io)?;
    drop(f);
    place_atomic(&tmp, dst).await
}
