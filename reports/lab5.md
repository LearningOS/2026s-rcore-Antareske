# lab5

实现类似银行家算法的死锁检测功能。锁和信号量是线程间争夺的资源，无论是锁和信号量还是线程都维护在进程里，因此死锁检测也是进程级别的功能。在第八章中 rcore 实现了两类基本的并发资源：mutex 锁 和 semaphore 信号量，它们作为资源都有自己的 id。本实验的算法统计各个资源的可用数量，已分配数量和需求数量。分别为锁和信号量准备一套（类）银行家算法，以资源的 id 统计每个资源的供需情况，并在内核实际分配它们之前先行模拟一遍分配，若出现可预见的分配问题则及时阻止死锁的情况。

# 数据结构

## 定义

为两类锁资源（mutex 锁，semaphore 信号量）提供了死锁检测功能的封装，并把它们维护在 process_inner 的成员 sync_guard 里。

available 记录当前所有同种资源（比如 mutex）的可用数量，以资源的 id 作为键名统计，值为可用数量。allocation 和 need 以线程 id 统计各个线程的该种资源的已分配数量和需求数量，以线程 id（tid）为键名，资源情况为值进行统计，资源情况和 available 一样是资源 id 到数量的映射。

```rust
/// [INFO] CH8 deadlock protection

use alloc::collections::{BTreeMap, BTreeSet};

/// Banker-like mutex/semaphore deadlock detection
#[derive(Clone)]
pub struct ResourceGuard {
    /// Available: {sync_id, count}  // sync_id: mutex or sema
    pub available: BTreeMap<usize, usize>,
    /// Allocation: {tid, {sync_id, count}}
    pub allocation: BTreeMap<usize, BTreeMap<usize, usize>>,
    /// Need: {tid, {sync_id, count}}
    pub need: BTreeMap<usize, BTreeMap<usize, usize>>,
}

impl ResourceGuard {
    /// new
    fn new() -> Self {
        Self {
            available: BTreeMap::new(),
            need: BTreeMap::new(),
            allocation: BTreeMap::new(),
        }
    }

    /// insert thread
    fn insert_thread(&mut self, tid: usize) {
        let res_map: BTreeMap<usize, usize> = self.available.keys().map(|k| (*k, 0)).collect();
        self.allocation.insert(tid, res_map.clone());
        self.need.insert(tid, res_map);
    }

    /// remove thread
    fn remove_thread(&mut self, tid: usize) {
        self.allocation.remove(&tid);
        self.need.remove(&tid);
    }

    /// reset all res of id 0 in need and alloc; set res of id res_count in avail
    pub fn reset_resource(&mut self, id: usize, res_count: usize) {
        // 新加资源为 res_count
        self.available.insert(id, res_count);  // 插入或覆盖
        // 每个线程的 res_map，插入或覆盖为 0
        self.allocation.values_mut().for_each(|res_map| {
            res_map.insert(id, 0);
        });
        self.need.values_mut().for_each(|res_map| {
            res_map.insert(id, 0);
        });
    }

    /// 检测是否死锁。应当先更新 need 再调用本方法
    pub fn is_deadlock(&self) -> bool {
        let mut work = self.available.clone();
        let mut finish_true = BTreeSet::new();
        let need = self.need.clone();

        while let Some((tid, _)) = need.iter()
            .find(|(tid, need_res_map)| {
                !finish_true.contains(*tid)  // 是 false
                    && need_res_map.iter().all(|(id, needing)| needing <= work.get(id).unwrap())
            }) {
                finish_true.insert(tid);
                work.iter_mut()
                    .for_each(|(id, amount)| *amount += self.allocation.get(tid).unwrap().get(id).unwrap());
        }

        finish_true.len() != need.len()
    }

}

/// Banker-like deadlock detection for process
#[derive(Clone)]
pub struct SyncGuard {
    /// whether deadlock detection is enabled
    pub enabled: bool,
    /// mutex record and detection
    pub mutex: ResourceGuard,
    /// semaphore record and detection
    pub semaphore: ResourceGuard,
}

impl SyncGuard {
    /// new
    pub fn new() -> Self {
        Self {
            enabled: false,
            mutex: ResourceGuard::new(),
            semaphore: ResourceGuard::new(),
        }
    }

    /// insert thread to both mutex and sema maps
    pub fn insert_thread(&mut self, tid: usize) {
        self.mutex.insert_thread(tid);
        self.semaphore.insert_thread(tid);
    }

    /// remove thread from both mutex and sema maps
    pub fn remove_thread(&mut self, tid: usize) {
        self.mutex.remove_thread(tid);
        self.semaphore.remove_thread(tid);
    }
}
```

两个关键的操作是 insert_thread 和 reset_resource。insert_thread 在创建线程时（包括进程初始化时创建主线程时）将这个新的线程更新到 allocation 和 need 里，即在 allocation 和 need 里插入（或覆盖）该 tid 的资源映射，映射中记录的资源与已有的资源一致（从 available 中获取），但数量都为 0。insert_thread 和资源无关，只是在统计中拓宽一层线程。SyncGuard 的同名方法同时对 mutex 和 semaphore 执行该操作。

reset_resource 添加或重置一个资源的记录，并记录其当前的可用数量，对 allocation 和 need 中记录的所有线程的资源映射都生效。insert 方法可以覆盖已有键值对，这么做的原因是 mutex 和 semaphore 被进程维护在队列中并以其索引作为 id，系统在创建资源时会优先使用队列中之前的资源被回收过后占位的 None 值，所以我们的算法处理 id 的行为也要一致。

```rust
// 创建 mutex 的系统调用中，会复用回收过的 id
pub fn sys_mutex_create(blocking: bool) -> isize {
    ...

    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        // 因为 mutex/sema id 可被复用，可能有残留记录，所以重置 alloc 和 need 状态，available 为 1
        process_inner.sync_guard.mutex.reset_resource(id, 1);
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        let id = process_inner.mutex_list.len() - 1;  // 先 push 再算 id
        // 同上
        process_inner.sync_guard.mutex.reset_resource(id, 1);
        id as isize
    }
}
```

## 死锁检测

根据描述：

<aside>
ℹ️

1. 设置两个向量: 工作向量 Work，表示操作系统可提供给线程继续运行所需的各类资源数目，它含有 m 个元素。初始时，Work = Available ；结束向量 Finish，表示系统是否有足够的资源分配给线程， 使之运行完成。初始时 Finish[0..n-1] = false，表示所有线程都没结束；当有足够资源分配给线程时， 设置 Finish[i] = true。
2. 从线程集合中找到一个能满足下述条件的线程
    
    ```rust
        1 Finish[i] == false;
        2 Need[i,j] <= Work[j];
    ```
    
    若找到，执行步骤 3，否则执行步骤 4。
    
3. 当线程 thr[i] 获得资源后，可顺利执行，直至完成，并释放出分配给它的资源，故应执行:
    
    ```rust
        1 Work[j] = Work[j] + Allocation[i, j];
        2 Finish[i] = true;
    ```
    
    跳转回步骤2
    
4. 如果 Finish[0..n-1] 都为 true，则表示系统处于安全状态；否则表示系统处于不安全状态，即出现死锁。
</aside>

因为使用了 BTreeMap 实现，所以做出适当调整：

```rust
/// Banker-like mutex/semaphore deadlock detection
#[derive(Clone)]
pub struct ResourceGuard {
    /// Available: {sync_id, count}  // sync_id: mutex or sema
    pub available: BTreeMap<usize, usize>,
    /// Allocation: {tid, {sync_id, count}}
    pub allocation: BTreeMap<usize, BTreeMap<usize, usize>>,
    /// Need: {tid, {sync_id, count}}
    pub need: BTreeMap<usize, BTreeMap<usize, usize>>,
}

impl ResourceGuard {
    ...

    /// 检测是否死锁。应当先更新 need 再调用本方法
    pub fn is_deadlock(&self) -> bool {
        let mut work = self.available.clone();
        let mut finish_true = BTreeSet::new();
        let need = self.need.clone();

        while let Some((tid, _)) = need.iter()
            .find(|(tid, need_res_map)| {
                !finish_true.contains(*tid)  // 是 false
                    && need_res_map.iter().all(|(id, needing)| needing <= work.get(id).unwrap())
            }) {
                finish_true.insert(tid);
                work.iter_mut()
                    .for_each(|(id, amount)| *amount += self.allocation.get(tid).unwrap().get(id).unwrap());
        }

        finish_true.len() != need.len()
    }

}
```

使用集合 finish_true，线程表示安全时将该线程 tid 加入到该集合中，最后若 finish_true 的长度不足所有线程，则说明有线程不安全，会出现死锁。

# 系统调用

在 mutex 和 semaphore 的相关系统调用中把它们的状态同步到 ResourceGuard 中；在实际分配它们的系统调用中，先行模拟它们的分配结果是否发生死锁。注意在分配前先在 need 中使该线程的该资源自增 1 以表示当前需求加一，然后执行死锁检测。若出现死锁，则需要回退 need 中自增的该线程的资源（使其自减 1），然后返回死锁信息；若未出现死锁，继续执行原来的系统调用，执行完毕后像其它系统调用一样将该线程的该资源的变化同步到 ResourceGuard  中。

```rust
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        // 因为 mutex/sema id 可被复用，可能有残留记录，所以重置 alloc 和 need 状态，available 为 1
        process_inner.sync_guard.mutex.reset_resource(id, 1);
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        let id = process_inner.mutex_list.len() - 1;  // 先 push 再算 id
        // 同上
        process_inner.sync_guard.mutex.reset_resource(id, 1);
        id as isize
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    // 需求加一后，检查当前可用能不能行
    *process_inner.sync_guard.mutex.need.get_mut(&tid).unwrap().get_mut(&mutex_id).unwrap() += 1;
    if process_inner.sync_guard.enabled {
        if process_inner.sync_guard.mutex.is_deadlock() {
            // 回退
            *process_inner.sync_guard.mutex.need.get_mut(&tid).unwrap().get_mut(&mutex_id).unwrap() -= 1;
            return -0xDEAD;
        }
    }
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.lock();
    // 更新状态
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let mutex_guard = &mut inner.sync_guard.mutex;
    // 分配完成，更新
    *mutex_guard.available.get_mut(&mutex_id).unwrap() -= 1;
    *mutex_guard.allocation.get_mut(&tid).unwrap().get_mut(&mutex_id).unwrap() += 1;
    *mutex_guard.need.get_mut(&tid).unwrap().get_mut(&mutex_id).unwrap() -= 1;  // 分配已完成，需求减一
    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    // 更新状态
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    *process_inner.sync_guard.mutex.allocation.get_mut(&tid).unwrap().get_mut(&mutex_id).unwrap() -= 1;
    *process_inner.sync_guard.mutex.available.get_mut(&mutex_id).unwrap() += 1;
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        process_inner.sync_guard.semaphore.reset_resource(id, res_count);
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        let id = process_inner.semaphore_list.len() - 1;
        process_inner.sync_guard.semaphore.reset_resource(id, res_count);
        return id as isize;
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    // V 操作，归还
    *process_inner.sync_guard.semaphore.available.get_mut(&sem_id).unwrap() += 1;
    *process_inner.sync_guard.semaphore.allocation.get_mut(&tid).unwrap().get_mut(&sem_id).unwrap() -= 1;
    drop(process_inner);
    sem.up();
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    // P 操作，申请
    // 需求加一
    *process_inner.sync_guard.semaphore.need.get_mut(&tid).unwrap().get_mut(&sem_id).unwrap() += 1;
    if process_inner.sync_guard.enabled {
        if process_inner.sync_guard.semaphore.is_deadlock() {
            // 回退
            *process_inner.sync_guard.semaphore.need.get_mut(&tid).unwrap().get_mut(&sem_id).unwrap() -= 1;
            return -0xDEAD;
        }
    }
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);  // [INFO] 漏了个 drop 查了一晚上
    sem.down();
    // 更新状态
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let sema_guard = &mut inner.sync_guard.semaphore;
    *sema_guard.available.get_mut(&sem_id).unwrap() -= 1;
    *sema_guard.allocation.get_mut(&tid).unwrap().get_mut(&sem_id).unwrap() += 1;
    *sema_guard.need.get_mut(&tid).unwrap().get_mut(&sem_id).unwrap() -= 1;
    0
}
```

# Bug 记录

sys_semaphore_down 里 sem.down(); 前漏了 drop(process_inner) 导致 RefMut panic 了一整晚才被我排查出来 😊。