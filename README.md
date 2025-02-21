# FastFind

🔍 超级快的 Windows 全盘文件搜索工具，基于 USN 日志。

## 特点

- 索引完成后，搜索速度**超级快**
- 借助 USN 日志和多线程，索引速度也**很快**
- 不需要**扫盘**，几乎不占用**CPU**
- 输出结果中关键词会**高亮**，有助于区分
- 关键词**不区分大小写**
- 索引可以与文件系统保持**同步**

## 使用

右键程序，以**管理员身份运行**（否则会闪退），会出现以下 prompt：

```sh
[ffd]> 
```

此时输入文件名，回车进行搜索。

若想通过命令行启动，请确保命令行拥有**管理员权限**。

> [!TIP]
>
> 首次索引时间可能较慢，默认情况下会索引所有支持（NTFS文件系统）的盘。

## 编译

从 [Github Releases](https://github.com/DreamAlone666/fastfind/releases) 下载预编译二进制，或者使用 Rust 工具链从源码编译。