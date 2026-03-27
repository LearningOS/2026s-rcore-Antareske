# lab1

# 环境

系统：WSL2 的 Ubuntu 20.04.6 发行版

Qemu版本：7.0.0

RustSBI ：0.3.0-alpha.2

# 目标

添加 ID 为 410 的系统调用的实现：

`fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize`

- 调用规范：
    - 这个系统调用有三种功能，根据 `trace_request` 的值不同，执行不同的操作：
    - 如果 `trace_request` 为 0，则 `id` 应被视作 `const u8` ，表示读取当前任务 `id` 地址处一个字节的无符号整数值。此时应忽略 `data` 参数。返回值为 `id` 地址处的值。
    - 如果 `trace_request` 为 1，则 `id` 应被视作 `mut u8` ，表示写入 `data` （作为 `u8`，即只考虑最低位的一个字节）到该用户程序 `id` 地址处。返回值应为0。
    - 如果 `trace_request` 为 2，表示查询当前任务调用编号为 `id` 的系统调用的次数，返回值为这个调用次数。本次调用也计入统计 。
    - 否则，忽略其他参数，返回值为 -1。
- 说明：
    - 你可能会注意到，这个调用的读写并不安全，使用不当可能导致崩溃。这是因为在下一章节实现地址空间之前，系统中缺乏隔离机制。所以我们 不要求你实现安全检查机制，只需通过测试用例即可 。
    - 你还可能注意到，这个系统调用读写本任务内存的功能并不是很有用。这是因为作业的灵感来源 syscall 主要依靠 trace 功能追踪其他任务的信息，但在本章节我们还没有进程、线程等概念，所以简化了操作，只要求追踪自身的信息。

# 实现

在 TaskManagerInner 中增添一个类型为`BTreeMap`的成员变量 `syscall_recorder` ，以记录所有任务对各个系统调用的调用次数：

```rust
// os/src/task/mod.rs

/// Inner of Task Manager
pub struct TaskManagerInner {
    /// task list
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// id of current `Running` task
    current_task: usize,
    /// ch3: count syscall by id for each task
    syscall_recorder: BTreeMap<usize, BTreeMap<usize, usize>>,
}
```

`syscall_recorder` 的外层 `BTreeMap` 的键是任务 ID（usize），内层 `BTreeMap` 的键是系统调用 ID（usize），值是调用次数（usize）。在 TASK_MANAGER 初始化中增添 `syscall_recorder` 的初始化。

在 `TaskManager` 的 impl 块中，添加两个方法：一个用于查询当前任务调用指定 ID 系统调用的次数，另一个用于为当前任务记录一次指定 ID 的系统调用。随后，在外部定义两个函数，分别调用这两个方法以提供全局访问。

```rust
// os/src/task/mod.rs

impl TaskManager {
    ...
    
    /// ch3
    fn get_current_syscall_count(&self, syscall_id: usize) -> isize {
        let mut inner = self.inner.exclusive_access();
        let current_task: usize = inner.current_task;
        let current_counters = inner.syscall_recorder
            .entry(current_task)
            .or_insert_with(BTreeMap::new);
        *current_counters.entry(syscall_id).or_insert(0) as isize
    }

    /// ch3
    fn record_syscall(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current_task: usize = inner.current_task;
        let current_counters = inner.syscall_recorder
            .entry(current_task)
            .or_insert_with(BTreeMap::new);
        *current_counters.entry(syscall_id).or_insert(0) += 1;
    }
}

lazy_static! {
    /// Global variable: TASK_MANAGER
    pub static ref TASK_MANAGER: TaskManager = {
	      ...
	      
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                    // ch3
                    syscall_recorder: BTreeMap::new(),
                })
            },
        }
    };
}

/// ch3
pub fn get_current_syscall_count(syscall_id: usize) -> isize {
    TASK_MANAGER.get_current_syscall_count(syscall_id)
}

/// ch3
pub fn record_syscall(syscall_id: usize) {
    TASK_MANAGER.record_syscall(syscall_id);
}
```

在内核的 `syscall` 函数中，调用 `record_syscall` 为当前任务的系统调用进行计数。在 process.rs 中实现 `sys_trace` 函数，通过模式匹配处理 trace_request 的三种模式（0、1、2），其中模式 2 直接调用 `get_current_syscall_count` 获取当前任务的系统调用计数。

```rust
// os/src/syscall/mod.rs

// ch3
use crate::task::record_syscall;

/// handle syscall exception with `syscall_id` and other arguments
pub fn syscall(syscall_id: usize, args: [usize; 3]) -> isize {
    // ch3
    record_syscall(syscall_id);

    match syscall_id {
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(args[0] as *mut TimeVal, args[1]),
        SYSCALL_TRACE => sys_trace(args[0], args[1], args[2]),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    }
}
```

```rust
// os/syscall/process.rs

use crate::{
    task::{exit_current_and_run_next, suspend_current_and_run_next, get_current_syscall_count},
    timer::get_time_us,
};

// ch3: syscall 接到 syscall_id=410，转到这里的实现
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    match trace_request {
        0 => {
            let ptr = id as *const u8;
            unsafe { *ptr as isize }
        }
        1 => {
            let ptr = id as *mut u8;
            unsafe {
                *ptr = data as u8;
            }
            0
        }
        2 => {
            get_current_syscall_count(id)
        }
        _ => -1
    }
}
```

# 通过测例

```
[rustsbi] RustSBI version 0.3.0-alpha.2, adapting to RISC-V SBI v1.0.0
.______       __    __      _______.___________.  _______..______   __
|   _  \     |  |  |  |    /       |           | /       ||   _  \ |  |
|  |_)  |    |  |  |  |   |   (----`---|  |----`|   (----`|  |_)  ||  |
|      /     |  |  |  |    \   \       |  |      \   \    |   _  < |  |
|  |\  \----.|  `--'  |.----)   |      |  |  .----)   |   |  |_)  ||  |
| _| `._____| \______/ |_______/       |__|  |_______/    |______/ |__|
[rustsbi] Implementation     : RustSBI-QEMU Version 0.2.0-alpha.2
[rustsbi] Platform Name      : riscv-virtio,qemu
[rustsbi] Platform SMP       : 1
[rustsbi] Platform Memory    : 0x80000000..0x88000000
[rustsbi] Boot HART          : 0
[rustsbi] Device Tree Region : 0x87000000..0x87000ef2
[rustsbi] Firmware Address   : 0x80000000
[rustsbi] Supervisor Address : 0x80200000
[rustsbi] pmp01: 0x00000000..0x80000000 (-wr)
[rustsbi] pmp02: 0x80000000..0x80200000 (---)
[rustsbi] pmp03: 0x80200000..0x88000000 (xwr)
[rustsbi] pmp04: 0x88000000..0x00000000 (-wr)
[kernel] Hello, world!
get_time OK! 14
current time_msec = 15
time_msec = 116 after sleeping 100 ticks, delta = 101ms!
Test sleep1 passed!
string from task trace test

Test trace OK!
Test sleep OK!
[kernel] Panicked at src/task/mod.rs:141 All applications completed!
```