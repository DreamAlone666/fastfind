use std::slice;

use bitflags::bitflags;
use windows::Win32::System::Ioctl::{
    USN_REASON_BASIC_INFO_CHANGE, USN_REASON_CLOSE, USN_REASON_COMPRESSION_CHANGE,
    USN_REASON_DATA_EXTEND, USN_REASON_DATA_OVERWRITE, USN_REASON_DATA_TRUNCATION,
    USN_REASON_EA_CHANGE, USN_REASON_ENCRYPTION_CHANGE, USN_REASON_FILE_CREATE,
    USN_REASON_FILE_DELETE, USN_REASON_HARD_LINK_CHANGE, USN_REASON_INDEXABLE_CHANGE,
    USN_REASON_NAMED_DATA_EXTEND, USN_REASON_NAMED_DATA_OVERWRITE,
    USN_REASON_NAMED_DATA_TRUNCATION, USN_REASON_OBJECT_ID_CHANGE, USN_REASON_RENAME_NEW_NAME,
    USN_REASON_RENAME_OLD_NAME, USN_REASON_REPARSE_POINT_CHANGE, USN_REASON_SECURITY_CHANGE,
    USN_REASON_STREAM_CHANGE, USN_RECORD_V2,
};

pub type Usn = i64;

/// 一批 USN 记录的迭代器，通常借用自缓冲区。
#[derive(Debug, Clone)]
pub struct UsnRecords<'a> {
    inner: &'a [u8],
}

impl<'a> UsnRecords<'a> {
    /// # Safety
    ///
    /// 参见 [`UsnRecordView::from_bytes`]。
    pub(crate) unsafe fn from_bytes(bytes: &'a [u8]) -> Self {
        Self { inner: bytes }
    }

    /// Lending Iterator 实现，配合 `while let` 使用。
    pub fn next_record(&mut self) -> Option<UsnRecordView<'_>> {
        let record = unsafe { UsnRecordView::from_bytes(self.inner) }?;
        let len = record.record_length() as _;
        self.inner = &self.inner[len..];

        Some(record)
    }
}

/// 一个借用的 USN 记录。
#[derive(Debug, Clone)]
pub struct UsnRecordView<'a> {
    // https://learn.microsoft.com/zh-cn/windows/win32/api/winioctl/ns-winioctl-usn_record_v2
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
            inner.RecordLength as usize <= bytes.len(),
            "RecordLength = {} 越界 bytes = {}",
            inner.RecordLength,
            bytes.len()
        );
        debug_assert!(
            offset + name_bytes <= inner.RecordLength as usize,
            "FileName 越界 RecordLength, FileNameOffset = {}, FileNameLength = {}, RecordLength = {}",
            offset,
            name_bytes,
            inner.RecordLength
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

    /// 自文件或目录打开以来在此记录中累积的更改原因标志。
    pub fn reasons(&self) -> UsnReasons {
        UsnReasons::from_bits_retain(self.inner.Reason)
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

bitflags! {
    /// 标识自文件或目录打开以来在记录中累积的更改原因标志。
    ///
    /// 当文件或目录关闭时，会生成具有 [`UsnReasons::CLOSE`] 标志的最终 USN 记录。
    /// 下一个更改发生时，会使用一组新的原因标志启动新记录。
    ///
    /// 重命名或移动操作将生成两条 USN 记录，一条记录项的旧父目录，一条记录新父目录。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct UsnReasons: u32 {
        /// 文件或目录中的数据被覆盖。
        const DATA_OVERWRITE          = USN_REASON_DATA_OVERWRITE;
        /// 文件或目录被追加。
        const DATA_EXTEND             = USN_REASON_DATA_EXTEND;
        /// 文件或目录被截断。
        const DATA_TRUNCATION         = USN_REASON_DATA_TRUNCATION;
        /// 文件的一个或多个命名数据流中的数据被覆盖。
        const NAMED_DATA_OVERWRITE    = USN_REASON_NAMED_DATA_OVERWRITE;
        /// 文件的一个或多个命名数据流被追加。
        const NAMED_DATA_EXTEND       = USN_REASON_NAMED_DATA_EXTEND;
        /// 文件的一个或多个命名数据流被截断。
        const NAMED_DATA_TRUNCATION   = USN_REASON_NAMED_DATA_TRUNCATION;
        /// 首次创建文件或目录。
        const FILE_CREATE             = USN_REASON_FILE_CREATE;
        /// 文件或目录被删除。
        const FILE_DELETE             = USN_REASON_FILE_DELETE;
        /// 用户对文件或目录扩展属性进行更改。基于 Windows 的应用程序无法访问这些 NTFS 文件系统属性。
        const EA_CHANGE               = USN_REASON_EA_CHANGE;
        /// 对文件或目录的访问权限进行了更改。
        const SECURITY_CHANGE         = USN_REASON_SECURITY_CHANGE;
        /// 文件或目录被重命名，且记录中的是旧命名。
        const RENAME_OLD_NAME         = USN_REASON_RENAME_OLD_NAME;
        /// 文件或目录被重命名，且记录中的是新命名。
        const RENAME_NEW_NAME         = USN_REASON_RENAME_NEW_NAME;
        /// 用户更改了 FILE_ATTRIBUTE_NOT_CONTENT_INDEXED 属性。
        /// 也就是说，用户将文件或目录从可编制内容索引的文件或目录更改为不能编制索引的文件或目录，反之亦然。
        /// （内容索引允许通过构建所选 content 的数据库来快速搜索数据。）
        const INDEXABLE_CHANGE        = USN_REASON_INDEXABLE_CHANGE;
        /// 用户已更改一个或多个文件或目录属性（如只读属性、隐藏属性、系统属性、存档属性或稀疏属性），或者一个或多个时间戳。
        const BASIC_INFO_CHANGE       = USN_REASON_BASIC_INFO_CHANGE;
        /// 在文件或目录中添加或删除 NTFS 文件系统硬链接。类似于 POSIX 硬链接的 NTFS 文件系统硬链接是查看同一文件或目录的多个目录条目之一。
        const HARD_LINK_CHANGE        = USN_REASON_HARD_LINK_CHANGE;
        /// 文件或目录的压缩状态被更改。
        const COMPRESSION_CHANGE      = USN_REASON_COMPRESSION_CHANGE;
        /// 文件或目录被加密或解密。
        const ENCRYPTION_CHANGE       = USN_REASON_ENCRYPTION_CHANGE;
        /// 文件或目录的对象标识符被更改。
        const OBJECT_ID_CHANGE        = USN_REASON_OBJECT_ID_CHANGE;
        /// 文件或目录中包含的重新分析点被更改，或者将重分析点添加到文件或目录中，或者从文件或目录中删除重分析点。
        const REPARSE_POINT_CHANGE    = USN_REASON_REPARSE_POINT_CHANGE;
        /// 在文件或目录中添加或删除命名流，或者重命名命名流。
        const STREAM_CHANGE           = USN_REASON_STREAM_CHANGE;
        /// 文件或目录被关闭。
        const CLOSE                   = USN_REASON_CLOSE;
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

    use super::{UsnRecordView, UsnRecords, test_fixtures::*};

    #[test]
    fn parses_a_captured_record_with_padding() {
        let record = unsafe { UsnRecordView::from_bytes(as_bytes(SPOT_FIX)) }.unwrap();

        assert_eq!(record.record_length() as usize, size_of_val(SPOT_FIX));
        assert_eq!(record.file_name.0.len(), 25);
    }

    #[test]
    fn next_record_advances_by_record_length_including_padding() {
        let mut words = Vec::with_capacity(SPOT_FIX.len() + TXF_LOG_BLF.len());
        words.extend_from_slice(SPOT_FIX);
        words.extend_from_slice(TXF_LOG_BLF);

        let mut buffer = unsafe { UsnRecords::from_bytes(as_bytes(&words)) };

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
