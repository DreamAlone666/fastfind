use anyhow::{anyhow, Result};
use std::{ffi::c_void, mem::MaybeUninit, ptr, slice};
use windows::{
    core::Owned,
    Win32::{
        Foundation::{ERROR_HANDLE_EOF, HANDLE},
        System::{
            Ioctl::{FSCTL_ENUM_USN_DATA, MFT_ENUM_DATA_V1, USN_RECORD_V2},
            IO::DeviceIoControl,
        },
    },
};

use super::Volume;

pub struct UsnRecord {
    pub frn: u64,
    pub parent_frn: u64,
    pub filename: String,
    length: u32,
}

impl UsnRecord {
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

pub struct IterFileRecord<'a, const BS: usize> {
    handle: &'a Owned<HANDLE>,
    in_buf: MFT_ENUM_DATA_V1,
    out_buf: MaybeUninit<[u8; BS]>,
    left_bytes: u32,
    ptr: *const USN_RECORD_V2,
}

impl<'a, const BS: usize> IterFileRecord<'a, BS> {
    pub fn new(volume: &'a Volume) -> Self {
        Self {
            handle: &volume.handle,
            in_buf: MFT_ENUM_DATA_V1 {
                StartFileReferenceNumber: 0, // FSCTL_ENUM_USN_DATA要求从0开始
                LowUsn: 0,
                HighUsn: i64::MAX,
                MinMajorVersion: 2,
                MaxMajorVersion: 2,
            },
            out_buf: MaybeUninit::uninit(), // 输出缓冲区，越大一次性得到的输出越多
            left_bytes: 0,
            ptr: ptr::null(),
        }
    }
}

impl<const BS: usize> Iterator for IterFileRecord<'_, BS> {
    type Item = Result<UsnRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.left_bytes == 0 {
                if let Err(e) = DeviceIoControl(
                    **self.handle,
                    FSCTL_ENUM_USN_DATA,
                    Some(&self.in_buf as *const _ as *const c_void),
                    size_of_val(&self.in_buf) as _,
                    Some(self.out_buf.as_mut_ptr() as *mut c_void),
                    size_of_val(self.out_buf.assume_init_ref()) as _,
                    Some(&mut self.left_bytes),
                    None,
                ) {
                    // https://learn.microsoft.com/zh-cn/windows/win32/api/winerror/nf-winerror-hresult_code
                    if (e.code().0 & 0xFFFF) as u32 == ERROR_HANDLE_EOF.0 {
                        return None;
                    } else {
                        return Some(Err(e.into()));
                    }
                };

                // 缓冲区只够前导的u64数
                if self.left_bytes == size_of::<u64>() as _ {
                    return Some(Err(anyhow!("缓冲区过小：{}B", BS)));
                }

                // 缓冲区最前头是一个u64数，后面跟着尽可能多的USN记录
                let ptr = self.out_buf.as_ptr();
                self.in_buf.StartFileReferenceNumber = *(ptr as *const u64);
                self.ptr = ptr.byte_add(size_of::<u64>()) as _;
                self.left_bytes -= size_of::<u64>() as u32;
            }

            let record = UsnRecord::from_raw(self.ptr);
            self.ptr = self.ptr.byte_add(record.length as _);
            self.left_bytes -= record.length;
            Some(Ok(record))
        }
    }
}
