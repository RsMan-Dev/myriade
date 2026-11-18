//! The user-space instructions retired by THIS thread (`perf_event_open`, no other thread, no kernel): a bench reads it
//! around its timed ticks, so the number does not carry the world build or the thread pools.
#[repr(C)]
#[derive(Default)]
struct Attr {
    kind: u32,
    size: u32,
    config: u64,
    sample_period: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    rest: [u64; 11],
}
/// A counter of the instructions of the calling thread, or `None` where the kernel refuses it.
pub struct Counter(i32);
impl Counter {
    pub fn new() -> Option<Self> {
        let attr = Attr { kind: 0, size: std::mem::size_of::<Attr>() as u32, config: 1, flags: (1 << 5) | (1 << 6), ..Default::default() };
        // SAFETY: a plain system call with a valid attribute block.
        let fd = unsafe { libc::syscall(libc::SYS_perf_event_open, &attr as *const Attr, 0, -1, -1, 0) };
        (fd >= 0).then_some(Self(fd as i32))
    }
    pub fn read(&self) -> u64 {
        let mut v = 0_u64;
        // SAFETY: reads 8 bytes into `v`.
        unsafe { libc::read(self.0, &mut v as *mut u64 as *mut libc::c_void, 8) };
        v
    }
}
impl Drop for Counter {
    fn drop(&mut self) {
        // SAFETY: closes the descriptor this value owns.
        unsafe { libc::close(self.0) };
    }
}
