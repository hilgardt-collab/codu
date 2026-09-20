//! Volume discovery: mounted filesystems and unmounted block devices.
//!
//! On Linux this reads `/proc/self/mounts`, `/proc/swaps`, `/sys/class/block`
//! and the udev database under `/run/udev/data`, and queries usage with
//! `statvfs`. Other platforms currently report no volumes.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    Disk,
    Removable,
    Network,
    Optical,
    Swap,
}

impl VolumeKind {
    pub fn label(self) -> &'static str {
        match self {
            VolumeKind::Disk => "disk",
            VolumeKind::Removable => "removable",
            VolumeKind::Network => "network",
            VolumeKind::Optical => "optical",
            VolumeKind::Swap => "swap",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// `/dev/sda1`, `/dev/mapper/vg-root`, `nas:/export`, …
    pub device: String,
    /// Where it is mounted; `None` for unmounted devices and swap.
    pub mount_point: Option<PathBuf>,
    /// Filesystem type as reported by the kernel or udev; empty if unknown.
    pub fs_type: String,
    /// Filesystem label or partition name, if any.
    pub label: Option<String>,
    /// Drive model from sysfs, if any.
    pub model: Option<String>,
    /// Capacity in bytes (filesystem size when mounted, device size otherwise).
    pub total: u64,
    /// Bytes in use (0 when unknown).
    pub used: u64,
    pub kind: VolumeKind,
    pub read_only: bool,
}

impl Volume {
    pub fn mounted(&self) -> bool {
        self.mount_point.is_some()
    }

    /// What to call it in a list: the mount point, or the device otherwise.
    pub fn name(&self) -> String {
        match &self.mount_point {
            Some(p) => p.display().to_string(),
            None => self.device.clone(),
        }
    }

    /// Short device name (`sda1` for `/dev/sda1`).
    pub fn short_device(&self) -> &str {
        self.device.strip_prefix("/dev/").unwrap_or(&self.device)
    }

    pub fn share(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.used as f64 / self.total as f64
        }
    }
}

/// All volumes: mounted ones first (by mount point), then unmounted devices.
pub fn list() -> Vec<Volume> {
    #[cfg(target_os = "linux")]
    {
        linux::list()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

/// Whether volume listing is implemented for this platform.
pub fn supported() -> bool {
    cfg!(target_os = "linux")
}

/// Find the volume mounted exactly at `path`.
pub fn at<'a>(volumes: &'a [Volume], path: &Path) -> Option<&'a Volume> {
    volumes
        .iter()
        .find(|v| v.mount_point.as_deref() == Some(path))
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Volume, VolumeKind};
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    #[derive(Debug, PartialEq, Eq)]
    pub struct MountEntry {
        pub device: String,
        pub mount_point: PathBuf,
        pub fs_type: String,
        pub options: String,
    }

    const NETWORK_FS: &[&str] = &[
        "nfs",
        "nfs4",
        "cifs",
        "smb3",
        "smbfs",
        "afs",
        "ceph",
        "glusterfs",
        "9p",
        "virtiofs",
        "fuse.sshfs",
        "fuse.rclone",
        "fuse.gcsfuse",
        "fuse.s3fs",
        "davfs",
        "fuse.davfs2",
    ];

    /// Decode the octal escapes used in /proc/mounts (`\040` for a space).
    pub fn decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\\'
                && i + 4 <= bytes.len()
                && let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 4])
                    .ok()
                    .and_then(|o| u8::from_str_radix(o, 8).ok())
            {
                out.push(v);
                i += 4;
                continue;
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    pub fn parse_mounts(text: &str) -> Vec<MountEntry> {
        text.lines()
            .filter_map(|line| {
                let mut f = line.split_whitespace();
                let device = decode(f.next()?);
                let mount_point = PathBuf::from(decode(f.next()?));
                let fs_type = f.next()?.to_string();
                let options = f.next().unwrap_or("").to_string();
                Some(MountEntry {
                    device,
                    mount_point,
                    fs_type,
                    options,
                })
            })
            .collect()
    }

    /// Real storage, as opposed to pseudo filesystems and snap/loop images.
    pub fn is_real(e: &MountEntry) -> bool {
        if e.fs_type == "squashfs" || e.fs_type == "autofs" {
            return false;
        }
        if e.device.starts_with("/dev/") {
            return !e.device.starts_with("/dev/loop");
        }
        NETWORK_FS.contains(&e.fs_type.as_str())
            || (e.fs_type.starts_with("fuse.")
                && !matches!(e.fs_type.as_str(), "fuse.portal" | "fuse.gvfsd-fuse"))
    }

    pub fn statvfs(path: &Path) -> Option<(u64, u64)> {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c` is a valid NUL-terminated path and `st` is a writable
        // out-parameter of the right type.
        let rc = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
        if rc != 0 {
            return None;
        }
        let frsize = st.f_frsize as u64;
        let total = st.f_blocks as u64 * frsize;
        let used = st.f_blocks.saturating_sub(st.f_bfree) as u64 * frsize;
        Some((total, used))
    }

    /// `/dev/mapper/x` → `/dev/dm-0`, so mounts and sysfs names can be matched.
    fn canonical(dev: &str) -> String {
        fs::canonicalize(dev)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| dev.to_string())
    }

    /// `E:KEY=value` lines from the udev database for a block device.
    fn udev_props(name: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let Ok(devno) = fs::read_to_string(format!("/sys/class/block/{name}/dev")) else {
            return map;
        };
        let Ok(text) = fs::read_to_string(format!("/run/udev/data/b{}", devno.trim())) else {
            return map;
        };
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("E:")
                && let Some((k, v)) = rest.split_once('=')
            {
                map.insert(k.to_string(), decode_udev(v));
            }
        }
        map
    }

    /// udev escapes non-printables as `\x20`.
    fn decode_udev(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' && chars.peek() == Some(&'x') {
                chars.next();
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(v) = u8::from_str_radix(&hex, 16) {
                    out.push(v as char);
                    continue;
                }
                out.push_str("\\x");
                out.push_str(&hex);
            } else {
                out.push(c);
            }
        }
        out
    }

    fn sys_read(name: &str, file: &str) -> Option<String> {
        fs::read_to_string(format!("/sys/class/block/{name}/{file}"))
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// The whole-disk name a partition belongs to (`sda1` → `sda`).
    fn base_disk(name: &str) -> String {
        if Path::new(&format!("/sys/class/block/{name}/partition")).exists()
            && let Ok(real) = fs::canonicalize(format!("/sys/class/block/{name}"))
            && let Some(parent) = real.parent().and_then(|p| p.file_name())
        {
            return parent.to_string_lossy().into_owned();
        }
        name.to_string()
    }

    fn kind_for(name: &str, fs_type: &str, device: &str) -> VolumeKind {
        if fs_type == "swap" {
            return VolumeKind::Swap;
        }
        if !device.starts_with("/dev/") || NETWORK_FS.contains(&fs_type) {
            return VolumeKind::Network;
        }
        let base = base_disk(name);
        if base.starts_with("sr") {
            return VolumeKind::Optical;
        }
        if sys_read(&base, "removable").as_deref() == Some("1") {
            return VolumeKind::Removable;
        }
        VolumeKind::Disk
    }

    fn model_for(name: &str) -> Option<String> {
        let base = base_disk(name);
        sys_read(&base, "device/model").filter(|m| !m.is_empty())
    }

    /// Active swap devices with their size and usage (from /proc/swaps).
    fn swaps() -> HashMap<String, (u64, u64)> {
        let mut m = HashMap::new();
        if let Ok(text) = fs::read_to_string("/proc/swaps") {
            for line in text.lines().skip(1) {
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() >= 4
                    && let (Ok(size), Ok(used)) = (f[2].parse::<u64>(), f[3].parse::<u64>())
                {
                    m.insert(canonical(f[0]), (size * 1024, used * 1024));
                }
            }
        }
        m
    }

    pub fn list() -> Vec<Volume> {
        let mounts = fs::read_to_string("/proc/self/mounts").unwrap_or_default();
        let entries: Vec<MountEntry> = parse_mounts(&mounts).into_iter().filter(is_real).collect();
        let swaps = swaps();

        let mut mounted_devs: HashSet<String> = HashSet::new();
        let mut volumes: Vec<Volume> = Vec::new();
        let mut seen: HashSet<(String, PathBuf)> = HashSet::new();
        for e in &entries {
            let canon = canonical(&e.device);
            mounted_devs.insert(canon.clone());
            if !seen.insert((canon.clone(), e.mount_point.clone())) {
                continue;
            }
            let name = canon.strip_prefix("/dev/").unwrap_or(&canon).to_string();
            let props = udev_props(&name);
            let (total, used) = statvfs(&e.mount_point).unwrap_or((0, 0));
            volumes.push(Volume {
                kind: kind_for(&name, &e.fs_type, &e.device),
                device: e.device.clone(),
                mount_point: Some(e.mount_point.clone()),
                fs_type: e.fs_type.clone(),
                label: props
                    .get("ID_FS_LABEL")
                    .or_else(|| props.get("ID_PART_ENTRY_NAME"))
                    .cloned(),
                model: model_for(&name),
                total,
                used,
                read_only: e.options.split(',').any(|o| o == "ro"),
            });
        }
        volumes.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));

        // Unmounted block devices: partitions, and whole disks with no partitions.
        let mut names: Vec<String> = fs::read_dir("/sys/class/block")
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        // Whole disks that carry partitions; their partitions are listed instead.
        let parents: HashSet<String> = names
            .iter()
            .filter(|n| Path::new(&format!("/sys/class/block/{n}/partition")).exists())
            .map(|n| base_disk(n))
            .collect();
        let mut unmounted: Vec<Volume> = Vec::new();
        for name in &names {
            if name.starts_with("loop") || name.starts_with("ram") {
                continue;
            }
            let is_partition = Path::new(&format!("/sys/class/block/{name}/partition")).exists();
            if !is_partition && parents.contains(name) {
                continue; // whole disk with partitions: its partitions are listed instead
            }
            let dev = format!("/dev/{name}");
            if mounted_devs.contains(&dev) {
                continue;
            }
            let size = sys_read(name, "size")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0)
                * 512;
            if size == 0 {
                continue;
            }
            let props = udev_props(name);
            let fs_type = props.get("ID_FS_TYPE").cloned().unwrap_or_default();
            let usage = props.get("ID_FS_USAGE").map(|s| s.as_str()).unwrap_or("");
            let swap = swaps.get(&dev).copied();
            if swap.is_none() && (fs_type.is_empty() || matches!(usage, "raid")) {
                continue; // no filesystem signature, or a RAID/LVM member
            }
            if fs_type == "LVM2_member" {
                continue;
            }
            let (total, used, fs_type) = match swap {
                Some((t, u)) => (t, u, "swap".to_string()),
                None => (size, 0, fs_type),
            };
            unmounted.push(Volume {
                kind: kind_for(name, &fs_type, &dev),
                device: dev,
                mount_point: None,
                fs_type,
                label: props
                    .get("ID_FS_LABEL")
                    .or_else(|| props.get("ID_PART_ENTRY_NAME"))
                    .cloned(),
                model: model_for(name),
                total,
                used,
                read_only: sys_read(name, "ro").as_deref() == Some("1"),
            });
        }
        volumes.extend(unmounted);
        volumes
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn decodes_mount_escapes() {
            assert_eq!(
                decode("/run/media/me/Shared\\040store"),
                "/run/media/me/Shared store"
            );
            assert_eq!(decode("plain"), "plain");
            assert_eq!(decode("tab\\011x"), "tab\tx");
            assert_eq!(decode_udev("EFI\\x20system"), "EFI system");
        }

        #[test]
        fn parses_and_filters_mounts() {
            let text = "\
proc /proc proc rw,nosuid 0 0
/dev/nvme1n1p1 / btrfs rw,relatime,subvol=/@ 0 0
tmpfs /tmp tmpfs rw 0 0
/dev/sdb2 /run/media/me/Shared\\040store ntfs3 ro,relatime 0 0
/dev/loop3 /snap/core/1 squashfs ro 0 0
nas:/export /mnt/nas nfs4 rw 0 0
portal /run/user/1000/doc fuse.portal rw 0 0
";
            let entries = parse_mounts(text);
            assert_eq!(entries.len(), 7);
            let real: Vec<&MountEntry> = entries.iter().filter(|e| is_real(e)).collect();
            let names: Vec<String> = real
                .iter()
                .map(|e| e.mount_point.display().to_string())
                .collect();
            assert_eq!(names, ["/", "/run/media/me/Shared store", "/mnt/nas"]);
            assert!(real[1].options.split(',').any(|o| o == "ro"));
        }

        #[test]
        fn statvfs_root_reports_capacity() {
            let (total, used) = statvfs(Path::new("/")).expect("statvfs /");
            assert!(total > 0 && used <= total);
        }

        #[test]
        fn listing_does_not_panic_and_is_ordered() {
            let v = list();
            let first_unmounted = v.iter().position(|x| !x.mounted()).unwrap_or(v.len());
            assert!(v[..first_unmounted].iter().all(|x| x.mounted()));
            assert!(v[first_unmounted..].iter().all(|x| !x.mounted()));
        }
    }
}
