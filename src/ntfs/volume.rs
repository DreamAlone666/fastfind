use anyhow::{ensure, Result};
use std::{ffi::OsStr, iter::once, os::windows::ffi::OsStrExt};
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
pub struct Volume(Owned<HANDLE>);

impl Volume {
    pub fn new(name: &str) -> Result<Self> {
        let fs = get_fs(name)?;
        ensure!(fs == "NTFS", "不支持的文件系统：{}", fs);

        // https://learn.microsoft.com/zh-cn/windows/win32/fileio/naming-a-file
        let path: Vec<_> = OsStr::new(r"\\.\")
            .encode_wide()
            .chain(OsStr::new(name).encode_wide())
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
        Ok(Self(handle))
    }

    pub fn names() -> Vec<String> {
        let mut res = Vec::new();
        unsafe {
            let mut mask = GetLogicalDrives();
            for driver in 'A'..='Z' {
                if mask & 1 == 1 {
                    let name = driver.to_string() + ":";
                    let path: Vec<_> = OsStr::new(&name).encode_wide().chain(once(0)).collect();

                    // https://learn.microsoft.com/zh-cn/windows/win32/api/fileapi/nf-fileapi-getdrivetypew
                    let driver_type = GetDriveTypeW(PCWSTR::from_raw(path.as_ptr()));
                    // 只扫描这三种驱动器类型（网络驱动器会被IO阻塞）
                    match driver_type {
                        DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_RAMDISK => {
                            if get_fs(&name).unwrap() == "NTFS" {
                                res.push(name);
                            }
                        }
                        _ => {}
                    };
                }

                mask >>= 1;
                if mask == 0 {
                    break;
                }
            }
        }
        res
    }

    pub fn iter_usn_record(&self, buffer_size: usize) -> IterUsnRecord {
        IterUsnRecord::new(*self.0, buffer_size)
    }
}

fn get_fs(name: &str) -> Result<String> {
    let path: Vec<_> = OsStr::new(&name)
        .encode_wide()
        .chain(OsStr::new(r"\").encode_wide())
        .chain(once(0))
        .collect();
    let mut fs = [0_u16; 12];
    unsafe {
        // https://learn.microsoft.com/zh-cn/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationw
        GetVolumeInformationW(
            PCWSTR::from_raw(path.as_ptr()),
            None,
            None,
            None,
            None,
            Some(&mut fs),
        )?;
    }
    // API返回的字符串以0表示结尾，但是Rust字符串不会这样认为
    let fs: Vec<_> = fs.into_iter().take_while(|x| *x != 0).collect();
    Ok(String::from_utf16_lossy(&fs))
}
