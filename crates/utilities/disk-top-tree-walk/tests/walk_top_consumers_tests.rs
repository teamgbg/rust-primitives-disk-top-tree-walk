//! Fixture pair for `walk_top_consumers` — the clean fixture exercises
//! the bounded single-root-total readback against real tempdirs, the
//! hot-tree fixture confirms the marker routes to the btrfs helper (and
//! is skipped gracefully when btrfs is unavailable on this host).
//!
//! @system disk-top-tree-walk
//! @status handwritten
//! @edit the two-half fixture pair required by `what-must-have-a-test`:
//!   the bounded-walk half always runs (every host has a unix tree-size
//!   tool on PATH); the hot-tree half is gated on `btrfs` being on PATH
//!   AND `/` being a btrfs filesystem, because the helper is a no-op
//!   otherwise and asserting success would be `proxy-is-not-behaviour`.

use std::path::PathBuf;

use disk_top_tree_walk::walk_top_consumers;

/// Build three roots with known size ordering — big, medium, tiny —
/// each in its own tempdir so the bounded single-root read reports the
/// real per-dir total (no cross-pollination).
async fn three_sized_roots() -> (tempdir::TempDir, Vec<PathBuf>) {
    let dir = tempdir::TempDir::new("disk-top-tree-walk-fixture")
        .expect("tempdir creation");
    let big = dir.path().join("big");
    let medium = dir.path().join("medium");
    let tiny = dir.path().join("tiny");
    for (path, size) in [
        (&big, 4096u64 * 1024),
        (&medium, 1024u64 * 1024),
        (&tiny, 256u64 * 1024),
    ] {
        std::fs::create_dir_all(path).expect("dir create");
        std::fs::write(path.join("payload.bin"), vec![0u8; size as usize])
            .expect("payload write");
    }
    (dir, vec![big, medium, tiny])
}

/// Clean fixture — bounded walk ranks by descending size, every entry
/// reports `is_hot = false`, no entry descended.
#[tokio::test]
async fn bounded_walk_ranks_by_descending_bytes_and_marks_no_tree_hot() {
    let (_dir, roots) = three_sized_roots().await;
    let report = walk_top_consumers(&roots, &[]).await.expect("bounded walk");
    assert_eq!(
        report.len(),
        3,
        "all three roots returned, no descent into any of them"
    );
    for entry in &report {
        assert!(
            !entry.is_hot,
            "no entry marked hot when excluded_hot is empty: {:?}",
            entry
        );
        assert!(
            entry.bytes > 0,
            "each root returned a positive byte total: {:?}",
            entry
        );
    }
    let sizes: Vec<u64> = report.iter().map(|e| e.bytes).collect();
    let mut sorted = sizes.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(
        sizes, sorted,
        "bounded walk returned entries in descending-byte order"
    );
}

/// The hot-tree fixture is GATED on the real environment: `btrfs` must
/// be on PATH AND `/` must report as a btrfs filesystem. Both checks
/// are explicit; the test skips with a recorded reason rather than
/// failing when the helper is structurally unavailable — a missing
/// binary is not the same defect as a wrong marker.
#[tokio::test]
async fn hot_tree_marker_routes_to_btrfs_helper_when_filesystem_supports_it() {
    if !binary_on_path("btrfs") {
        eprintln!("skip: btrfs binary not on PATH");
        return;
    }
    if !root_is_btrfs().await {
        eprintln!("skip: / is not a btrfs filesystem on this host");
        return;
    }
    let (_dir, roots) = three_sized_roots().await;
    let hot = roots.clone();
    let report = walk_top_consumers(&roots, &hot)
        .await
        .expect("bounded walk with all-hot roots");
    assert_eq!(report.len(), 3, "all three roots still returned");
    for entry in &report {
        assert!(
            entry.is_hot,
            "hot marker routed the root to the btrfs helper: {:?}",
            entry
        );
    }
}

fn binary_on_path(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

async fn root_is_btrfs() -> bool {
    let output = host_spawn::command_async("stat")
        .args(["-f", "-c", "%T", "/"])
        .kill_on_drop(true)
        .output()
        .await;
    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).contains("btrfs")
        }
        _ => false,
    }
}

/// A root that does not exist is omitted from the report — a missing
/// path is a structural fact, not an error, and the bounded walk
/// surfaces it by absence rather than panicking.
#[tokio::test]
async fn missing_root_is_omitted_not_panicked() {
    let (_dir, real_roots) = three_sized_roots().await;
    let mut roots = real_roots;
    roots.push(PathBuf::from("/this/path/does/not/exist/anywhere"));
    let report = walk_top_consumers(&roots, &[])
        .await
        .expect("bounded walk tolerates a missing root");
    assert_eq!(
        report.len(),
        3,
        "the missing root was omitted; the three real roots still ranked"
    );
}
