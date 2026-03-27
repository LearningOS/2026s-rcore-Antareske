//! Process management syscalls
use crate::task::{
    change_program_brk,
    exit_current_and_run_next,
    suspend_current_and_run_next,
    current_user_token,
    // [INFO] ch4
    get_current_syscall_count,
    // [INFO] ch4
    mmap,
    // [INFO] ch4
    munmap,
};
use crate::timer::get_time_us;
// [INFO] ch4
use crate::mm::{translated_byte_buffer, translated_byte_ref_u8};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// [INFO] Ch4
/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    // -1
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

/// [INFO] Ch4
/// sys_trace 处理粒度为一字节，不可能跨页。
/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(_trace_request: usize, _id: usize, _data: usize) -> isize {
    trace!("kernel: sys_trace");

    let token = current_user_token();
    let addr = _id;
    if !(check_sv39_valid(addr)) {
        return -1;  // 非法地址
        /*
            这是针对 ch4 测例 ch4_trace1.rs 中期望 isize::MAX as usize as *const _ 为 None 的修改。
            原来的实现 (From<usize> for VirtAddr) 未对 usize 做 SV39 要求的高位的格式的检查,
            可能会转换出非法地址。
        */
    }
    match _trace_request {
        // read
        0 => {
            if let Some(byte_ref) = translated_byte_ref_u8(token, _id as *mut u8, false) {
                *byte_ref as isize
            } else {
                -1
            }
        }
        // write
        1 => {
            if let Some(byte_ref) = translated_byte_ref_u8(token, _id as *mut u8, true) {
                *byte_ref = _data as u8;
                0
            } else {
                -1
            }
        }
        // count
        2 => {
            get_current_syscall_count(_id)
        }
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    // -1
    // 返回 isize 状态码
    mmap(_start.into(), _len, _port)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    // -1
    munmap(_start.into(), _len)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

pub fn check_sv39_valid(addr: usize) -> bool {
    const VA_WIDTH: usize = 39;
    let bit38 = (addr >> (VA_WIDTH - 1)) & 1;
    let high = addr >> VA_WIDTH;
    let expected_high = if bit38 == 1 {
        (1 << (64 - VA_WIDTH)) - 1
    } else {
        0
    };
    high == expected_high
}