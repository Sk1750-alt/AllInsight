//! Directories a storage walk must not enter.
//!
//! On Windows every volume has its own drive letter and a walk of `C:\` stays
//! on `C:`. A Unix filesystem is one tree, and a walk of `/` would otherwise
//! wander into:
//!
//! - kernel interfaces such as `/proc` and `/sys`, where `/proc/kcore` alone
//!   reports the size of the address space and would top Large Files
//! - memory-backed filesystems (`tmpfs`), which are not disk space
//! - Snap's squashfs images, which would count every snap twice
//! - network and WSL shares, which are someone else's disk and slow to walk
//!
//! The fence lists the mount points of those filesystems once, at the start of
//! a walk, and the walkers step around them. A fenced mount point that *is* the
//! walk's root, or contains it, is left alone: when a person deliberately
//! scans `/mnt/c`, they get `/mnt/c`.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Fence {
    blocked: Vec<PathBuf>,
}

/// Filesystem types that never hold the user's disk usage, or that belong to
/// another machine.
#[cfg(target_os = "linux")]
const FENCED_TYPES: &[&str] = &[
    // Kernel and memory-backed.
    "proc",
    "sysfs",
    "devtmpfs",
    "devpts",
    "tmpfs",
    "ramfs",
    "cgroup",
    "cgroup2",
    "securityfs",
    "debugfs",
    "tracefs",
    "pstore",
    "bpf",
    "fusectl",
    "configfs",
    "mqueue",
    "hugetlbfs",
    "autofs",
    "binfmt_misc",
    "efivarfs",
    "nsfs",
    "rpc_pipefs",
    "selinuxfs",
    // Read-only images and container layers that duplicate other storage.
    "squashfs",
    "overlay",
    "fuse.snapfuse",
    "iso9660",
    // Desktop portals and virtual file systems.
    "fuse.portal",
    "fuse.gvfsd-fuse",
    "fuse.xdg-document-portal",
    // Other machines, including the Windows drives WSL exposes.
    "9p",
    "drvfs",
    "nfs",
    "nfs4",
    "cifs",
    "smb3",
    "smbfs",
    "fuse.sshfs",
    "fuse.rclone",
    "afs",
    "ceph",
    "glusterfs",
];

/// Decode the octal escapes `/proc/self/mounts` uses for spaces and tabs.
#[cfg(any(target_os = "linux", test))]
fn unescape_mount_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let digits = &bytes[i + 1..i + 4];
            if digits.iter().all(|b| (b'0'..=b'7').contains(b)) {
                let value = digits
                    .iter()
                    .fold(0u32, |acc, d| acc * 8 + u32::from(d - b'0'));
                if let Ok(v) = u8::try_from(value) {
                    out.push(v);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The mount points in a `/proc/self/mounts` listing whose type is fenced.
#[cfg(any(target_os = "linux", test))]
fn fenced_mounts(table: &str, fenced: &[&str]) -> Vec<PathBuf> {
    table
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _device = fields.next()?;
            let mount = fields.next()?;
            let fstype = fields.next()?;
            fenced
                .contains(&fstype)
                .then(|| PathBuf::from(unescape_mount_path(mount)))
        })
        .collect()
}

impl Fence {
    /// Build the fence for a walk that starts at `root`.
    pub fn for_root(root: &Path) -> Self {
        let mut blocked: Vec<PathBuf> = Vec::new();

        #[cfg(target_os = "linux")]
        {
            for always in ["/proc", "/sys", "/dev", "/run"] {
                blocked.push(PathBuf::from(always));
            }
            if let Ok(table) = std::fs::read_to_string("/proc/self/mounts") {
                blocked.extend(fenced_mounts(&table, FENCED_TYPES));
            }
        }

        #[cfg(target_os = "macos")]
        {
            // `/System/Volumes/Data` is the same data again through a
            // firmlink, and `/Volumes` holds other disks.
            for always in ["/dev", "/System/Volumes", "/Volumes", "/private/var/vm"] {
                blocked.push(PathBuf::from(always));
            }
        }

        // A fence around the walk's own root, or above it, would stop the walk
        // the user asked for before it started.
        blocked.retain(|b| !root.starts_with(b));
        blocked.sort();
        blocked.dedup();
        Self { blocked }
    }

    /// True when the walk must not descend into `dir`.
    pub fn blocks(&self, dir: &Path) -> bool {
        !self.blocked.is_empty() && self.blocked.iter().any(|b| b == dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_tables_yield_only_fenced_types() {
        let table = "\
/dev/sda2 / ext4 rw,relatime 0 0
proc /proc proc rw,nosuid 0 0
tmpfs /run/user/1000 tmpfs rw 0 0
/dev/loop3 /snap/firefox/4793 squashfs ro 0 0
drivers /usr/lib/wsl/drivers 9p ro 0 0
C:\\134 /mnt/c 9p rw 0 0
/dev/sdb1 /media/me/My\\040Disk ext4 rw 0 0
";
        let fenced = fenced_mounts(table, &["proc", "tmpfs", "squashfs", "9p"]);
        assert!(fenced.contains(&PathBuf::from("/proc")));
        assert!(fenced.contains(&PathBuf::from("/snap/firefox/4793")));
        assert!(fenced.contains(&PathBuf::from("/mnt/c")));
        assert!(!fenced.contains(&PathBuf::from("/")));
        assert!(!fenced.iter().any(|p| p.starts_with("/media")));
    }

    #[test]
    fn octal_escapes_are_decoded() {
        assert_eq!(
            unescape_mount_path("/media/me/My\\040Disk"),
            "/media/me/My Disk"
        );
        assert_eq!(unescape_mount_path("/plain"), "/plain");
        assert_eq!(unescape_mount_path("/trailing\\"), "/trailing\\");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_interfaces_are_fenced_from_the_root_but_not_from_themselves() {
        let from_root = Fence::for_root(Path::new("/"));
        assert!(from_root.blocks(Path::new("/proc")));
        assert!(from_root.blocks(Path::new("/sys")));
        assert!(!from_root.blocks(Path::new("/home")));

        let from_proc = Fence::for_root(Path::new("/proc"));
        assert!(!from_proc.blocks(Path::new("/proc")));
    }
}
