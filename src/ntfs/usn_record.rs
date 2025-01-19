use log::{debug, error};
use std::{ffi::c_void, mem::MaybeUninit, ptr, slice, usize};
use windows::{
    core::Owned,
    Win32::{
        Foundation::HANDLE,
        System::{
            Ioctl::{FSCTL_ENUM_USN_DATA, MFT_ENUM_DATA_V1, USN_RECORD_V2},
            IO::DeviceIoControl,
        },
    },
};

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

pub struct IterUsnRecord<'a, const BS: usize> {
    handle: &'a Owned<HANDLE>,
    in_buf: MFT_ENUM_DATA_V1,
    out_buf: MaybeUninit<[u8; BS]>,
    left_bytes: u32,
    ptr: *const USN_RECORD_V2,
}

impl<'a, const BS: usize> IterUsnRecord<'a, BS> {
    pub(super) fn from_handle(handle: &'a Owned<HANDLE>) -> Self {
        Self {
            handle,
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

impl<const BS: usize> Iterator for IterUsnRecord<'_, BS> {
    type Item = UsnRecord;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.left_bytes <= 0 {
                DeviceIoControl(
                    **self.handle,
                    FSCTL_ENUM_USN_DATA,
                    Some(&self.in_buf as *const _ as *const c_void),
                    size_of_val(&self.in_buf) as _,
                    Some(self.out_buf.as_mut_ptr() as *mut c_void),
                    size_of_val(self.out_buf.assume_init_ref()) as _,
                    Some(&mut self.left_bytes),
                    None,
                )
                .inspect_err(|e| debug!("{e}"))
                .ok()?;

                // 缓冲区只够前导的u64数
                if self.left_bytes == size_of::<u64>() as _ {
                    error!("缓冲区过小：{}B", BS);
                    return None;
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
            Some(record)
        }
    }
}
