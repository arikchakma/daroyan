use std::collections::HashMap;

use daroyan_protocol::report::DiskState;
use nix::sys::statvfs::statvfs;
use sysinfo::{Disk, Disks};

use crate::collect::per_sec;

const SKIPPED_FILESYSTEMS: [&str; 12] = [
    "overlay", "tmpfs", "devtmpfs", "squashfs", "nsfs", "autofs", "virtiofs", "9p",
    "nfs", "nfs4", "cifs", "smb3",
];

pub fn collect_disks(
    disks: &Disks,
    allow: &[String],
    elapsed_secs: f64,
) -> Vec<DiskState> {
    pick_disks(disks.list(), allow)
        .into_iter()
        .map(|disk| {
            let usage = disk.usage();
            let (inode_total, inode_used) =
                statvfs(disk.mount_point()).map_or((0, 0), |stat| {
                    let total = u64::from(stat.files());
                    (total, total.saturating_sub(u64::from(stat.files_free())))
                });
            DiskState {
                mount: disk.mount_point().display().to_string(),
                filesystem: disk.file_system().to_string_lossy().into_owned(),
                disk_total: disk.total_space(),
                disk_used: disk.total_space().saturating_sub(disk.available_space()),
                inode_total,
                inode_used,
                read_speed: per_sec(usage.read_bytes, elapsed_secs),
                write_speed: per_sec(usage.written_bytes, elapsed_secs),
            }
        })
        .collect()
}

fn pick_disks<'a>(disks: &'a [Disk], allow: &[String]) -> Vec<&'a Disk> {
    let mut by_device: HashMap<String, &Disk> = HashMap::new();
    for disk in disks {
        let mount = disk.mount_point().to_string_lossy();
        if !is_reported(&mount, &disk.file_system().to_string_lossy(), allow) {
            continue;
        }
        let device = disk.name().to_string_lossy().into_owned();
        let is_shorter = by_device
            .get(&device)
            .is_none_or(|kept| mount.len() < kept.mount_point().to_string_lossy().len());
        if is_shorter {
            by_device.insert(device, disk);
        }
    }
    let mut picked: Vec<&Disk> = by_device.into_values().collect();
    picked.sort_by(|a, b| a.mount_point().cmp(b.mount_point()));
    picked
}

fn is_reported(mount: &str, filesystem: &str, allow: &[String]) -> bool {
    if allow.is_empty() {
        !SKIPPED_FILESYSTEMS.contains(&filesystem) && !filesystem.starts_with("fuse")
    } else {
        allow.iter().any(|allowed| allowed == mount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_virtual_and_network_filesystems_unless_allowed() {
        assert!(is_reported("/", "ext4", &[]));
        assert!(is_reported("/var/lib/docker", "xfs", &[]));
        assert!(!is_reported("/var/lib/docker/overlay2/x/merged", "overlay", &[]));
        assert!(!is_reported("/run", "tmpfs", &[]));
        assert!(!is_reported("/Users", "virtiofs", &[]));
        assert!(!is_reported("/mnt/backup", "nfs4", &[]));
        assert!(!is_reported("/var/lib/lxcfs", "fuse.lxcfs", &[]));
        assert!(!is_reported("/", "ext4", &["/data".to_string()]));
        assert!(is_reported("/mnt/backup", "nfs4", &["/mnt/backup".to_string()]));
    }
}
