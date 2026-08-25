use std::{marker::PhantomData, mem::MaybeUninit, os::windows::io::AsRawHandle, ptr};

use thiserror::Error;
use windows::Win32::{
    Foundation::{ERROR_HANDLE_EOF, HANDLE},
    System::{
        IO::DeviceIoControl,
        Ioctl::{FSCTL_ENUM_USN_DATA, MFT_ENUM_DATA_V1},
    },
};
use windows_result::HRESULT;

use crate::{buffer::OutputBuffer, record::UsnRecords, volume::Volume};

/// 基于 `FSCTL_ENUM_USN_DATA` 枚举每个文件的 USN 记录的迭代器。
#[derive(Debug)]
pub struct EnumUsnData<'a> {
    handle: HANDLE,
    in_buf: MFT_ENUM_DATA_V1,
    out_buf: OutputBuffer,
    _marker: PhantomData<&'a Volume>,
}

impl<'a> EnumUsnData<'a> {
    pub(crate) fn new(volume: &'a Volume, out_buf: OutputBuffer) -> Self {
        // https://learn.microsoft.com/zh-cn/windows/win32/api/winioctl/ns-winioctl-mft_enum_data_v1
        let in_buf = MFT_ENUM_DATA_V1 {
            StartFileReferenceNumber: 0,
            LowUsn: 0,
            HighUsn: i64::MAX,
            MinMajorVersion: 2,
            MaxMajorVersion: 2,
        };

        Self {
            handle: HANDLE(volume.as_raw_handle()),
            in_buf,
            out_buf,
            _marker: PhantomData,
        }
    }

    /// 缓冲区大小至少为 `bytes` 个字节，且会向上取整至 8 的倍数。
    pub fn with_capacity(volume: &'a Volume, bytes: usize) -> Self {
        Self::new(volume, OutputBuffer::with_capacity(bytes))
    }
}

impl EnumUsnData<'_> {
    /// Lending Iterator 实现，配合 `while let` 使用。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::error::Error;
    /// # use fastfind_ntfs::volume::Volume;
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// let volume = Volume::open(r"\\.\C:")?;
    /// // 重要：必须存为变量，避免 `while let` 每轮重复创建迭代器导致进度重置
    /// let mut iter = volume.enum_usn_data();
    ///
    /// while let Some(mut records) = iter.next_records()? {
    ///     while let Some(record) = records.next_record() {
    ///         println!("{}", record.file_name().to_string_lossy());
    ///     }
    /// }
    /// ```
    pub fn next_records(&mut self) -> Result<Option<UsnRecords<'_>>, EnumUsnDataError> {
        let mut bytes_returned = MaybeUninit::uninit();
        unsafe {
            // https://learn.microsoft.com/zh-cn/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data
            if let Err(e) = DeviceIoControl(
                self.handle,
                FSCTL_ENUM_USN_DATA,
                Some(ptr::from_ref(&self.in_buf).cast()),
                size_of_val(&self.in_buf) as _,
                Some(self.out_buf.as_mut_c_void()),
                self.out_buf.capacity() as _,
                Some(bytes_returned.as_mut_ptr()),
                None,
            ) {
                // ioctl 完成
                const E_HANDLE_EOF: HRESULT = ERROR_HANDLE_EOF.to_hresult();
                if e.code() == E_HANDLE_EOF {
                    return Ok(None);
                }
                return Err(e.into());
            };

            self.out_buf.set_len(bytes_returned.assume_init() as _);
        }

        let (next_frn, records) = self
            .out_buf
            .split_first_u64()
            // [HACK]: 暂不做错误处理，让我们相信 Windows
            .expect("成功的 `FSCTL_ENUM_USN_DATA` 应当填充非空数据");
        // [NOTE]: 实测会遇到写入了相同 frn 却成功返回，而不报 ERROR_INSUFFICIENT_BUFFER 的情况，
        // 这样做可以避免死循环，不过会丢弃非空记录，但同时出现这两种情况还是比较猎奇的
        if self.in_buf.StartFileReferenceNumber == next_frn {
            return Err(EnumUsnDataError::NoProgress {
                frn: next_frn,
                capacity: self.out_buf.capacity(),
            });
        }
        self.in_buf.StartFileReferenceNumber = next_frn;

        Ok(Some(unsafe { UsnRecords::from_bytes(records) }))
    }
}

#[derive(Debug, Error)]
pub enum EnumUsnDataError {
    #[error("FileReferenceNumber ({frn}) 没有推进，缓冲区可能过小 ({capacity})")]
    NoProgress { frn: u64, capacity: usize },
    #[error(transparent)]
    Windows(#[from] windows_result::Error),
}
