use anyhow::Result;
use std::{ffi::c_void, mem::MaybeUninit};
use windows::Win32::System::{
    Ioctl::{FSCTL_QUERY_USN_JOURNAL, USN_JOURNAL_DATA_V0},
    IO::DeviceIoControl,
};

use super::Volume;

#[derive(Debug)]
pub struct UsnJournalData {
    pub id: u64,
    pub next_usn: i64,
}

impl UsnJournalData {
    unsafe fn from_raw(ptr: *const USN_JOURNAL_DATA_V0) -> Self {
        let data = &*ptr;
        Self {
            id: data.UsnJournalID,
            next_usn: data.NextUsn,
        }
    }

    pub fn try_new(vol: &Volume) -> Result<Self> {
        const BS: usize = size_of::<USN_JOURNAL_DATA_V0>();
        let mut buf = MaybeUninit::<[u8; BS]>::uninit();
        unsafe {
            DeviceIoControl(
                *vol.handle,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some(buf.as_mut_ptr() as *mut c_void),
                BS as _,
                None,
                None,
            )?;
            Ok(Self::from_raw(buf.as_ptr() as _))
        }
    }
}
