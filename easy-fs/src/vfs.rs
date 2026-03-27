use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Call a function over a disk inode (自己的 DiskInode) to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode (自己的 DiskInode) to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    /// 在指定目录型 DiskInode 下查指定文件名的目录项的 DiskInode 编号
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    /// <!> 只被根目录 Inode 调用
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Create inode under current inode by name
    /// <!> 只被根目录 Inode 调用
    /// 为新文件分配新 DiskInode 并初始化；将新文件的目录项插入进根目录 (自己的 DiskInode) 的数据中
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    /// <!> 只被根目录 Inode 调用
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    /// DiskInode::read_at 的封装
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    /// DiskInode::write_at 的封装 + 自动扩容
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    /// 回收文件的索引块和数据块
    /// e.g. 当带有 CREATE 标志打开已存在文件时，需先清空文件
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }

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
