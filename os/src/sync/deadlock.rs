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