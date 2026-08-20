use windows::Win32::System::Ioctl::USN_RECORD_V2;

/// 一个借用的 USN 记录。
#[derive(Debug, Clone)]
pub struct UsnRecordView<'a> {
    inner: &'a USN_RECORD_V2,
}
