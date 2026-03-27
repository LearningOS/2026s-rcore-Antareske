# lab4

---

# bug 记录

## easy-fs/src/vfs.rs :`Inode::linkat`

遇到了自锁问题，上锁的嵌套顺序有问题。

```rust
impl Inode {
    ...

    /// [INFO] CH6
    /// <!> 只被根目录 Inode 调用
    /// 在根目录插入一个名字为 new_name，inode_id 和 old_name 文件相同 inode_id 的目录项
    /// 没有找到 old_name 文件会返回 -1
    pub fn linkat(&self, old_name: &str, new_name: &str) -> isize {
			  // let mut fs = self.fs.lock();  // 不能在闭包外给 fs 上锁
        if let Some(the_inode_id) = self.read_disk_inode(
            |disk_inode| { self.find_inode_id(old_name, disk_inode) }
        ) {
            self.modify_disk_inode(|root_inode| {
                // 在闭包内部上锁，否则 Mutex 重入死锁
                let mut fs = self.fs.lock();
                // append file in the dirent
                let file_count = (root_inode.size as usize) / DIRENT_SZ;
                let new_size = (file_count + 1) * DIRENT_SZ;
                // increase size
                self.increase_size(new_size as u32, root_inode, &mut fs);
                // write dirent
                let dirent = DirEntry::new(new_name, the_inode_id);
                root_inode.write_at(
                    file_count * DIRENT_SZ,
                    dirent.as_bytes(),
                    &self.block_device,
                );
            });
            let the_inode = self.find(old_name).unwrap();
            the_inode.modify_disk_inode(|the_disk_inode| {
                the_disk_inode.nlink += 1;
            });
            block_cache_sync_all();
            return 0;
        }
        -1  // 没有找到 old_name 文件
    }
}
```

---

<aside>
🤥

由 amalgo 生成代码总览，由 Grok 处理总结。

</aside>

本报告整理了项目中所有标注有"[INFO] CH6"的代码实现部分，包括函数、方法和相关结构体修改。所有内容严格基于提供的源代码，未添加任何额外代码或解释。每个部分后附带该函数/方法的用途或功能说明（基于源代码中的注释或上下文描述）。

# os/src 部分

## 1. 来自 src/fs/inode.rs 的函数：linkat

```rust
/// [INFO] CH6
/// 在根目录插入一个名字为 new_name，inode_id 和 old_name 文件相同 inode_id 的目录项
/// 新名字过长会截断
/// 没有找到 old_name 文件会返回 -1
pub fn linkat(old_name: &str, new_name: &str) -> isize {
    ROOT_INODE.linkat(old_name, new_name)
}

```

该函数用于在根目录中插入一个新目录项，其 inode_id 与旧文件相同；如果旧文件不存在，返回 -1。

## 2. 来自 src/fs/inode.rs 的函数：unlinkat

```rust
/// [INFO] CH6
/// 删除根目录下名字为 name 的目录项
/// 没有找到 name 文件会返回 -1
/// <!> 功能不完全
pub fn unlinkat(name: &str) -> isize {
    ROOT_INODE.unlinkat(name)
}

```

该函数用于删除根目录下指定名字的目录项；如果文件不存在，返回 -1；功能不完全。

## 3. 来自 src/fs/inode.rs 的方法：get_stat（在 impl File for OSInode 内）

```rust
impl File for OSInode {
    ...

    /// [INFO] CH6
    /// 获取文件状态信息
    fn get_stat(&self) -> Stat {
        let inner = self.inner.exclusive_access();
        let (inode_id, nlink, is_file) = inner.inode.get_stat();

        let mode = if is_file {
            StatMode::FILE
        } else {
            StatMode::DIR
        };
        Stat {
            dev: 0,
            ino: inode_id,
            mode,
            nlink,
            pad: [0; 7],
        }
    }
}

```

该方法用于获取文件的状态信息，包括设备 ID、inode 号、模式和硬链接数。

## 4. 来自 src/fs/stdio.rs 的方法：get_stat（在 impl File for Stdin 内）

```rust
impl File for Stdin {
    ...

    /// [INFO] CH6
    fn get_stat(&self) -> Stat {
        Stat {
            dev: 0,
            ino: 0,
            mode: StatMode::NULL,
            nlink: 1,
            pad: [0; 7],
        }
    }
}

```

该方法用于获取标准输入文件的状态信息，返回一个空模式的状态结构体。

## 5. 来自 src/fs/stdio.rs 的方法：get_stat（在 impl File for Stdout 内）

```rust
impl File for Stdout {
    ...

    /// [INFO] CH6
    fn get_stat(&self) -> Stat {
        Stat {
            dev: 0,
            ino: 0,
            mode: StatMode::NULL,
            nlink: 1,
            pad: [0; 7],
        }
    }
}

```

该方法用于获取标准输出文件的状态信息，返回一个空模式的状态结构体。

## 6. 来自 src/syscall/fs.rs 的函数：sys_linkat

```rust
/// [INFO] CH6
/// 在根目录插入一个名字为 new_name，inode_id 和 old_name 文件相同 inode_id 的目录项
/// 新名字过长会截断
/// 没有找到 old_name 文件会返回 -1
/// ! 不考虑新文件路径已经存在的情况
/// YOUR JOB: Implement linkat.
pub fn sys_linkat(old_name: *const u8, new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let old_name = translated_str(token, old_name);
    let new_name = translated_str(token, new_name);
    if old_name == new_name {
        return -1
    }
    linkat(old_name.as_str(), new_name.as_str())
}

```

该函数用于实现 linkat 系统调用，在根目录中插入一个新目录项，其 inode_id 与旧文件相同；如果旧文件不存在或新旧名称相同，返回 -1；不考虑新路径已存在的情况。

## 7. 来自 src/syscall/fs.rs 的函数：sys_unlinkat

```rust
/// [INFO] CH6
/// 取消一个文件路径到文件的链接
/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat",
        current_task().unwrap().pid.0
    );
    let name = translated_str(current_user_token(), name);
    unlinkat(name.as_str())
}

```

该函数用于实现 unlinkat 系统调用，取消指定文件路径到文件的链接。

## 8. 来自 src/syscall/process.rs 的函数：sys_spawn

```rust
/// [INFO] CH6
/// 改为从文件中加载应用
/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let data = app_inode.read_all();
        let parent = current_task().unwrap();
        // fork 一个子进程
        let new_task = parent.fork();
        // 用 exec 替换子进程的内存映像 + 执行
        new_task.exec(data.as_slice());
        let new_pid = new_task.pid.0;
        // 把子进程加入调度器
        add_task(new_task);
        new_pid as isize
    } else {
        -1
    }
}

```

该函数用于实现 spawn 系统调用，从文件中加载应用创建新进程；提示 fork + exec 不等于 spawn；如果文件打开失败，返回 -1。

# easy-fs/src 部分

## 1. 来自 src/efs.rs 的方法：cal_inode_id（在 impl EasyFileSystem 内）

```rust
/// [INFO] CH6
/// efs 辅助方法
impl EasyFileSystem {
    /// 计算 inode_id
    /// 全 let
    pub fn cal_inode_id(&self, block_id: usize, block_offset: usize) -> usize {
        let inode_size = core::mem::size_of::<DiskInode>();
        let inodes_per_block = BLOCK_SZ / inode_size;
        // 都用 usize 计算
        let inode_id = (block_id - self.inode_area_start_block as usize) * inodes_per_block
            + (block_offset / inode_size);
        inode_id
    }
}

```

**用途/功能**：该方法是 EasyFileSystem 的辅助方法，用于根据块 ID 和块内偏移计算 inode 的索引号（inode_id）。计算方式为：用块 ID 减去 inode 区域起始块号，乘以每个块的 inode 数，再加上偏移量除以 inode 大小的结果。

## 2. 来自 src/layout.rs 的结构体：DiskInode（新增属性 nlink）

```rust
/// A disk inode
#[repr(C)]
pub struct DiskInode {
    pub size: u32,
    pub direct: [u32; INODE_DIRECT_COUNT],
    pub indirect1: u32,
    pub indirect2: u32,
    pub nlink: u32,     // [INFO] CH6 硬链接计数
    type_: DiskInodeType,
}

```

**用途/功能**：在 DiskInode 结构体中新增了 `nlink: u32` 字段，用于记录文件的硬链接计数，表示该 inode 被多少个目录项引用。

## 3. 来自 src/layout.rs 的常量：INODE_DIRECT_COUNT

```rust
/// The max number of direct inodes
/// [INFO] CH6 加上新的字段 nlink: u32 后，将直接索引数减 1
const INODE_DIRECT_COUNT: usize = 27;

```

**用途/功能**：该常量定义了 DiskInode 结构体的直接索引数量。由于新增了 `nlink: u32` 字段，占用了部分空间，因此将直接索引数从默认值减少 1，设置为 27。

## 4. 来自 src/vfs.rs 的方法：linkat（在 impl Inode 内）

```rust
impl Inode {
    ...

    /// [INFO] CH6
    /// <!> 只被根目录 Inode 调用
    /// 在根目录插入一个名字为 new_name，inode_id 和 old_name 文件相同 inode_id 的目录项
    /// 没有找到 old_name 文件会返回 -1
    pub fn linkat(&self, old_name: &str, new_name: &str) -> isize {
        if let Some(the_inode_id) = self.read_disk_inode(
            |disk_inode| { self.find_inode_id(old_name, disk_inode) }
        ) {
            self.modify_disk_inode(|root_inode| {
                // 在闭包内部上锁，否则 Mutex 重入死锁
                let mut fs = self.fs.lock();
                // append file in the dirent
                let file_count = (root_inode.size as usize) / DIRENT_SZ;
                let new_size = (file_count + 1) * DIRENT_SZ;
                // increase size
                self.increase_size(new_size as u32, root_inode, &mut fs);
                // write dirent
                let dirent = DirEntry::new(new_name, the_inode_id);
                root_inode.write_at(
                    file_count * DIRENT_SZ,
                    dirent.as_bytes(),
                    &self.block_device,
                );
            });
            let the_inode = self.find(old_name).unwrap();
            the_inode.modify_disk_inode(|the_disk_inode| {
                the_disk_inode.nlink += 1;
            });
            block_cache_sync_all();
            return 0;
        }
        -1  // 没有找到 old_name 文件
    }
}

```

**用途/功能**：该方法用于在根目录中插入一个新目录项，名称为 `new_name`，其 inode_id 与 `old_name` 文件的 inode_id 相同。如果 `old_name` 文件不存在，返回 -1。方法会增加目标文件的硬链接计数（nlink），并将新目录项追加到根目录的数据中。

## 5. 来自 src/vfs.rs 的方法：unlinkat（在 impl Inode 内）

```rust
impl Inode {
    ...

    /// [INFO] CH6
    /// 将目录项置空
    /// <!> 只被根目录 Inode 调用
    /// <!> 功能不完全：
    ///     Inode 未实现删除文件，其中根目录新加目录项时是在数据队尾 append，而非找到空位置插入。
    ///     这里删除目录项仅将指定目录项置为 empty()，该空洞不会被分配给新的目录项
    pub fn unlinkat(&self, name: &str) -> isize {
        let the_inode = self.find(name).unwrap();

        let r = self.modify_disk_inode(|root_inode| {
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            // 查根目录所有目录项
            for i in 0..file_count {
                let mut tmp_dirent = DirEntry::empty();
                // 从 inode 里读出第 i 个目录项
                root_inode.read_at(
                    i * DIRENT_SZ,
                    tmp_dirent.as_bytes_mut(),  // 读入到 tmp_dirent
                    &self.block_device,
                );
                if tmp_dirent.name() == name {
                    root_inode.write_at(
                        i * DIRENT_SZ,
                        DirEntry::empty().as_bytes(),
                        &self.block_device,
                    );
                    return 0;
                }
            }
            -1
        });

        // 若 nlink 归零，则删除+回收文件
        let nlink = the_inode.modify_disk_inode(|disk_inode| {
            disk_inode.nlink -= 1;
            disk_inode.nlink
        });
        if nlink == 0 {
            the_inode.clear();
        }
        block_cache_sync_all();
        r
    }
}

```

**用途/功能**：该方法用于删除根目录中指定名称的目录项，仅将目录项置为空（empty()），不重新分配该空洞（功能不完全）。如果找到并删除目录项，返回 0；否则返回 -1。删除后会减少目标文件的硬链接计数（nlink），若计数归零，则清空并回收文件。

## 6. 来自 src/vfs.rs 的方法：get_stat（在 impl Inode 内）

```rust
impl Inode {
    ...

    /// [INFO] CH6
    /// 获取文件状态信息：inode_id, nlink, is_file
    pub fn get_stat(&self)
        -> (
            u64, // inode_id
            u32, // nlink
            bool // is_file
        )
    {
        // 调 efs 辅助函数算 inode 编号
        let fs = self.fs.lock();
        let inode_id = fs.cal_inode_id(self.block_id, self.block_offset);
        self.read_disk_inode(|disk_inode| (inode_id as u64, disk_inode.nlink, disk_inode.is_file()))
    }
}

```

**用途/功能**：该方法用于获取文件的状态信息，返回一个三元组，包含 inode 编号（inode_id）、硬链接计数（nlink）和是否为普通文件（is_file）的标志。调用 EasyFileSystem 的 `cal_inode_id` 方法计算 inode_id。