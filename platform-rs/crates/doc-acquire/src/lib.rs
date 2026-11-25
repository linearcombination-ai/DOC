//! Repository acquisition behind the `Acquirer` port.
//!
//! Why the `git` command line and not `git2`: a clone must be killable. A subprocess can be killed
//! on timeout (with its process group, so `git-remote-https` goes too); a libgit2 clone runs in a
//! blocking thread that cannot be interrupted except through progress callbacks. The command line
//! is also exactly what DOC runs (`git -c http.userAgent=... clone --depth=1 --single-branch`), needs
//! no C library build, and a fake `git` executable makes the clone failures testable.
//!
//! Layout: `<cache>/<sha256(url)[..16]>/` holds `v-<id>/` working trees, `v-<id>.fetched` marker
//! files (the mtime is the fetch time) and a `current` symlink to the live version. A refresh clones
//! into `tmp-<id>/`, renames it to `v-<id>/`, writes the marker and flips `current` with an atomic
//! `rename` of a new symlink, so a reader holding the old path keeps a consistent tree; old
//! versions are removed after a grace period.

mod disk;
mod git;

pub use disk::DiskRepo;
pub use git::{GitAcquirer, GitConfig};
