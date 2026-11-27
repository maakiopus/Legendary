//! The Linux side of the runtime's memory calls under Wine (`victory_gpu::
//! linuxmem` loads it; docs: LINUXMEM.md, stage S1). Wine keeps its own
//! account of every address: each call here only changes or asks what the
//! kernel shows inside a range Wine already believes is ours. No call takes
//! Wine's `virtual_mutex`, asks the wineserver or allocates.
//!
//! Calls (index into `__wine_unix_call_funcs`):
//! 0 probe     what this kernel can do; opens `/proc/self/pagemap` once
//! 1 show      pages of a shared mapping shown at other addresses (`mremap`)
//! 2 scan      `PAGEMAP_SCAN` of a range: runs of written or present pages
//! 3 readable  how many 4 KiB pages of a range can be read (`process_vm_readv`)
//!
//! Each returns 0 and reports in its argument (a negative error number where
//! a result is expected). Built by `build.sh` beside this file; needs no
//! library at all (no libc to match in whatever runtime Proton brings).
#![no_std]

use core::sync::atomic::{AtomicI32, Ordering::Relaxed};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// The layout of the argument structs; the PE side refuses another.
const VERSION: u32 = 1;

const OPEN: usize = 2;
const IOCTL: usize = 16;
const MREMAP: usize = 25;
const MADVISE: usize = 28;
const GETPID: usize = 39;
const PROCESS_VM_READV: usize = 310;

const O_RDONLY_CLOEXEC: usize = 0o2000000;
const MAYMOVE_FIXED: usize = 1 | 2;
const MADV_POPULATE_WRITE: usize = 23;
/// `_IOWR('f', 16, struct pm_scan_arg)`.
const PAGEMAP_SCAN: usize = 0xc060_6610;
const PM_SCAN_WP_MATCHING: u64 = 1;
const PM_SCAN_CHECK_WPASYNC: u64 = 2;
const EPERM: isize = 1;
const EBADF: isize = 9;
const KERNEL_PAGE: usize = 4096;

/// `/proc/self/pagemap`, opened by [`probe`], never closed; -1: none.
static PAGEMAP: AtomicI32 = AtomicI32::new(-1);
static PID: AtomicI32 = AtomicI32::new(0);

unsafe fn sys6(n: usize, a: usize, b: usize, c: usize, d: usize, e: usize, f: usize) -> isize {
    let result: isize;
    // SAFETY: the Linux system call convention; the caller answers for the call.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") n as isize => result,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            in("r10") d,
            in("r8") e,
            in("r9") f,
            out("rcx") _,
            out("r11") _,
            options(nostack)
        );
    }
    result
}

/// `struct pm_scan_arg` (linux/fs.h).
#[repr(C)]
struct PmScan {
    size: u64,
    flags: u64,
    start: u64,
    end: u64,
    walk_end: u64,
    vec: u64,
    vec_len: u64,
    max_pages: u64,
    category_inverted: u64,
    category_mask: u64,
    category_anyof_mask: u64,
    return_mask: u64,
}

/// `struct page_region`: pages `[start, end)` of the categories asked for.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Run {
    start: u64,
    end: u64,
    categories: u64,
}

unsafe fn pagemap_scan(arg: &mut PmScan) -> isize {
    let fd = PAGEMAP.load(Relaxed);
    if fd < 0 {
        return -EBADF;
    }
    // SAFETY: the kernel checks the range and the answer's room.
    unsafe { sys6(IOCTL, fd as usize, PAGEMAP_SCAN, arg as *mut PmScan as usize, 0, 0, 0) }
}

fn scan_arg(start: usize, end: usize, category: u64, flags: u64, vec: usize, vec_len: usize) -> PmScan {
    PmScan {
        size: size_of::<PmScan>() as u64,
        flags,
        start: start as u64,
        end: end as u64,
        walk_end: 0,
        vec: vec as u64,
        vec_len: vec_len as u64,
        max_pages: 0,
        category_inverted: 0,
        category_mask: category,
        category_anyof_mask: 0,
        return_mask: category,
    }
}

#[repr(C)]
pub struct Caps {
    /// Out: [`VERSION`].
    version: u32,
    /// Out: `PAGEMAP_SCAN` answers (Linux 6.7 and on).
    pagemap_scan: u32,
    /// Out: `process_vm_readv` of ourselves is allowed.
    readv: u32,
    /// Out: `MADV_POPULATE_WRITE` is known (Linux 5.14 and on).
    populate: u32,
}

unsafe extern "C" fn probe(args: *mut Caps) -> u32 {
    // SAFETY: the caller passes a `Caps`; the calls ask about our own stack.
    unsafe {
        let caps = &mut *args;
        caps.version = VERSION;
        if PAGEMAP.load(Relaxed) < 0 {
            let fd = sys6(OPEN, c"/proc/self/pagemap".as_ptr() as usize, O_RDONLY_CLOEXEC, 0, 0, 0, 0);
            // (Two threads probing at once leave a descriptor open: no harm.)
            if fd >= 0 {
                PAGEMAP.store(fd as i32, Relaxed);
            }
        }
        PID.store(sys6(GETPID, 0, 0, 0, 0, 0, 0) as i32, Relaxed);
        let here = (&raw const *caps) as usize & !(KERNEL_PAGE - 1);
        let mut arg = scan_arg(here, here + KERNEL_PAGE, 1 << 3, 0, 0, 0);
        caps.pagemap_scan = u32::from(pagemap_scan(&mut arg) >= 0);
        let mut read = Readable { base: here, len: 1, all: 1, there: 0, of: 0 };
        readable(&mut read);
        caps.readv = u32::from(read.there == 1);
        // (Zero bytes: only whether the kernel knows the advice.)
        caps.populate = u32::from(sys6(MADVISE, here, 0, MADV_POPULATE_WRITE, 0, 0, 0) == 0);
    }
    0
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ShowOp {
    /// Where the pages are mapped already (a shared mapping).
    from: usize,
    /// Where to show them too; what is mapped there goes.
    at: usize,
    bytes: usize,
}

#[repr(C)]
pub struct Show {
    ops: *const ShowOp,
    n: usize,
    /// Bit 0: fault the pages in at once (`MADV_POPULATE_WRITE`).
    flags: usize,
    /// Out: ops done; the first failure stops.
    done: usize,
    /// Out: 0, or the error number of the op that failed (negative).
    error: isize,
}

/// `mremap` with an old size of 0: a second mapping of the same shared
/// pages, made in one step in place of what is at `at`.
unsafe extern "C" fn show(args: *mut Show) -> u32 {
    // SAFETY: the caller passes a `Show` with `n` ops; each call replaces the
    // mapping at `at`, which the caller owns (a reservation Wine made).
    unsafe {
        let a = &mut *args;
        (a.done, a.error) = (0, 0);
        for i in 0..a.n {
            let op = *a.ops.add(i);
            let got = sys6(MREMAP, op.from, 0, op.bytes, MAYMOVE_FIXED, op.at, 0);
            if got != op.at as isize {
                a.error = if got < 0 { got } else { -EPERM };
                return 0;
            }
            if a.flags & 1 != 0 {
                sys6(MADVISE, op.at, op.bytes, MADV_POPULATE_WRITE, 0, 0, 0);
            }
            a.done += 1;
        }
    }
    0
}

#[repr(C)]
pub struct Scan {
    base: usize,
    len: usize,
    /// Room for `cap` runs.
    out: *mut Run,
    cap: usize,
    /// The page categories asked for (`PAGE_IS_WRITTEN` 2, `PAGE_IS_PRESENT`
    /// 8, `PAGE_IS_SWAPPED` 16): a page of all of them is told.
    category: u64,
    /// Bit 0: reset what is told as written (`PM_SCAN_WP_MATCHING`). Bit 1:
    /// the whole range must be write-watched memory (userfaultfd, async
    /// write-protect, as Wine registers `MEM_WRITE_WATCH` reservations):
    /// else nothing is told or reset and `found` is -EPERM.
    flags: u64,
    /// Out: runs written to `out`, or a negative error number.
    found: isize,
    /// Out: where the walk ended; before `base + len` if `out` was full
    /// (ask again from there: nothing after it was reset).
    walk_end: usize,
}

unsafe extern "C" fn scan(args: *mut Scan) -> u32 {
    // SAFETY: the caller passes a `Scan` whose `out` holds `cap` runs.
    unsafe {
        let a = &mut *args;
        let (start, end) = (a.base, a.base + a.len);
        let reset = if a.flags & 1 != 0 { PM_SCAN_WP_MATCHING } else { 0 };
        let check = if a.flags & 2 != 0 { PM_SCAN_CHECK_WPASYNC } else { 0 };
        a.walk_end = start;
        if reset != 0 && check != 0 {
            // A resetting walk that meets memory it must refuse has reset
            // what lay before it already: looked at first, without reset.
            let mut look = scan_arg(start, end, a.category, check, 0, 0);
            let got = pagemap_scan(&mut look);
            if got < 0 {
                a.found = got;
                return 0;
            }
        }
        let mut arg = scan_arg(start, end, a.category, reset | check, a.out as usize, a.cap);
        a.found = pagemap_scan(&mut arg);
        if a.found >= 0 {
            a.walk_end = arg.walk_end as usize;
        }
    }
    0
}

#[repr(C)]
pub struct Readable {
    base: usize,
    len: usize,
    /// Nonzero: stop at the first page that cannot be read.
    all: usize,
    /// Out: the 4 KiB pages that can be read.
    there: usize,
    /// Out: the pages looked at (all of them unless stopped).
    of: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IoVec {
    base: usize,
    len: usize,
}

/// A byte of each page read by the kernel, which faults nothing and takes
/// no signal for a page that is not there or may not be read: it says how
/// far it got. (A page not yet touched is faulted in, as a read would.)
unsafe extern "C" fn readable(args: *mut Readable) -> u32 {
    const BATCH: usize = 256;
    // SAFETY: the caller passes a `Readable`; the kernel checks the pages.
    unsafe {
        let a = &mut *args;
        (a.there, a.of) = (0, 0);
        if a.len == 0 {
            return 0;
        }
        let pid = PID.load(Relaxed) as usize;
        let last = (a.base + a.len - 1) / KERNEL_PAGE;
        let mut page = a.base / KERNEL_PAGE;
        // (Not zeroed: no `memset` to ask a C library for.)
        let mut bytes = core::mem::MaybeUninit::<[u8; BATCH]>::uninit();
        let mut remote = core::mem::MaybeUninit::<[IoVec; BATCH]>::uninit();
        let remote = remote.as_mut_ptr().cast::<IoVec>();
        while page <= last {
            let n = if last - page + 1 < BATCH { last - page + 1 } else { BATCH };
            let mut i = 0;
            while i < n {
                // (The first byte asked for itself, not its page's first.)
                let at = (page + i) * KERNEL_PAGE;
                remote.add(i).write(IoVec { base: if at < a.base { a.base } else { at }, len: 1 });
                i += 1;
            }
            let local = IoVec { base: bytes.as_mut_ptr() as usize, len: n };
            let got = sys6(PROCESS_VM_READV, pid, &raw const local as usize, 1, remote as usize, n, 0);
            let read = if got < 0 { 0 } else { got as usize };
            a.there += read;
            if read == n {
                a.of += n;
                page += n;
                continue;
            }
            // Page `page + read` cannot be read.
            a.of += read + 1;
            if a.all != 0 {
                return 0;
            }
            page += read + 1;
        }
    }
    0
}

type Call = unsafe extern "C" fn(*mut core::ffi::c_void) -> u32;

/// Wine's table of a unix library's calls.
#[unsafe(no_mangle)]
pub static __wine_unix_call_funcs: [Call; 4] = unsafe {
    // SAFETY: each takes one pointer; Wine passes the caller's.
    [
        core::mem::transmute::<unsafe extern "C" fn(*mut Caps) -> u32, Call>(probe),
        core::mem::transmute::<unsafe extern "C" fn(*mut Show) -> u32, Call>(show),
        core::mem::transmute::<unsafe extern "C" fn(*mut Scan) -> u32, Call>(scan),
        core::mem::transmute::<unsafe extern "C" fn(*mut Readable) -> u32, Call>(readable),
    ]
};
