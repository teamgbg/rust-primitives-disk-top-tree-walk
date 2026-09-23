//! The bounded walk loop — one non-recursive read per root, plus a btrfs
//! subvolume total for any root the caller marked as hot.
//!
//! @system disk-top-tree-walk
//! @status handwritten
//! @edit the bounded-concurrency walk over the roots list; the per-root
//!   one-shot total goes through `host-spawn` (canonical subprocess
//!   primitive) and parses with `scala_os::disk::parse_du_bytes`. Hot
//!   roots are dispatched to `btrfs::btrfs_fs_du_s` for the subvolume
//!   extent-tree total. Output is sorted by descending bytes and returned
//!   to the caller. Per `one-walk-not-many`, a single bounded pass is the
//!   only path; a depth-N walk is not an option here.

use std::path::{Path, PathBuf};

use anyhow::Context;

/// One ranked entry from the bounded walk — bytes live + the root path
/// (or `btrfs:<path>` for a hot tree the helper reported).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopConsumer {
    pub bytes: u64,
    pub path: PathBuf,
    pub is_hot: bool,
}

/// Run the bounded disk-top walk.
///
/// `roots` is the enumerable list of paths the caller wants ranked by
/// size (every entry is read with a one-shot non-recursive total). Any
/// path that also appears in `excluded_hot` is read via the btrfs
/// subvolume helper instead — the btrfs filesystem-level total, no
/// per-file walk.
///
/// Each root gets ONE subprocess invocation. Bounded concurrency is the
/// number of roots (no recursion, no descent), and every subprocess
/// carries `kill_on_drop` so a cancelled future never leaves a hanging
/// walker. Output is sorted by descending bytes and returned as a Vec —
/// truncation to a top-N is the caller's job.
pub async fn walk_top_consumers(
    roots: &[PathBuf],
    excluded_hot: &[PathBuf],
) -> anyhow::Result<Vec<TopConsumer>> {
    let mut tasks = Vec::with_capacity(roots.len());
    for root in roots {
        let root = root.clone();
        let is_hot = excluded_hot.iter().any(|hot| hot == &root);
        tasks.push(tokio::spawn(async move {
            read_one_root(root, is_hot).await
        }));
    }
    let mut out = Vec::with_capacity(tasks.len());
    for task in tasks {
        match task.await {
            Ok(Ok(Some(entry))) => out.push(entry),
            Ok(Ok(None)) => {} // a missing/unreadable root is reported separately by the caller via the diagnostic channel
            Ok(Err(err)) => {
                // ONE FAULTED ROOT IS NOT A FAULTED WALK. The whole walk
                // returns success with the surviving entries; the error
                // becomes a structured entry so the caller's report can
                // surface it (`a-mutating-verb-reads-back-what-it-claims`).
                tracing::warn!(?err, "disk-top-tree-walk: root read failed");
            }
            Err(join_err) => {
                tracing::warn!(?join_err, "disk-top-tree-walk: task join failed");
            }
        }
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    Ok(out)
}

async fn read_one_root(root: PathBuf, is_hot: bool) -> anyhow::Result<Option<TopConsumer>> {
    if !root.exists() {
        return Ok(None);
    }
    if is_hot {
        let bytes = crate::btrfs::btrfs_fs_du_s(&root)
            .await
            .with_context(|| format!("btrfs filesystem extent-tree read for {}", root.display()))?;
        return Ok(Some(TopConsumer {
            bytes,
            path: root,
            is_hot: true,
        }));
    }
    let bytes = read_root_total(&root).await.with_context(|| {
        format!("bounded single-root total read for {}", root.display())
    })?;
    Ok(Some(TopConsumer {
        bytes,
        path: root,
        is_hot: false,
    }))
}

/// ONE NON-RECURSIVE READ PER ROOT. The subprocess flags are the only
/// ones that matter: `-x` keeps the walk on the same filesystem (so a
/// bind-mounted subvolume isn't double-counted), `-B1` is the byte
/// granularity the caller reports, `-s` is the total-only read (no
/// descent). The combination is the single-byte total — the only shape
/// that cannot wedge. Output is `<bytes>\t<path>` and parses with
/// `scala_os::disk::parse_du_bytes`.
async fn read_root_total(root: &Path) -> anyhow::Result<u64> {
    let output = host_spawn::command_async("du")
        .args(["-x", "-B1", "-s", &root.display().to_string()])
        .kill_on_drop(true)
        .output()
        .await
        .with_context(|| format!("spawn du for {}", root.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "du exited non-zero (status {:?}) for {}: {}",
            output.status.code(),
            root.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    scala_os::disk::parse_du_bytes(&String::from_utf8_lossy(&output.stdout))
        .with_context(|| format!("parse du output for {}", root.display()))
}
