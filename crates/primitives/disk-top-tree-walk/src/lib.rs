//! Bounded disk-top walk primitive.
//!
//! @system disk-top-tree-walk
//! @status handwritten
//! @edit the SINGLE public surface — `walk_top_consumers` and its entry
//!   kind. Replaces the wedged depth-3 tree walk in `scala-tools disk --top`
//!   with bounded per-root totals and btrfs subvolume totals for excluded
//!   hot trees. Split per `single-purpose-file`: the walk loop lives in
//!   `walk.rs`, the btrfs subvolume helper in `btrfs.rs`, this file owns
//!   the public type and the top-level entry shape.
//!
//! ## Why the wedge closes here
//!
//! Measured 2026-09-19: the old `scala-tools disk --top` walked the host
//! at depth 3, descending into `/var/lib/postgresql` (live WAL) and
//! `/var/lib/incus` (the btrfs pool file backing the container rootfs),
//! and wedged at futex_do_wait for over a minute. The defect was
//! structural — the wrong command was given. This crate runs ONE
//! non-recursive read per root (the single-byte total, no descent), plus
//! a btrfs subvolume extent-tree total for any root on a btrfs
//! filesystem the caller marks as hot. Neither call descends. Neither
//! can wedge the way the old walk did.

mod btrfs;
mod walk;

pub use walk::{walk_top_consumers, TopConsumer};
