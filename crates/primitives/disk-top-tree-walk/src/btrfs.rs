//! btrfs subvolume total — the readback the disk-is-a-budget canonical
//! instrument recommends for any path sitting on a btrfs filesystem.
//!
//! @system disk-top-tree-walk
//! @status handwritten
//! @edit the ONE helper for excluded_hot trees: `btrfs filesystem du -s`
//!   reports the subvolume exclusive+shared extent total WITHOUT
//!   descending. It is the readback shape the disk top walk needs for
//!   hot trees (per `disk-is-a-budget` — btrfs extents pinned by
//!   snapshots are visible to the filesystem-level total and invisible
//!   to a directory tree walk). `walk.rs` dispatches to this helper for
//!   any path the caller marked hot; the helper returns the byte total
//!   or refuses naming the failure shape.

use std::path::Path;

use anyhow::Context;

/// Run `btrfs filesystem du -s <path>` and return the byte total. The
/// command reports a `<total>\t<path>` line on stdout (one path per
/// invocation). Output is parsed with `scala_os::disk::parse_du_bytes`
/// — the same parser the bounded walk uses — so a parse failure has one
/// shape everywhere.
pub(crate) async fn btrfs_fs_du_s(path: &Path) -> anyhow::Result<u64> {
    let output = host_spawn::command_async("btrfs")
        .args(["filesystem", "du", "-s", &path.display().to_string()])
        .kill_on_drop(true)
        .output()
        .await
        .with_context(|| format!("spawn btrfs filesystem extent-tree read for {}", path.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "btrfs filesystem du exited non-zero (status {:?}) for {}: {}",
            output.status.code(),
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    scala_os::disk::parse_du_bytes(&String::from_utf8_lossy(&output.stdout))
        .with_context(|| format!("parse btrfs filesystem du output for {}", path.display()))
}
