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

use crate::{buffer::OutputBuffer, record::UsnRecordView, volume::Volume};

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

    /// 缓冲区大小至少为 `bytes` 个字节，且会向上取整至 8 的倍数。
    pub fn with_capacity(volume: &'a Volume, bytes: usize) -> Self {
        Self::new(volume, OutputBuffer::with_capacity(bytes))
    }
}

impl EnumUsnData<'_> {
    /// Lending Iterator 实现，配合 `while let` 使用。
    pub fn next_buffer(&mut self) -> Result<Option<EnumUsnDataBuffer<'_>>, NextBufferError> {
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
            return Err(NextBufferError::NoProgress {
                frn: next_frn,
                capacity: self.out_buf.capacity(),
            });
        }
        self.in_buf.StartFileReferenceNumber = next_frn;

        Ok(Some(EnumUsnDataBuffer::new(records)))
    }
}

#[derive(Debug, Error)]
pub enum NextBufferError {
    #[error("FileReferenceNumber ({frn}) 没有推进，缓冲区可能过小 ({capacity})")]
    NoProgress { frn: u64, capacity: usize },
    #[error(transparent)]
    Windows(#[from] windows_result::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumUsnDataBuffer<'a> {
    inner: &'a [u8],
}

impl<'a> EnumUsnDataBuffer<'a> {
    fn new(inner: &'a [u8]) -> Self {
        Self { inner }
    }

    /// Lending Iterator 实现，配合 `while let` 使用。
    pub fn next_record(&mut self) -> Option<UsnRecordView<'_>> {
        let record = unsafe { UsnRecordView::from_bytes(self.inner) }?;
        let len = record.record_length() as _;
        self.inner = &self.inner[len..];

        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use super::EnumUsnDataBuffer;
    use crate::record::test_fixtures::{SPOT_FIX, TXF_LOG_BLF, as_bytes};

    #[test]
    fn next_record_advances_by_record_length_including_padding() {
        let mut words = Vec::with_capacity(SPOT_FIX.len() + TXF_LOG_BLF.len());
        words.extend_from_slice(SPOT_FIX);
        words.extend_from_slice(TXF_LOG_BLF);

        let mut buffer = EnumUsnDataBuffer::new(as_bytes(&words));

        assert_eq!(
            buffer.next_record().unwrap().record_length() as usize,
            size_of_val(SPOT_FIX)
        );
        assert_eq!(
            buffer.next_record().unwrap().record_length() as usize,
            size_of_val(TXF_LOG_BLF)
        );
        assert!(buffer.next_record().is_none());
    }
}
