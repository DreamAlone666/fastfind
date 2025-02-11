mod usn_journal_data;
mod usn_record;

use anyhow::{ensure, Result};
use std::{
    ffi::OsStr,
    fs::File,
    mem::MaybeUninit,
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW},
        System::WindowsProgramming::{DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE},
    },
};

pub use usn_journal_data::UsnJournalData;
pub use usn_record::{FileRecords, UsnRecord, UsnRecords};

// https://github.com/microsoft/windows-rs/pull/3013
// 通过Drop自动释放HANDLE
pub struct Volume {
    driver: String,
    file: File,
}

impl Volume {
    pub fn open(driver: String) -> Result<Self> {
        let fs = driver_fs(&driver)?;
        ensure!(fs == "NTFS", "不支持的文件系统：{}", fs);

        Ok(Self {
            file: File::open(format!("{}{driver}", r"\\.\"))?,
            driver,
        })
    }

    /// 基准测试中，64KB 缓冲区占优
    pub fn file_records<const BS: usize>(&self) -> FileRecords<BS> {
        FileRecords::new(self)
    }

    pub fn usn_journal_data(&self) -> Result<UsnJournalData> {
        UsnJournalData::try_new(self)
    }

    pub fn usn_records_from<const BS: usize>(&self, id: u64, start: i64) -> UsnRecords<BS> {
        UsnRecords::with_start(self, id, start)
    }

    pub fn driver(&self) -> &str {
        &self.driver
    }

    fn as_handle(&self) -> HANDLE {
        HANDLE(self.file.as_raw_handle())
    }
}

pub fn scan_drivers() -> Vec<String> {
    let mut res = Vec::new();
    let mut mask = unsafe { GetLogicalDrives() };
    for letter in 'A'..='Z' {
        if mask & 1 == 1 {
            let driver = String::from_iter([letter, ':']);
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
