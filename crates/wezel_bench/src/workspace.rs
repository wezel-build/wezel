//! `Workspace` — explicit per-invocation state.
//!
//! Bundles the project directory, the local plugin store, and the loaded
//! project config. Wezel is moot without a config, so `Workspace::discover`
//! fails when one isn't found.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::ProjectConfig;

#[derive(Debug)]
pub struct Workspace {
    pub project_dir: PathBuf,
    /// Where forager binaries live. Tests pass a tempdir; the CLI passes
    /// the dir of the running wezel binary.
    pub plugin_dir: PathBuf,
    pub config: ProjectConfig,
}

impl Workspace {
    /// Load `.wezel/config.toml` from `project_dir` and pair it with the
    /// caller-chosen plugin store directory.
    pub fn discover(project_dir: PathBuf, plugin_dir: PathBuf) -> Result<Self> {
        let canonical_project_dir = std::fs::canonicalize(&project_dir)?;
        let config = ProjectConfig::load(&canonical_project_dir)?;
        Ok(Self {
            project_dir,
            plugin_dir,
            config,
        })
    }

    /// Resolve the absolute path of a forager binary in the local store, or
    /// `None` if it isn't installed.
    pub fn resolve_plugin(&self, forager: &str) -> Option<PathBuf> {
        let candidate = self.plugin_dir.join(format!("forager-{forager}"));
        candidate.is_file().then_some(candidate)
    }

    /// Path to the cached `--schema` JSON sidecar for a forager. Written by
    /// the installer; read by lint so we don't shell out per-invocation.
    pub fn schema_path(&self, forager: &str) -> PathBuf {
        self.plugin_dir
            .join(format!("forager-{forager}.schema.json"))
    }

    /// Path to the project-scoped editor schema bundle. Rewritten on every
    /// `wezel project tool sync`; each experiment.toml references it via `#:schema`.
    pub fn bundle_schema_path(&self) -> PathBuf {
        self.project_dir.join(".wezel").join("schema.json")
    }

    /// Default plugin store: the directory containing the running wezel
    /// binary. Used by the CLI; tests should pass a tempdir to `discover`
    /// directly.
    pub fn default_plugin_dir() -> Result<PathBuf> {
        std::env::current_exe()
            .context("locating current exe")?
            .parent()
            .map(|p| p.to_path_buf())
            .context("current exe has no parent directory")
    }
}

/// Per-run isolated checkout. Foragers run inside this directory so a build's
/// `target/` never leaks into the user's working tree and step patches don't
/// touch tracked files in-place.
///
/// The whole git repository is cloned, even when the project's `.wezel` lives
/// in a subdirectory (e.g. a crate inside a workspace): [`Scratch::path`] is
/// the cloned repo root, while [`Scratch::project_dir`] is the project dir
/// within it. Git operations (clone, patch, snapshot) act on the repo root;
/// foragers run from `project_dir`.
pub struct Scratch {
    dir: tempfile::TempDir,
    /// Project dir relative to the cloned repo root. Empty when the project
    /// lives at the repo root.
    rel: PathBuf,
}

impl Scratch {
    /// Clone the git repository containing `source` into a fresh tempdir and
    /// check out `commit_sha` detached. Local clones use git's default
    /// hardlinking when possible, so this is much cheaper than a network clone.
    ///
    /// `source` is the project dir (where `.wezel` lives); the whole enclosing
    /// repo is cloned so workspace builds resolve. Measures exactly the
    /// committed `commit_sha` — the caller's uncommitted changes are not
    /// carried over. Use [`Scratch::create_with_worktree`] when measuring the
    /// current checkout's working tree.
    pub fn create(source: &Path, commit_sha: &str) -> Result<Self> {
        let toplevel = crate::git::toplevel(source)?;
        let rel = project_rel(source, &toplevel)?;

        let dir = tempfile::Builder::new()
            .prefix("wezel-scratch-")
            .tempdir()
            .context("creating scratch tempdir")?;

        let status = Command::new("git")
            .arg("clone")
            .arg("--local")
            .arg("--no-checkout")
            .arg(&toplevel)
            .arg(dir.path())
            .status()
            .context("spawning git clone")?;
        if !status.success() {
            bail!("git clone into {} failed", dir.path().display());
        }

        let status = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .arg("checkout")
            .arg("--detach")
            .arg(commit_sha)
            .status()
            .context("spawning git checkout")?;
        if !status.success() {
            bail!("git checkout {commit_sha} in scratch failed");
        }

        Ok(Self { dir, rel })
    }

    /// Like [`Scratch::create`], but after checking out `commit_sha` it overlays
    /// the repo's uncommitted working-tree state onto the clone: modified and
    /// untracked files (respecting `.gitignore`, so build dirs like `target/`
    /// stay out) are copied in, and files deleted from the worktree are removed.
    ///
    /// This lets `wezel experiment run` measure local edits without first
    /// committing them. `commit_sha` should be the source's current `HEAD`, so
    /// the overlaid diff lines up with the checked-out tree.
    pub fn create_with_worktree(source: &Path, commit_sha: &str) -> Result<Self> {
        let toplevel = crate::git::toplevel(source)?;
        let scratch = Self::create(source, commit_sha)?;
        // Overlay from the repo root so changes anywhere in the repo are
        // reflected and `git status` paths stay repo-root-relative.
        overlay_worktree(&toplevel, scratch.path())?;
        Ok(scratch)
    }

    /// Root of the cloned repository. Git operations and snapshots act here.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The project directory (where `.wezel` lives) within the clone. Foragers
    /// run from here. Equals [`Scratch::path`] for repo-root projects.
    pub fn project_dir(&self) -> PathBuf {
        self.dir.path().join(&self.rel)
    }
}

/// Copy `args[..n-1]` to `args[n-1]` (cp's CLI shape) with the fcp engine:
/// parallel, CoW where the filesystem supports it, and timestamp-preserving —
/// mtimes must survive snapshot/restore or cargo-style staleness checks would
/// see every sampled iteration as dirty.
fn fast_copy<P: AsRef<Path>>(args: &[P]) -> Result<()> {
    let args = args
        .iter()
        .map(|p| {
            let p = p.as_ref();
            p.to_str()
                .map(str::to_owned)
                .with_context(|| format!("non-UTF-8 path {}", p.display()))
        })
        .collect::<Result<Vec<_>>>()?;
    // fcp's Error intentionally doesn't implement std::error::Error; carry
    // its message (one line per failed entry) over to anyhow.
    fcp::fcp(&args).map_err(|err| anyhow::anyhow!("{err}"))
}

/// Path of `project_dir` relative to its repo `toplevel`. Both are canonicalized
/// first so a symlinked `project_dir` (e.g. macOS `/var` → `/private/var`) still
/// strips cleanly.
fn project_rel(project_dir: &Path, toplevel: &Path) -> Result<PathBuf> {
    let proj = std::fs::canonicalize(project_dir)
        .with_context(|| format!("canonicalizing {}", project_dir.display()))?;
    let top = std::fs::canonicalize(toplevel)
        .with_context(|| format!("canonicalizing {}", toplevel.display()))?;
    let rel = proj.strip_prefix(&top).with_context(|| {
        format!(
            "project dir {} is not inside repo root {}",
            proj.display(),
            top.display()
        )
    })?;
    Ok(rel.to_path_buf())
}

/// Copy `source`'s uncommitted working-tree changes onto an already
/// checked-out `dest` clone. See [`Scratch::create_with_worktree`].
fn overlay_worktree(source: &Path, dest: &Path) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    // `--untracked-files=all` lists untracked files individually (not their
    // parent dir) and respects `.gitignore`; `--no-renames` keeps each record a
    // single path so the NUL-separated parse stays simple.
    let out = Command::new("git")
        .arg("-C")
        .arg(source)
        .args([
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ])
        .output()
        .context("running git status for worktree overlay")?;
    if !out.status.success() {
        bail!("git status for worktree overlay failed");
    }

    // Each `-z` record is `XY <path>`: two status columns, a space, then the
    // repo-relative path, terminated by NUL.
    for record in out.stdout.split(|&b| b == 0) {
        if record.len() < 4 {
            continue; // trailing empty record
        }
        let rel = Path::new(OsStr::from_bytes(&record[3..]));
        let src = source.join(rel);
        let dst = dest.join(rel);

        // `symlink_metadata` so a dangling symlink still counts as present.
        if src.symlink_metadata().is_ok() {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            // fcp can't overwrite an existing destination symlink, so clear
            // whatever the clone has at this path before copying.
            if let Ok(meta) = dst.symlink_metadata() {
                if meta.is_dir() {
                    std::fs::remove_dir_all(&dst)
                        .with_context(|| format!("removing {}", dst.display()))?;
                } else {
                    std::fs::remove_file(&dst)
                        .with_context(|| format!("removing {}", dst.display()))?;
                }
            }
            fast_copy(&[&src, &dst]).with_context(|| {
                format!("overlaying {} onto {}", src.display(), dst.display())
            })?;
        } else if dst.symlink_metadata().is_ok() {
            // Deleted from the worktree — drop it from the clone. git reports
            // deletions per file, so `dst` is a file or symlink, never a dir.
            std::fs::remove_file(&dst).with_context(|| format!("removing {}", dst.display()))?;
        }
    }

    Ok(())
}

/// Side-stash of a project directory's contents, used to make sampled steps
/// i.i.d. by restoring before each iteration.
pub struct Snapshot {
    holder: tempfile::TempDir,
}

impl Snapshot {
    /// Copy `source` to `<holder>/snap`. Captures the full tree (including
    /// `target/`, `.git/`, etc.) so a restore returns to byte-identical state.
    pub fn capture(source: &Path) -> Result<Self> {
        let holder = tempfile::Builder::new()
            .prefix("wezel-snapshot-")
            .tempdir()
            .context("creating snapshot tempdir")?;
        let dest = holder.path().join("snap");
        fast_copy(&[source, &dest])
            .with_context(|| format!("snapshotting {}", source.display()))?;
        Ok(Self { holder })
    }

    /// Wipe `target` and copy the snapshot's contents back in. `target` itself
    /// is preserved (so external owners — like a `Scratch`'s `TempDir` — keep
    /// their invariant).
    pub fn restore_to(&self, target: &Path) -> Result<()> {
        for entry in std::fs::read_dir(target)
            .with_context(|| format!("reading {} for restore", target.display()))?
        {
            let entry = entry?;
            let p = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() && !ft.is_symlink() {
                std::fs::remove_dir_all(&p).with_context(|| format!("removing {}", p.display()))?;
            } else {
                std::fs::remove_file(&p).with_context(|| format!("removing {}", p.display()))?;
            }
        }
        // fcp has no `snap/.` "copy contents" idiom; pass the snapshot's
        // children as individual sources into `target` instead.
        let snap = self.holder.path().join("snap");
        let mut args = std::fs::read_dir(&snap)
            .with_context(|| format!("reading {} for restore", snap.display()))?
            .map(|entry| Ok(entry?.path()))
            .collect::<Result<Vec<_>>>()?;
        if args.is_empty() {
            // Empty snapshot: the wipe above already produced the right state.
            return Ok(());
        }
        args.push(target.to_path_buf());
        fast_copy(&args).with_context(|| format!("restoring into {}", target.display()))
    }
}
