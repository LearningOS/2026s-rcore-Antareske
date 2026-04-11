# lab3

<aside>
💾

本实验迁移了前一章（lab2）的 sys_get_time, mmap, munmap 的相关实现，在这里也整理出来。

</aside>

<aside>
🤥

由 amalgo 生成代码总览，由 Grok 处理总结。

</aside>

本报告整理了项目中所有标注有"[INFO] CH5"的实现部分。这些实现主要涉及stride优先级调度算法、内存映射（mmap/munmap）、时间系统调用、进程生成（spawn）和优先级设置等功能。报告按文件路径分类，每项包括代码抄写（Rust格式）、来源路径以及用途/功能说明。

## src/config.rs

### 常量：BIG_STRIDE 和 DEFAULT_PRIORITY

```rust
/// [INFO] CH5 : stride 优先级算法
/// 进程的 BigStride 常数 : 一个很大的常数
pub const BIG_STRIDE: usize = usize::MAX;
/// [INFO] CH5 : stride 优先级算法
/// 进程的初始优先级
pub const DEFAULT_PRIORITY: usize = 16;

```

- **来源路径**：src/config.rs
- **用途/功能**：`BIG_STRIDE` 定义了stride算法中的大步长常量，用于计算进程的步进值（pass = BIG_STRIDE / priority），防止优先级过小时步长过大导致溢出。`DEFAULT_PRIORITY` 定义进程的初始优先级，用于初始化任务的优先级和步长。

## src/mm/frame_allocator.rs

### 方法：count_unalloc_frame (在 impl StackFrameAllocator 内)

```rust
impl StackFrameAllocator {
    /// [INFO] CH5
    /// 计算未分配的页帧数目
    pub fn count_unalloc_frame(&self) -> usize {
        return self.end - self.current + 1 + self.recycled.len();
    }
    ...
}

```

- **来源路径**：src/mm/frame_allocator.rs
- **用途/功能**：计算当前帧分配器中剩余的未分配物理页帧数量，包括连续未分配区域和回收列表中的帧，用于检查是否足够分配新页（如mmap时检查）。

### 函数：count_unalloc_frame

```rust
/// [INFO] CH5
/// 计算未分配页帧的数目
pub fn count_unalloc_frame() -> usize {
    FRAME_ALLOCATOR.exclusive_access().count_unalloc_frame()
}

```

- **来源路径**：src/mm/frame_allocator.rs
- **用途/功能**：全局函数，用于获取全局帧分配器中剩余未分配页帧的数量，方便在内存操作（如mmap）中检查可用帧。

## src/mm/memory_set.rs

### 方法：get_mapped_pages (在 impl MemorySet 内)

```rust
impl MemorySet {
    ...
    /// [INFO] CH5
    /// 返回区间内已映射的 vpn 向量，左闭右闭；形参是 vpn 的版本
    /// 可以用于获知空间的区间内已映射的 vpn 的数目
    pub fn get_mapped_pages(&self, start_vpn: VirtPageNum, end_vpn: VirtPageNum) -> Vec<VirtPageNum> {
        let mut valid_pages: Vec<VirtPageNum> = Vec::new();
        let mut end_vpn = end_vpn;
        end_vpn.step();     // VPNRange 左闭右开
        let vpn_range = VPNRange::new(start_vpn, end_vpn);
        for vpn in vpn_range {
            if let Some(pte) = self.page_table.translate(vpn) {
                if pte.is_valid() {
                    valid_pages.push(vpn);
                }
            }
        }
        valid_pages
    }
    ...
}

```

- **来源路径**：src/mm/memory_set.rs
- **用途/功能**：获取指定虚拟页号（VPN）范围内已映射的页，返回一个向量。用于检查区域是否已映射（如mmap前检查重叠）或计算映射页数。

### 方法：mmap (在 impl MemorySet 内)

```rust
impl MemorySet {
    ...
    /// [INFO] CH5
    /// 申请映射一段虚拟地址
    pub fn mmap(&mut self, start: VirtAddr, len: usize, port: usize) -> isize {
        let end: VirtAddr = (usize::from(start) + len - 1).into();
        let start_vpn = start.floor();
        let end_vpn = end.floor();
        let num_pages = end_vpn.0 - start_vpn.0 + 1;

        if start.0 % PAGE_SIZE == 0 && port & !0x7 == 0 && port & 0x7 != 0
            // && self.count_mapped_page(start_vpn, end_vpn) == 0
            && self.get_mapped_pages(start_vpn, end_vpn).len() == 0
            && num_pages <= count_unalloc_frame()
        {
            let mut permission = MapPermission::U;
            if port & 0x1 == 0x1 { permission |= MapPermission::R; }
            if port & 0x2 == 0x2 { permission |= MapPermission::W; }
            if port & 0x4 == 0x4 { permission |= MapPermission::X; }

            // [歧义]
            // insert_framed_area 调用 MapArea::new(.., end_va, ..) 以 end_va.ceil() 为上开界，
            // 若 end_va 对齐 PAGE_SIZE，end_va.ceil() 等于本身所在页号，则 insert_framed_area 时
            // 少映射了一页。
            // [说明]
            // 当 va 对齐，ceil() 在整除 PAGE_SIZE 时截断 (PAGE_SIZE - 1) 部分，仍等于 va 所在页号，
            // 即 va.ceil() == va.floor()。可能语义上认为对齐的地址既是前一页的 ceil 又是后一页的 floor。
            // [解决]
            // 若地址 end 对齐，令 end ++
            let upper_bound = if end.aligned() { end.0 + 1 } else { end.0 };
            self.insert_framed_area(start, upper_bound.into(), permission);
            return 0;
        }
        -1
    }
    ...
}

```

- **来源路径**：src/mm/memory_set.rs
- **用途/功能**：在地址空间中映射一段虚拟地址区域（mmap系统调用实现），检查对齐、权限、未映射和可用帧后，插入新的帧映射区域，返回0表示成功，否则-1。

### 方法：which_framed_area (在 impl MemorySet 内)

```rust
impl MemorySet {
    ...
    /// [INFO] CH5
    /// 指定的 VPN 在哪个 MapArea 内
    pub fn which_framed_area(&mut self, vpn: VirtPageNum) -> Option<&mut MapArea> {
        for area in self.areas.iter_mut() {
            if area.map_type == MapType::Framed {
                if area.vpn_range.get_start() <= vpn && vpn < area.vpn_range.get_end() {
                    return Some(area);
                }
            }
        }
        None
    }
    ...
}

```

- **来源路径**：src/mm/memory_set.rs
- **用途/功能**：查找指定虚拟页号（VPN）所属的帧映射区域（MapArea），用于内存操作如munmap时定位区域。

## src/syscall/process.rs

### 函数：sys_get_time

```rust
/// [INFO] CH5
/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time",
        current_task().unwrap().pid.0
    );
    let us = get_time_us();

    let tv = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };

    // tv 的 u8 切片
    let tv_bytes = unsafe {
        core::slice::from_raw_parts(
            &tv as *const TimeVal as *const u8,
            core::mem::size_of::<TimeVal>(),
        )
    };

    // ts 的有序 u8 切片
    let mut bufs = translated_byte_buffer(
        current_user_token(),
        _ts as *const u8,
        tv_bytes.len()  // TimeVal 是纯值类型长度一致 (ts 和 tv)
    );

    // 按段拷贝
    let mut copied = 0;
    for buf in bufs.iter_mut() {
        let len = buf.len().min(tv_bytes.len() - copied);
        buf[..len].copy_from_slice(&tv_bytes[copied..copied+len]);
        copied += len;
    }
    0
}

```

- **来源路径**：src/syscall/process.rs
- **用途/功能**：实现get_time系统调用，获取当前时间（秒和微秒），处理可能跨页的TimeVal结构，通过字节缓冲区拷贝数据到用户空间。

### 函数：sys_mmap

```rust
/// [INFO] CH5
/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap",
        current_task().unwrap().pid.0
    );
    mmap(_start.into(), _len, _port)
}

```

- **来源路径**：src/syscall/process.rs
- **用途/功能**：mmap系统调用入口，调用processor::mmap在当前任务的地址空间中映射虚拟地址区域。

### 函数：sys_munmap

```rust
/// [INFO] CH5
/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap",
        current_task().unwrap().pid.0
    );
    munmap(_start.into(), _len)
}

```

- **来源路径**：src/syscall/process.rs
- **用途/功能**：munmap系统调用入口，调用processor::munmap解除当前任务地址空间中的虚拟地址映射。

### 函数：sys_spawn

```rust
/// [INFO] CH5
/// 注：本章测例需要先完成 sys_spawn 才能检测其它作业
/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let parent = current_task().unwrap();
        // fork 一个子进程
        let new_task = parent.fork();
        // 用 exec 替换子进程的内存映像 + 执行
        new_task.exec(data);
        let new_pid = new_task.pid.0;
        // 把子进程加入调度器
        add_task(new_task);
        new_pid as isize
    } else {
        -1
    }
}

```

- **来源路径**：src/syscall/process.rs
- **用途/功能**：spawn系统调用，实现进程生成：fork创建子进程、exec加载新程序、加入调度器，返回新进程PID或-1（失败）。

### 函数：sys_set_priority

```rust
/// [INFO] CH5
// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority",
        current_task().unwrap().pid.0
    );
    if _prio <= 2 {
        return -1;
    }
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .update_priority(_prio as usize);
    _prio
}

```

- **来源路径**：src/syscall/process.rs
- **用途/功能**：set_priority系统调用，设置当前任务的优先级（至少>2），更新步长，返回新优先级或-1（无效优先级）。

## src/task/manager.rs

### 方法：add (在 impl TaskManager 内)

```rust
impl TaskManager {
    ...
    /// [INFO] CH5
    /// 将任务交给调度器 (TaskManager)
    /// CH5 场景：sys_fork, suspend_current_and_run_next, INITPROC 初始化
    /// 考虑到还有强制中断等其它场景会将让出的任务加入调度，
    /// 因此选择在这里触发任务 stride 的更新
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        task.inner_exclusive_access().update_stride();
        self.ready_queue.push_back(task);
    }
    ...
}

```

- **来源路径**：src/task/manager.rs
- **用途/功能**：将任务加入就绪队列前更新其stride值，确保stride算法的进度更新，用于进程调度。

### 方法：fetch (在 impl TaskManager 内)

```rust
impl TaskManager {
    ...
    /// [INFO] CH5
    /// 应当取出 stride 最小的任务交付给 processor 去 run
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        // self.ready_queue.pop_front()     // 不再按序交付任务
        if self.ready_queue.is_empty() {
            return None;
        }
        // stride 进度最小的任务的索引
        let min_stride_index = self
            .ready_queue
            .iter()
            // enumerate(): 每个元素变成 (idx, task) 形式
            .enumerate()
            // min_by_key(): 返回 key 最小的 (idx, task) 元素的 Option
            // 模式匹配用一个 & 消一层引用
            .min_by_key(|&(_, task)| task.inner_exclusive_access().stride)
            // Option<(idx, task)> => Option<idx>
            // ? 拆 Option，若为 None 直接 return None
            .map(|(idx, _)| idx)?;
        self.ready_queue.remove(min_stride_index)
    }
    ...
}

```

- **来源路径**：src/task/manager.rs
- **用途/功能**：从就绪队列中取出stride值最小的任务（优先级最高），实现stride调度算法的核心选择逻辑。

## src/task/processor.rs

### 函数：mmap

```rust
/// [INFO] CH5
pub fn mmap(start: VirtAddr, len: usize, port: usize) -> isize {
    // * 临时值的生命周期会在句末结束，用新变量 binding 延长 unwrap() 返回值的生命
    // let binding = current_task().unwrap();
    // * 虽然 inner_exclusive_access 返回可变引用，但仍需手动声明变量为可变
    // let mut inner = binding.inner_exclusive_access();
    // inner.memory_set.mmap(start, len, port)
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .memory_set
        .mmap(start, len, port)
}

```

- **来源路径**：src/task/processor.rs
- **用途/功能**：全局mmap函数，调用当前任务的MemorySet::mmap实现虚拟地址映射。

### 函数：munmap

```rust
/// [INFO] CH5
pub fn munmap(start: VirtAddr, len: usize) -> isize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .memory_set
        .munmap(start, len)
}

```

- **来源路径**：src/task/processor.rs
- **用途/功能**：全局munmap函数，调用当前任务的MemorySet::munmap解除虚拟地址映射。

## src/task/task.rs

### 结构体：TaskControlBlockInner (新增属性)

```rust
pub struct TaskControlBlockInner {
    ...
    /// [INFO] CH5
    /// 代表进程的执行进度，初始为 0，始终 >= 2
    pub stride: usize,
    /// [INFO] CH5
    /// 进程的优先级，默认初始值为 16
    pub priority: usize,
    /// [INFO] CH5
    /// 每次执行累加在 stride 上的步长值，为 BIG_STRIDE / priority
    pub pass: usize,
}

```

- **来源路径**：src/task/task.rs
- **用途/功能**：在TaskControlBlockInner结构体中新增stride、priority和pass字段，用于实现stride优先级调度算法，支持进程进度跟踪和优先级管理。

### 方法：update_stride (在 impl TaskControlBlockInner 内)

```rust
impl TaskControlBlockInner {
    ...
    /// [INFO] CH5
    /// 更新 stride
    /// * 用于调度器
    pub fn update_stride(&mut self) {
        // 防止溢出
        self.stride = self.stride.saturating_add(self.pass);
    }
    ...
}

```

- **来源路径**：src/task/task.rs
- **用途/功能**：更新进程的stride值，通过添加pass（步长）实现进度累加，防止usize溢出，用于调度器选择任务。

### 方法：update_priority (在 impl TaskControlBlockInner 内)

```rust
impl TaskControlBlockInner {
    ...
    /// [INFO] CH5
    /// 更新 priority
    /// * 用于 syscall
    pub fn update_priority(&mut self, priority: usize) {
        self.priority = priority;
        // 防止溢出
        self.pass = BIG_STRIDE.saturating_div(priority);
    }
    ...
}

```

- **来源路径**：src/task/task.rs
- **用途/功能**：更新进程优先级，并重新计算步长（pass = BIG_STRIDE / priority），防止除法溢出，用于set_priority系统调用。