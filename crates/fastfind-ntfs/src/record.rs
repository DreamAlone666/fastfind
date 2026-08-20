use std::slice;

use windows::Win32::System::Ioctl::USN_RECORD_V2;

/// 一个借用的 USN 记录。
#[derive(Debug, Clone)]
pub struct UsnRecordView<'a> {
    inner: &'a USN_RECORD_V2,
    file_name: U16Str<'a>,
}

impl<'a> UsnRecordView<'a> {
    /// 如果字节切片容量充足，则从首部构造 USN 记录。
    ///
    /// # Safety
    ///
    /// `bytes` 必须 8 字节对齐，内容首部必须是合法的 [`USN_RECORD_V2`]。
    ///
    /// 更多具体的要求：
    /// - `RecordLength <= bytes.len()`
    /// - `FileNameOffset + FileNameLength <= RecordLength`
    /// - `FileNameLength` 为偶数
    /// - `FileNameOffset` 满足 u16 对齐
    pub(crate) unsafe fn from_bytes(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < size_of::<USN_RECORD_V2>() {
            return None;
        }

        let ptr = bytes.as_ptr();
        let inner = unsafe { ptr.cast::<USN_RECORD_V2>().as_ref_unchecked() };

        let offset = inner.FileNameOffset.into();
        let name_bytes: usize = inner.FileNameLength.into();
        debug_assert!(
            offset + name_bytes <= bytes.len(),
            "FileName 越界 bytes, offset = {}, name_bytes = {}, bytes = {}",
            offset,
            name_bytes,
            bytes.len()
        );
        debug_assert!(
            inner.RecordLength as usize <= bytes.len(),
            "RecordLength = {} 越界 bytes = {}",
            inner.RecordLength,
            bytes.len()
        );
        debug_assert!(
            name_bytes.is_multiple_of(2),
            "FileNameLength = {} 不是偶数",
            name_bytes
        );
        let name_ptr = unsafe { ptr.add(offset).cast() };
        let name_slice = unsafe { slice::from_raw_parts(name_ptr, name_bytes / 2) };
        let file_name = U16Str(name_slice);

        Some(Self { inner, file_name })
    }

    /// 记录的总长度（以字节为单位）。
    pub fn record_length(&self) -> u32 {
        self.inner.RecordLength
    }

    /// 与此记录关联的文件或目录的名称，采用 Unicode 格式。
    pub fn file_name(&self) -> U16Str<'_> {
        self.file_name
    }
}

/// widestring 风格的 16 位宽未知编码字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct U16Str<'a>(&'a [u16]);

impl U16Str<'_> {
    pub fn to_string_lossy(self) -> String {
        String::from_utf16_lossy(self.0)
    }
}

#[cfg(test)]
pub(crate) mod test_fixtures {
    use std::{mem::size_of_val, slice};

    // The crate targets Windows, whose native endian is little endian. Storing the
    // captured words this way mirrors OutputBuffer's Vec<u64> representation.
    /// Captured from `FSCTL_ENUM_USN_DATA`; the 8-byte continuation header is omitted.
    pub const SPOT_FIX: &[u64] = &[
        0x0000000200000070,
        0x0013000000000013,
        0x0012000000000012,
        0x0000000000000000,
        0x0000000000000000,
        0x0000000000000000,
        0x0000008000000000,
        0x00700053003C0032,
        0x006900460074006F,
        0x0032003000320078,
        0x0031003400300035,
        0x0032003000310037,
        0x002E003400300037,
        0x00000067006F006C,
    ];

    /// Captured from `FSCTL_ENUM_USN_DATA`; the 8-byte continuation header is omitted.
    pub const TXF_LOG_BLF: &[u64] = &[
        0x0000000200000058,
        0x0001000000000021,
        0x000100000000001E,
        0x0000000B38736C18,
        0x0000000000000000,
        0x0000000000000000,
        0x0000002000000000,
        0x00540024003C0016,
        0x006F004C00660078,
        0x006C0062002E0067,
        0x0000000000000066,
    ];

    /// Views aligned captured words as the byte payload expected by the parser.
    pub fn as_bytes(words: &[u64]) -> &[u8] {
        // SAFETY: `words` is initialized and u64-aligned; the crate runs on
        // little-endian Windows, matching the captured wire representation.
        unsafe { slice::from_raw_parts(words.as_ptr().cast::<u8>(), size_of_val(words)) }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of_val;

    use super::{UsnRecordView, test_fixtures::*};

    #[test]
    fn parses_a_captured_record_with_padding() {
        let record = unsafe { UsnRecordView::from_bytes(as_bytes(SPOT_FIX)) }.unwrap();

        assert_eq!(record.record_length() as usize, size_of_val(SPOT_FIX));
        assert_eq!(record.file_name.0.len(), 25);
    }
}
