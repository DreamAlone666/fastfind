use std::ffi::c_void;

/// 8 字节对齐的输出缓冲区。
#[derive(Debug)]
pub(crate) struct OutputBuffer(Vec<u64>);

impl OutputBuffer {
    /// 构造至少 `bytes` 个字节的空缓冲区，向上取整直至 8 的倍数。
    pub(crate) fn with_capacity(bytes: usize) -> Self {
        let capacity = bytes.div_ceil(8);
        Self(Vec::with_capacity(capacity))
    }

    /// 返回缓冲区的字节总容量。
    pub(crate) fn capacity(&self) -> usize {
        self.0.capacity() * 8
    }

    pub(crate) fn as_mut_c_void(&mut self) -> *mut c_void {
        self.0.as_mut_ptr().cast()
    }

    /// 设置缓冲区的有效长度。
    ///
    /// # Safety
    /// `bytes_returned` 必须 <= `capacity()` 且是 8 的倍数，长度范围内必须被初始化。
    ///
    /// https://learn.microsoft.com/zh-cn/windows/win32/api/winioctl/ns-winioctl-usn_record_v2:
    /// > 在从使用 USN_RECORD_V2 的 DeviceIoControl 操作返回 的输出缓冲区中，所有记录都与缓冲区开头的 64 位边界对齐。
    ///
    /// 因此搭配 FFI 返回的 `bytes_returned` 使用一般都是安全的。
    pub(crate) unsafe fn set_len(&mut self, bytes_returned: usize) {
        debug_assert!(
            bytes_returned.is_multiple_of(8),
            "`bytes_returned = {}` 不是 8 的倍数",
            bytes_returned
        );

        unsafe { self.0.set_len(bytes_returned / 8) };
    }

    /// 如果缓冲区有效长度足够，则返回第一个 u64，剩下的作为切片。
    pub(crate) fn split_first_u64(&self) -> Option<(u64, &[u8])> {
        let (&first, rest) = self.0.split_first()?;
        // SAFETY: Vec<u64> 字节长度永远是 8 的倍数
        let rest = unsafe { std::slice::from_raw_parts(rest.as_ptr().cast(), rest.len() * 8) };

        Some((first, rest))
    }
}

impl Default for OutputBuffer {
    fn default() -> Self {
        // 64 KB 在基准测试中占优
        const CAPACITY: usize = 64 * 1024 / 8;
        Self(Vec::with_capacity(CAPACITY))
    }
}
