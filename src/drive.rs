use std::{ffi::OsStr, iter, os::windows::ffi::OsStrExt};

use windows::{
    Win32::{
        Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives},
        System::WindowsProgramming::{DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE},
    },
    core::PCWSTR,
};

/// 表示一个驱动器（盘符）。
#[derive(Debug, Clone)]
pub struct Drive {
    inner: String,
}

impl Drive {
    /// # Panics
    ///
    /// 如果盘符不在 A 到 Z 之间，函数会崩溃。
    fn from_letter(letter: char) -> Self {
        assert!(
            letter.is_ascii_uppercase(),
            "盘符 {} 不在 A 到 Z 之间",
            letter
        );

        Self {
            inner: format!(r"\\.\{}:", letter),
        }
    }

    /// 作为卷路径返回，类似 `\\.\C:`。
    pub fn as_volume_path(&self) -> &str {
        &self.inner
    }

    /// 作为驱动器路径返回，类似 `C:`
    pub fn as_drive_path(&self) -> &str {
        &self.inner[4..6]
    }
}

/// 扫描支持的驱动器。
pub fn discover_drives() -> Vec<Drive> {
    let mut res = Vec::new();
    let mut mask = unsafe { GetLogicalDrives() };
    for letter in 'A'..='Z' {
        if mask & 1 == 1 {
            let drive = Drive::from_letter(letter);
            if matches!(
                driver_type(&drive),
                // 某些网络挂载类型会导致打开句柄时线程因 IO 被挂起
                DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_RAMDISK
            ) {
                res.push(drive);
            }
        }

        mask >>= 1;
        if mask == 0 {
            break;
        }
    }
    res
}

fn driver_type(drive: &Drive) -> u32 {
    // 传给 FFI 的路径是 0 结尾的
    let path: Vec<u16> = OsStr::new(drive.as_drive_path())
        .encode_wide()
        .chain(iter::once(0))
        .collect();
    // https://learn.microsoft.com/zh-cn/windows/win32/api/fileapi/nf-fileapi-getdrivetypew
    unsafe { GetDriveTypeW(PCWSTR::from_raw(path.as_ptr())) }
}

#[cfg(test)]
mod tests {
    use super::Drive;

    #[test]
    fn drive_paths_keep_the_drive_colon() {
        let drive = Drive::from_letter('C');

        assert_eq!(drive.as_volume_path(), r"\\.\C:");
        assert_eq!(drive.as_drive_path(), "C:");
    }
}
