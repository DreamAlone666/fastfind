use anyhow::{ensure, Result};
use std::{ffi::OsStr, iter::once, mem::MaybeUninit, os::windows::ffi::OsStrExt};
use windows::{
    core::{Owned, PCWSTR},
    Win32::{
        Foundation::{GENERIC_READ, HANDLE},
        Storage::FileSystem::{
            CreateFileW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
            FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::WindowsProgramming::{DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE},
    },
};

use super::IterUsnRecord;

// https://github.com/microsoft/windows-rs/pull/3013
// 通过Drop自动释放HANDLE
pub struct Volume {
    driver: String,
    handle: Owned<HANDLE>,
}

impl Volume {
    pub fn from_driver(driver: String) -> Result<Self> {
        let fs = driver_fs(&driver)?;
        ensure!(fs == "NTFS", "不支持的文件系统：{}", fs);

        // https://learn.microsoft.com/zh-cn/windows/win32/fileio/naming-a-file
        let path: Vec<_> = OsStr::new(r"\\.\")
            .encode_wide()
            .chain(OsStr::new(&driver).encode_wide())
            .chain(once(0))
            .collect();
        let handle = unsafe {
            // https://learn.microsoft.com/zh-cn/windows/win32/api/fileapi/nf-fileapi-createfilew
            Owned::new(CreateFileW(
                PCWSTR::from_raw(path.as_ptr()),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )?)
        };

        Ok(Self { driver, handle })
    }

    pub fn iter_usn_record(&self, buf_size: usize) -> IterUsnRecord {
        IterUsnRecord::new(&self.handle, buf_size)
    }

    pub fn driver(&self) -> &str {
        &self.driver
    }
}

pub fn scan_drivers() -> Vec<String> {
    let mut res = Vec::new();
    let mut mask = unsafe { GetLogicalDrives() };
    for letter in 'A'..='Z' {
        if mask & 1 == 1 {
            let driver = format!("{}:", letter);
            if matches!(
                driver_type(&driver),
                DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_RAMDISK
            ) {
                res.push(driver);
            }
        }

        mask >>= 1;
        if mask == 0 {
            break;
        }
    }
    res
}

fn driver_fs(driver: &str) -> Result<String> {
    let mut buf: MaybeUninit<[u16; 12]> = MaybeUninit::uninit();
    let path = driver_to_path(driver);
    unsafe {
        // https://learn.microsoft.com/zh-cn/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationw
        GetVolumeInformationW(
            PCWSTR::from_raw(path.as_ptr()),
            None,
            None,
            None,
            None,
            Some(buf.assume_init_mut()),
        )?;
    }

    let buf = unsafe { buf.assume_init_ref() };
    let termination = buf.iter().position(|&ch| ch == 0).unwrap();
    Ok(String::from_utf16_lossy(&buf[..termination]))
}

fn driver_type(driver: &str) -> u32 {
    let path = driver_to_path(driver);
    // https://learn.microsoft.com/zh-cn/windows/win32/api/fileapi/nf-fileapi-getdrivetypew
    unsafe { GetDriveTypeW(PCWSTR::from_raw(path.as_ptr())) }
}

fn driver_to_path(driver: &str) -> Vec<u16> {
    OsStr::new(driver)
        .encode_wide()
        .chain(OsStr::new("\\\0").encode_wide())
        .collect()
}
