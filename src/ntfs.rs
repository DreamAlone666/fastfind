use anyhow::{ensure, Result};
use std::{
    ffi::{c_void, OsStr},
    iter::once,
    os::windows::ffi::OsStrExt,
    ptr, slice,
};
use windows::{
    core::{Owned, PCWSTR},
    Win32::{
        Foundation::{GENERIC_READ, HANDLE},
        Storage::FileSystem::{
            CreateFileW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
            FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            Ioctl::{FSCTL_ENUM_USN_DATA, MFT_ENUM_DATA_V1, USN_RECORD_V2},
            WindowsProgramming::{DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE},
            IO::DeviceIoControl,
        },
    },
};

// https://github.com/microsoft/windows-rs/pull/3013
// 通过Drop自动释放HANDLE
pub struct Volume(Owned<HANDLE>);
pub struct USNRecord {
    pub frn: u64,
    pub parent_frn: u64,
    pub filename: String,
    length: u32,
}
pub struct IterUSNRecord {
    handle: HANDLE,
    in_buf: MFT_ENUM_DATA_V1,
    out_buf: Vec<u8>,
    left_bytes: u32,
    ptr: *const USN_RECORD_V2,
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

    pub fn iter_usn_record(&self, buffer_size: usize) -> IterUSNRecord {
        IterUSNRecord::new(*self.0, buffer_size)
    }
}

impl USNRecord {
    unsafe fn from_raw(ptr: *const USN_RECORD_V2) -> Self {
        let record = &*ptr;
        let filename = slice::from_raw_parts(
            ptr.byte_add(record.FileNameOffset.into()) as *const u16,
            (record.FileNameLength / 2).into(),
        );
        Self {
            filename: String::from_utf16_lossy(filename),
            frn: record.FileReferenceNumber,
            parent_frn: record.ParentFileReferenceNumber,
            length: record.RecordLength,
        }
    }
}

impl IterUSNRecord {
    fn new(handle: HANDLE, buffer: usize) -> Self {
        Self {
            handle,
            in_buf: MFT_ENUM_DATA_V1 {
                StartFileReferenceNumber: 0, // FSCTL_ENUM_USN_DATA要求从0开始
                LowUsn: 0,
                HighUsn: i64::MAX,
                MinMajorVersion: 2,
                MaxMajorVersion: 2,
            },
            out_buf: vec![0; buffer], // 输出缓冲区，越大一次性得到的输出越多
            left_bytes: 0,
            ptr: ptr::null(),
        }
    }
}

impl Iterator for IterUSNRecord {
    type Item = USNRecord;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.left_bytes <= 0 {
                let res = DeviceIoControl(
                    self.handle,
                    FSCTL_ENUM_USN_DATA,
                    // 直接转成空指针似乎不行，要转两次
                    Some(&self.in_buf as *const _ as *const c_void),
                    size_of_val(&self.in_buf) as _,
                    Some(self.out_buf.as_mut_ptr() as *mut c_void),
                    size_of_val(self.out_buf.as_slice()) as _,
                    Some(&mut self.left_bytes),
                    None,
                );

                if res.is_err() {
                    return None;
                }
                // 缓冲区最前头是一个u64数，后面跟着尽可能多的USN记录
                let ptr = self.out_buf.as_ptr();
                self.in_buf.StartFileReferenceNumber = *(ptr as *const u64);
                self.ptr = ptr.byte_add(size_of::<u64>()) as _;
                self.left_bytes -= size_of::<u64>() as u32;
            }

            if self.left_bytes > 0 {
                let record = USNRecord::from_raw(self.ptr);
                self.ptr = self.ptr.byte_add(record.length as _);
                self.left_bytes -= record.length;
                return Some(record);
            }
        }

        None
    }
}
