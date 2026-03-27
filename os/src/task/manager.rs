//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
/// 调度器
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
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
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
