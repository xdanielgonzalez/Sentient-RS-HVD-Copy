# .ci/scripts/uat/debugfs-lib.sh — shared debugfs helpers for OFFLINE rootfs edits.
#
# Source this (do not execute it) from the UAT injection scripts. All writes into the golden
# base's ext4 rootfs go through debugfs (e2fsprogs): no mount, no loop device, no root-in-guest —
# the same tooling the old rootfs-injection approach used, just wrapped so the rm/write/sif
# four-step isn't hand-repeated (and mis-typed) at every call site.

# dfs_write <rootfs> <local-src> <dest-path> [mode]
#   Write a local file into the rootfs at dest-path, replacing any existing file, then set the
#   inode fields (sif sets mode INCLUDING the type bits, then ownership root:root). The preceding
#   `rm` is tolerated when the target doesn't exist yet.
#   NOTE: debugfs `write` places the file at the full nested destination path directly — the
#   parent directory must already exist (see dfs_mkdir).
dfs_write() {
  local rootfs="$1" src="$2" dst="$3" mode="${4:-0100644}"
  debugfs -w -R "rm $dst" "$rootfs" 2>/dev/null || true
  debugfs -w -R "write $src $dst" "$rootfs"
  debugfs -w -R "sif $dst mode $mode" "$rootfs"
  debugfs -w -R "sif $dst uid 0" "$rootfs"
  debugfs -w -R "sif $dst gid 0" "$rootfs"
}

# dfs_mkdir <rootfs> <dir>
#   mkdir inside the rootfs — a no-op error if the directory already exists; tolerate that.
#   The PARENT must exist (debugfs mkdir is not -p).
dfs_mkdir() {
  debugfs -w -R "mkdir $2" "$1" 2>/dev/null || true
}

# dfs_cat <rootfs> <path>
#   Read a file back out (verification/diagnostics). Non-fatal on absence — callers decide.
dfs_cat() {
  debugfs -R "cat $2" "$1" 2>/dev/null
}
