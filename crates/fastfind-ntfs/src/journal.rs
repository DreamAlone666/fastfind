use std::{marker::PhantomData, mem::MaybeUninit, os::windows::io::AsRawHandle, ptr};

use windows::Win32::{
    Foundation::HANDLE,
    System::{
        IO::DeviceIoControl,
        Ioctl::{
            FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0,
            USN_JOURNAL_DATA_V0,
        },
    },
};

use crate::{
    buffer::OutputBuffer,
    record::{Usn, UsnReasons, UsnRecords},
    volume::Volume,
};

pub type UsnJournalId = u64;

/// USN 日志信息。
///
/// 仅代表当前时间点日志信息的快照，不进行实时同步。
pub struct UsnJournal {
    inner: USN_JOURNAL_DATA_V0,
}

impl UsnJournal {
    /// 基于 `FSCTL_QUERY_USN_JOURNAL` 查询 USN 日志信息。
    pub fn query(volume: &Volume) -> Result<Self, windows_result::Error> {
        let handle = HANDLE(volume.as_raw_handle());
        let mut out_buf = MaybeUninit::<USN_JOURNAL_DATA_V0>::uninit();
        let mut bytes_returned = MaybeUninit::uninit();
        let inner = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some(out_buf.as_mut_ptr().cast()),
                size_of_val(&out_buf) as _,
                Some(bytes_returned.as_mut_ptr()),
                None,
            )?;
            debug_assert!(
                bytes_returned.assume_init() as usize >= size_of_val(&out_buf),
                "FSCTL_QUERY_USN_JOURNAL 返回字节数 {} 填不满 USN_JOURNAL_DATA_V0 {}",
                bytes_returned.assume_init(),
                size_of_val(&out_buf),
            );
            out_buf.assume_init()
        };

        Ok(Self { inner })
    }

    /// 返回从日志末尾读取的游标。
    pub fn next_cursor(&self) -> JournalCursor {
        JournalCursor::new(self.inner.UsnJournalID, self.inner.NextUsn)
    }

    /// 日志标识符。
    pub fn usn_journal_id(&self) -> UsnJournalId {
        self.inner.UsnJournalID
    }

    /// 可从日志中读取的第一条记录的 USN。
    pub fn first_usn(&self) -> Usn {
        self.inner.FirstUsn
    }

    /// 要写入日志的下一条记录的 USN。
    pub fn next_usn(&self) -> Usn {
        self.inner.NextUsn
    }

    /// 日志自创建起写入的第一条记录的 USN。
    ///
    /// 请不要和 [`Self::first_usn`] 混淆，由于滚动覆盖机制，此值可能小于第一条可读取的 USN。
    /// [`EnumUsnData`](crate::enumerate::EnumUsnData) 也可能返回低于此值的 USN，这说明日志或许经过重建（标识符不同）。
    /// 无论如何这都表明了日志存在中断，期间的 USN 记录已不可追溯。
    pub fn lowest_valid_usn(&self) -> Usn {
        self.inner.LowestValidUsn
    }

    /// 日志支持的最大 USN。
    ///
    /// 当 [`Self::next_usn`] 的值接近此值时，管理员必须删除 USN 日志。
    pub fn max_usn(&self) -> Usn {
        self.inner.MaxUsn
    }

    /// 日志的目标最大大小（以字节为单位）。
    ///
    /// 日志可能会增大到超过此值，但随后在下一个 NTFS 文件系统检查点处将其截断为小于此值。
    pub fn maximum_size(&self) -> u64 {
        self.inner.MaximumSize
    }

    /// 每次分配或解除分配内存时，添加到末尾并从更改日志开头删除的磁盘内存字节数。
    ///
    /// 换句话说，分配和解除分配以此大小的单位进行。群集大小的整数倍数是此成员的合理值。
    pub fn allocation_delta(&self) -> u64 {
        self.inner.AllocationDelta
    }
}

/// 基于 `FSCTL_READ_USN_JOURNAL` 分批读取 USN 日志的迭代器。
pub struct ReadUsnJournal<'a> {
    handle: HANDLE,
    in_buf: READ_USN_JOURNAL_DATA_V0,
    out_buf: OutputBuffer,
    _marker: PhantomData<&'a Volume>,
}

impl<'a> ReadUsnJournal<'a> {
    /// 缓冲区大小至少为 `bytes` 个字节，且会向上取整至 8 的倍数。
    pub fn with_capacity(volume: &'a Volume, capacity: usize, cursor: JournalCursor) -> Self {
        // https://learn.microsoft.com/zh-cn/windows/win32/api/winioctl/ns-winioctl-read_usn_journal_data_v0
        let in_buf = READ_USN_JOURNAL_DATA_V0 {
            UsnJournalID: cursor.journal_id,
            StartUsn: cursor.next_usn,
            ReasonMask: u32::MAX,
            ReturnOnlyOnClose: 0,
            // [TODO]: 内核级等待的心智太过于复杂，暂时难以设计人体工学友好的 API，默认采用非阻塞模式
            BytesToWaitFor: 0,
            Timeout: 0,
        };
        let handle = HANDLE(volume.as_raw_handle());
        let out_buf = OutputBuffer::with_capacity(capacity);

        Self {
            handle,
            in_buf,
            out_buf,
            _marker: PhantomData,
        }
    }

    /// 游标于 [`Self::next_records`] 中推进，该函数返回当前游标。
    pub fn cursor(&self) -> JournalCursor {
        JournalCursor::new(self.in_buf.UsnJournalID, self.in_buf.StartUsn)
    }

    /// 过滤 USN 记录的掩码，记录命中任意一个标志都会被包含在结果中，零掩码不会有任何命中。
    pub fn reason_mask(&self) -> UsnReasons {
        UsnReasons::from_bits_retain(self.in_buf.ReasonMask)
    }

    /// 设置 [`Self::reason_mask`]，默认不对记录做过滤。
    pub fn set_reason_mask(&mut self, reasons: UsnReasons) -> &mut Self {
        self.in_buf.ReasonMask = reasons.bits();
        self
    }

    /// USN 记录结果返回的时机。
    ///
    /// 开启后，只有在最终句柄关闭时才返回记录；
    /// 返回记录的 Reason 通常包含 [`UsnReasons::CLOSE`]，并携带此前累积的其他原因标志。
    /// 请不要和 [`Self::reason_mask`] 混淆，此项可以与掩码搭配使用。
    pub fn only_on_close(&self) -> bool {
        self.in_buf.ReturnOnlyOnClose != 0
    }

    /// 设置 [`Self::only_on_close`]，默认为 `false`。
    pub fn set_only_on_close(&mut self, only_on_close: bool) -> &mut Self {
        self.in_buf.ReturnOnlyOnClose = only_on_close.into();
        self
    }
}

impl ReadUsnJournal<'_> {
    /// Lending Iterator 实现，配合 `while let` 使用。
    ///
    /// 此函数尽力读到当前 USN 日志末尾，经过一段时间再调用可能会有新的数据（非幂等）。
    /// 过程中可能出现没有 USN 记录的缓冲区，不一定代表读到末尾，请以返回 [`None`] 为准。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::error::Error;
    /// # use fastfind_ntfs::volume::Volume;
    /// # fn main() -> Result<(), Box<dyn Error>> {
    /// let volume = Volume::open(r"\\.\C:")?;
    /// let journal = volume.query_usn_journal()?;
    /// // 重要：必须存为变量，避免 `while let` 每轮重复创建迭代器导致进度重置
    /// let mut iter = volume.read_usn_journal(journal.next_cursor());
    ///
    /// while let Some(mut records) = iter.next_records()? {
    ///     while let Some(record) = records.next_record() {
    ///         println!("{}", record.file_name().to_string_lossy());
    ///     }
    /// }
    /// ```
    pub fn next_records(&mut self) -> Result<Option<UsnRecords<'_>>, windows_result::Error> {
        let mut bytes_returned = MaybeUninit::uninit();
        unsafe {
            DeviceIoControl(
                self.handle,
                FSCTL_READ_USN_JOURNAL,
                Some(ptr::from_ref(&self.in_buf).cast()),
                size_of_val(&self.in_buf) as _,
                Some(self.out_buf.as_mut_c_void()),
                self.out_buf.capacity() as _,
                Some(bytes_returned.as_mut_ptr()),
                None,
            )?;

            self.out_buf.set_len(bytes_returned.assume_init() as _);
        }

        let (next_usn, records) = self
            .out_buf
            .split_first_u64()
            .expect("成功的 `FSCTL_READ_USN_JOURNAL` 应当填充非空数据");
        let next_usn = next_usn as Usn;
        if self.in_buf.StartUsn == next_usn {
            debug_assert!(
                records.is_empty(),
                "USN ({}) 不推进但有 {} 字节的记录被填充",
                next_usn,
                records.len(),
            );
            return Ok(None);
        }
        self.in_buf.StartUsn = next_usn;

        Ok(Some(unsafe { UsnRecords::from_bytes(records) }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JournalCursor {
    pub journal_id: UsnJournalId,
    /// 要读取的下一条 USN 记录，0 代表日志中的第一条。
    pub next_usn: Usn,
}

impl JournalCursor {
    pub fn new(journal_id: UsnJournalId, next_usn: Usn) -> Self {
        Self {
            journal_id,
            next_usn,
        }
    }
}
