use anyhow::{anyhow, Result};
use std::{ffi::c_void, mem::MaybeUninit, ptr, slice, u32};
use windows::{
    core::Owned,
    Win32::{
        Foundation::{ERROR_HANDLE_EOF, HANDLE},
        System::{
            Ioctl::{
                FSCTL_ENUM_USN_DATA, FSCTL_READ_USN_JOURNAL, MFT_ENUM_DATA_V1,
                READ_USN_JOURNAL_DATA_V0, USN_REASON_CLOSE, USN_REASON_FILE_CREATE,
                USN_REASON_FILE_DELETE, USN_REASON_RENAME_NEW_NAME, USN_RECORD_V2,
            },
            IO::DeviceIoControl,
        },
    },
};

use super::Volume;

#[derive(Debug)]
pub struct UsnRecord {
    pub frn: u64,
    pub parent_frn: u64,
    pub filename: String,
    pub reason: u32,
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
            reason: record.Reason,
            length: record.RecordLength,
        }
    }
}

pub struct IterFileRecord<'a, const BS: usize> {
    handle: &'a Owned<HANDLE>,
    in_buf: MFT_ENUM_DATA_V1,
    out_buf: IterRecordBuf<BS>,
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
            out_buf: IterRecordBuf::new_uninit(),
        }
    }
}

impl<const BS: usize> Iterator for IterFileRecord<'_, BS> {
    type Item = Result<UsnRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(record) = self.out_buf.next() {
            return Some(Ok(record));
        }

        unsafe {
            if let Err(e) = DeviceIoControl(
                **self.handle,
                FSCTL_ENUM_USN_DATA,
                Some(&self.in_buf as *const _ as *const c_void),
                size_of_val(&self.in_buf) as _,
                Some(self.out_buf.buf.as_mut_ptr() as *mut c_void),
                BS as _,
                Some(&mut self.out_buf.left_bytes),
                None,
            ) {
                // https://learn.microsoft.com/zh-cn/windows/win32/api/winerror/nf-winerror-hresult_code
                if (e.code().0 & 0xFFFF) as u32 == ERROR_HANDLE_EOF.0 {
                    return None;
                } else {
                    return Some(Err(e.into()));
                }
            };
            self.in_buf.StartFileReferenceNumber = self.out_buf.reload();
        }

        match self.out_buf.next() {
            Some(r) => Some(Ok(r)),
            None => Some(Err(anyhow!("缓冲区过小: {BS}B"))),
        }
    }
}

pub struct IterUsnRecord<'a, const BS: usize> {
    handle: &'a Owned<HANDLE>,
    in_buf: READ_USN_JOURNAL_DATA_V0,
    out_buf: IterRecordBuf<BS>,
}

impl<'a, const BS: usize> IterUsnRecord<'a, BS> {
    pub fn with_start(vol: &'a Volume, id: u64, start: i64) -> Self {
        const MASK: u32 = USN_REASON_FILE_CREATE
            | USN_REASON_FILE_DELETE
            | USN_REASON_RENAME_NEW_NAME
            | USN_REASON_CLOSE;
        Self {
            handle: &vol.handle,
            // https://learn.microsoft.com/zh-cn/windows/win32/api/winioctl/ns-winioctl-read_usn_journal_data_v0
            in_buf: READ_USN_JOURNAL_DATA_V0 {
                StartUsn: start,
                ReasonMask: MASK,
                ReturnOnlyOnClose: 1,
                Timeout: 0,
                BytesToWaitFor: 0,
                UsnJournalID: id,
            },
            out_buf: IterRecordBuf::new_uninit(),
        }
    }

    pub fn next_usn(&self) -> i64 {
        self.in_buf.StartUsn
    }
}

impl<const BS: usize> Iterator for IterUsnRecord<'_, BS> {
    type Item = Result<UsnRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(record) = self.out_buf.next() {
            return Some(Ok(record));
        }

        unsafe {
            if let Err(e) = DeviceIoControl(
                **self.handle,
                FSCTL_READ_USN_JOURNAL,
                Some(&self.in_buf as *const _ as *const c_void),
                size_of_val(&self.in_buf) as _,
                Some(self.out_buf.buf.as_mut_ptr() as *mut c_void),
                BS as _,
                Some(&mut self.out_buf.left_bytes),
                None,
            ) {
                return Some(Err(e.into()));
            }
            let usn: i64 = self.out_buf.reload();
            if usn == self.in_buf.StartUsn {
                return None;
            }
            self.in_buf.StartUsn = usn;
        }

        match self.out_buf.next() {
            Some(r) => Some(Ok(r)),
            None => Some(Err(anyhow!("缓冲区过小: {BS}B"))),
        }
    }
}

struct IterRecordBuf<const BS: usize> {
    buf: MaybeUninit<[u8; BS]>,
    left_bytes: u32,
    ptr: *const USN_RECORD_V2,
}

impl<const BS: usize> IterRecordBuf<BS> {
    fn new_uninit() -> Self {
        Self {
            buf: MaybeUninit::uninit(),
            left_bytes: 0,
            ptr: ptr::null(),
        }
    }

    /// 当缓冲区被重新装填时，请调用此函数重载。
    ///
    /// 解析并返回缓冲区的第一个数。
    unsafe fn reload<T: Copy>(&mut self) -> T {
        // 缓冲区最前头应该是一个 64 位数，后面跟着尽可能多的 USN 记录
        let ptr = self.buf.as_ptr();
        let usn = *(ptr as *const T);
        self.ptr = ptr.byte_add(size_of::<u64>()) as _;
        self.left_bytes -= size_of::<T>() as u32;
        usn
    }
}

impl<const BS: usize> Iterator for IterRecordBuf<BS> {
    type Item = UsnRecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.left_bytes == 0 {
            return None;
        }

        let record;
        unsafe {
            record = UsnRecord::from_raw(self.ptr);
            self.ptr = self.ptr.byte_add(record.length as _);
        }
        self.left_bytes -= record.length;
        Some(record)
    }
}
