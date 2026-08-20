use std::{fs::File, mem::MaybeUninit, os::windows::io::AsRawHandle, path::Path};

use windows::Win32::{
    Foundation::HANDLE,
    System::{
        IO::DeviceIoControl,
        Ioctl::{FSCTL_GET_NTFS_VOLUME_DATA, NTFS_VOLUME_DATA_BUFFER},
    },
};

use crate::{buffer::OutputBuffer, enumerate::EnumUsnData};

/// NTFS 卷。
#[derive(Debug)]
pub struct Volume {
    file: File,
}

impl Volume {
    /// `path` 按原样传递，但只有类似 `\\.\C:` 的正确路径才受支持。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, windows_result::Error> {
        let file = File::open(path)?;
        ensure_ntfs_volume(&file)?;

        Ok(Self { file })
    }

    /// 基于 FSCTL_ENUM_USN_DATA 枚举每个文件的 USN 记录。
    ///
    /// 使用默认的缓冲区大小，如果需要配置，参考 [`EnumUsnData::with_capacity`]。
    pub fn enum_usn_data(&self) -> EnumUsnData<'_> {
        let out_buf = OutputBuffer::default();
        EnumUsnData::new(self, out_buf)
    }
}

impl AsRawHandle for Volume {
    fn as_raw_handle(&self) -> std::os::windows::prelude::RawHandle {
        self.file.as_raw_handle()
    }
}

fn ensure_ntfs_volume(handle: &impl AsRawHandle) -> Result<(), windows_result::Error> {
    // [XXX]: 不确定是否必须传入 `loutbuffer`
    let mut buf = MaybeUninit::<NTFS_VOLUME_DATA_BUFFER>::uninit();
    // https://learn.microsoft.com/zh-cn/windows/win32/api/ioapiset/nf-ioapiset-deviceiocontrol
    // > 如果 lpOverlapped 为 NULL，则 lpBytesReturned 不能为 NULL。
    let mut bytes_returned = MaybeUninit::uninit();
    let handle = HANDLE(handle.as_raw_handle());

    // https://learn.microsoft.com/zh-cn/windows/win32/api/winioctl/ni-winioctl-fsctl_get_ntfs_volume_data
    unsafe {
        DeviceIoControl(
            handle,
            FSCTL_GET_NTFS_VOLUME_DATA,
            None,
            0,
            Some(buf.as_mut_ptr().cast()),
            size_of_val(&buf) as _,
            Some(bytes_returned.as_mut_ptr()),
            None,
        )
    }
}
