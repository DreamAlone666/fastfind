use std::{marker::PhantomData, mem::MaybeUninit, os::windows::io::AsRawHandle};

use windows::Win32::{
    Foundation::{ERROR_HANDLE_EOF, HANDLE},
    System::{
        IO::DeviceIoControl,
        Ioctl::{FSCTL_ENUM_USN_DATA, MFT_ENUM_DATA_V1},
    },
};
use windows_result::HRESULT;

use crate::{buffer::OutputBuffer, volume::Volume};

/// 基于 `FSCTL_ENUM_USN_DATA` 枚举每个文件的 USN 记录。
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

    pub fn with_capacity(volume: &'a Volume, bytes: usize) -> Self {
        Self::new(volume, OutputBuffer::with_capacity(bytes))
    }
}

impl EnumUsnData<'_> {
    /// Lending Iterator 实现，配合 `while let` 使用。
    pub fn next_batch(&mut self) -> windows_result::Result<Option<EnumUsnDataBuffer<'_>>> {
        let mut bytes_returned = MaybeUninit::uninit();
        unsafe {
            // https://learn.microsoft.com/zh-cn/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data
            if let Err(e) = DeviceIoControl(
                self.handle,
                FSCTL_ENUM_USN_DATA,
                Some(&self.in_buf as *const _ as _),
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
                return Err(e);
            };

            self.out_buf.set_len(bytes_returned.assume_init() as _);
        }

        let (next_frn, records) = self
            .out_buf
            .split_first_u64()
            // [HACK]: 暂不做错误处理，让我们相信 Windows
            .expect("成功的 `FSCTL_ENUM_USN_DATA` 应当填充非空数据");
        self.in_buf.StartFileReferenceNumber = next_frn;

        Ok(Some(EnumUsnDataBuffer::new(records)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumUsnDataBuffer<'a>(&'a [u8]);

impl<'a> EnumUsnDataBuffer<'a> {
    fn new(inner: &'a [u8]) -> Self {
        Self(inner)
    }
}
