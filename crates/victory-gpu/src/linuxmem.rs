//! The runtime's memory calls under Wine, answered by Linux itself
//! (docs: LINUXMEM.md, stage S1). Every hot memory call of ours is, under
//! Wine, a trip through its system call dispatcher, its one address-space
//! lock (`virtual_mutex`) and often the wineserver. A small unix library of
//! ours beside the program (`victory_mem.so`, `victory-hle/unix`) does the
//! same on the kernel directly, inside ranges Wine already believes are
//! ours:
//!
//! - `VICTORY_DIRECT_PAGES=1`: a page mapping is a reservation Wine makes
//!   without the wineserver, with the section's pages shown there by
//!   `mremap` ([`show`]; `xmem` in the runtime).
//! - `VICTORY_NATIVE_SCAN=1`: the written pages of write-watched memory by
//!   `PAGEMAP_SCAN` ([`written`]), where `GetWriteWatch` holds the lock over
//!   the same call.
//! - `VICTORY_NATIVE_PRESENT=1`: whether pages can be read by one
//!   `process_vm_readv` ([`readable`]), where `QueryWorkingSetEx` holds the
//!   lock over a read of `/proc/self/pagemap`.
//! - `VICTORY_NATIVE_THREAD_TIMES`: the meters' thread processor time by
//!   `getrusage(RUSAGE_THREAD)` ([`thread_times`]), where `GetThreadTimes` is
//!   a wineserver request and a read of `/proc`. Unset: on whenever the
//!   library is loaded for another switch; `1` loads it for this; `0` off.
//! - (Not wired in this tree: the calls are here, the guards in `watch` do
//!   not use them.) `VICTORY_NATIVE_PROTECT=1` (LINUXMEM.md S2a): the guards on told views
//!   by `mprotect` ([`protect`]), merged over views, where `VirtualProtect`
//!   holds the lock over the same call, a view at a time.
//! - `VICTORY_GUARD_UFFD=1` (S2b): the guards on told views by our own
//!   userfaultfd in async write-protect mode ([`uffd_watch`], [`dirty`]):
//!   no change of protection, no signal.
//!
//! All off by default but the thread times. Each falls back to the Windows call when we are not
//! under Wine, the library is not there or its probe says the kernel cannot;
//! one log line each says which way is taken.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// The library's argument layouts (its `VERSION`).
const VERSION: u32 = 2;
const KERNEL_PAGE: usize = 4096;
/// `PAGE_IS_WRITTEN`, `PAGE_IS_PRESENT` (linux/fs.h).
const WRITTEN: u64 = 1 << 1;
const PRESENT: u64 = 1 << 3;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Caps {
    version: u32,
    pub pagemap_scan: u32,
    pub readv: u32,
    pub populate: u32,
}

/// Pages of a shared mapping at `from` to be shown at `at` as well; what is
/// mapped at `at` goes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ShowOp {
    pub from: usize,
    pub at: usize,
    pub bytes: usize,
}

#[repr(C)]
struct Show {
    ops: *const ShowOp,
    n: usize,
    flags: usize,
    done: usize,
    error: isize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Run {
    start: u64,
    end: u64,
    categories: u64,
}

#[repr(C)]
struct Scan {
    base: usize,
    len: usize,
    out: *mut Run,
    cap: usize,
    category: u64,
    flags: u64,
    found: isize,
    walk_end: usize,
}

#[repr(C)]
struct Readable {
    base: usize,
    len: usize,
    all: usize,
    there: usize,
    of: usize,
}

#[repr(C)]
struct Times {
    user: u64,
    kernel: u64,
    error: isize,
}

/// A run of pages and the access to them: 1 read-only, 3 read-write.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProtRun {
    pub at: usize,
    pub bytes: usize,
    pub prot: usize,
}

#[repr(C)]
struct Protect {
    runs: *const ProtRun,
    n: usize,
    done: usize,
    error: isize,
}

#[repr(C)]
struct Uffd {
    error: isize,
    step: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WatchRun {
    pub at: usize,
    pub bytes: usize,
}

#[repr(C)]
struct Watch {
    runs: *const WatchRun,
    n: usize,
    flags: usize,
    done: usize,
    error: isize,
    step: usize,
}

type Call = unsafe extern "system" fn(u64, u32, *mut c_void) -> i32;

struct Library {
    handle: u64,
    call: Call,
    caps: Caps,
}

impl Library {
    fn call<T>(&self, code: u32, args: &mut T) -> bool {
        // SAFETY: the library's call of that number takes this struct.
        unsafe { (self.call)(self.handle, code, (&raw mut *args).cast()) == 0 }
    }
}

/// Whether we run under Wine (its ntdll exports `wine_get_version`).
fn on_wine() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    // SAFETY: plain lookups in ntdll, always loaded.
    unsafe {
        let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr().cast());
        !ntdll.is_null() && GetProcAddress(ntdll, c"wine_get_version".as_ptr().cast()).is_some()
    }
}

/// Loads the library: by Wine's own way for unix libraries named by path
/// (`NtQueryVirtualMemory`, class `MemoryWineLoadUnixLibByName`), as
/// LloydNicholson 7ed6ca02 found. `Err`: why not.
fn load() -> Result<Library, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    #[repr(C)]
    struct UnicodeString {
        length: u16,
        maximum: u16,
        buffer: *const u16,
    }
    type Query = unsafe extern "system" fn(isize, *const c_void, i32, *mut c_void, usize, *mut usize) -> i32;
    /// `MemoryWineLoadUnixLibByName` (Wine 9 and on).
    const LOAD_BY_NAME: i32 = 1002;
    if !on_wine() {
        return Err("not under Wine".into());
    }
    // (`VICTORY_MEM_SO`: another file, for tests.)
    let path = match std::env::var_os("VICTORY_MEM_SO") {
        Some(path) => std::path::PathBuf::from(path),
        None => std::env::current_exe().map_err(|e| e.to_string())?.with_file_name("victory_mem.so"),
    };
    if !path.is_file() {
        return Err(format!("{} is not there", path.display()));
    }
    let name: Vec<u16> = r"\??\".encode_utf16().chain(path.as_os_str().encode_wide()).collect();
    let length = u16::try_from(name.len() * 2).map_err(|_| "path too long")?;
    let name = UnicodeString { length, maximum: length, buffer: name.as_ptr() };
    // SAFETY: ntdll's exports by their own signatures; Wine's query takes
    // the library's NT path and gives the module and the call handle.
    let (handle, call) = unsafe {
        let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr().cast());
        let query: Query = std::mem::transmute(
            GetProcAddress(ntdll, c"NtQueryVirtualMemory".as_ptr().cast()).ok_or("no NtQueryVirtualMemory")?,
        );
        let call = GetProcAddress(ntdll, c"__wine_unix_call_dispatcher".as_ptr().cast())
            .ok_or("no __wine_unix_call_dispatcher")? as *const *const c_void;
        if (*call).is_null() {
            return Err("no unix call dispatcher".into());
        }
        let call: Call = std::mem::transmute(*call);
        let mut out = [0u64; 2];
        let status = query(
            -1,
            (&raw const name).cast(),
            LOAD_BY_NAME,
            out.as_mut_ptr().cast(),
            size_of_val(&out),
            std::ptr::null_mut(),
        );
        if status != 0 || out[1] == 0 {
            return Err(format!("Wine did not load {} (status {status:#x})", path.display()));
        }
        (out[1], call)
    };
    let mut library = Library { handle, call, caps: Caps::default() };
    let mut caps = Caps::default();
    if !library.call(0, &mut caps) || caps.version != VERSION {
        return Err(format!("{} is of another build (version {}, wanted {VERSION})", path.display(), caps.version));
    }
    library.caps = caps;
    Ok(library)
}

struct State {
    library: Option<Library>,
    direct_pages: bool,
    native_scan: bool,
    native_present: bool,
    native_times: bool,
    native_protect: bool,
    guard_uffd: bool,
}

fn asked(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v.trim() == "1")
}

fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(|| {
        let mut wanted = [
            "VICTORY_DIRECT_PAGES",
            "VICTORY_NATIVE_SCAN",
            "VICTORY_NATIVE_PRESENT",
            "VICTORY_NATIVE_PROTECT",
            "VICTORY_GUARD_UFFD",
            "VICTORY_NATIVE_THREAD_TIMES",
        ]
        .map(asked);
        // (Not loaded at all with every switch off.)
        let library = if wanted.contains(&true) { load() } else { Err("not asked for".into()) };
        // The meters' clock: with the library whenever it is there, unless told not to.
        wanted[5] |= library.is_ok() && std::env::var("VICTORY_NATIVE_THREAD_TIMES").is_err();
        let caps = library.as_ref().map(|l| l.caps).unwrap_or_default();
        // (VICTORY_NATIVE_SCAN: PAGEMAP_SCAN alone is not enough; the probe
        // asks a write-watched allocation of ours whether Wine's watches are
        // the userfaultfd ones the scan can read. Not under Wine, no.)
        let scan_probe = match (&library, wanted[1], caps.pagemap_scan) {
            (Ok(library), true, non_zero) if non_zero != 0 => Some(write_watch_works(library)),
            _ => None,
        };
        if scan_probe == Some(false) {
            tracing::warn!(
                "VICTORY_NATIVE_SCAN=1 but falling back to GetWriteWatch: this Wine does not register MEM_WRITE_WATCH memory with userfaultfd async write-protect"
            );
        }
        let scan_ok = scan_probe != Some(false);
        // Our own userfaultfd, opened when asked for: why not, if not.
        let uffd = match &library {
            Ok(library) if wanted[4] => {
                let mut args = Uffd { error: 0, step: 0 };
                if !library.call(6, &mut args) {
                    Err("the library's call failed".to_string())
                } else if args.error != 0 {
                    let what = if args.step == 1 { "userfaultfd(UFFD_USER_MODE_ONLY)" } else { "UFFDIO_API (async write-protect of shared memory, Linux 6.7)" };
                    Err(format!("{what} failed, errno {}", -args.error))
                } else if caps.pagemap_scan == 0 {
                    Err("no PAGEMAP_SCAN".to_string())
                } else {
                    Ok(())
                }
            }
            _ => Err(String::new()),
        };
        let times = library.as_ref().is_ok_and(|library| {
            let mut args = Times { user: 0, kernel: 0, error: 0 };
            library.call(4, &mut args) && args.error == 0
        });
        let can = [library.is_ok(), caps.pagemap_scan != 0 && scan_ok, caps.readv != 0, library.is_ok(), uffd.is_ok(), times];
        if let (true, Ok(_), Err(why)) = (wanted[4], &library, &uffd) {
            tracing::warn!("VICTORY_GUARD_UFFD=1: no userfaultfd of ours: {why}");
        }
        let said = [
            ("VICTORY_DIRECT_PAGES", "title pages shown by mremap, no wineserver request", "views of the section (MapViewOfFile3)"),
            ("VICTORY_NATIVE_SCAN", "written pages asked of the kernel (PAGEMAP_SCAN), no virtual_mutex", "GetWriteWatch"),
            ("VICTORY_NATIVE_PRESENT", "readable pages asked of the kernel (process_vm_readv), no virtual_mutex", "QueryWorkingSetEx"),
            // (Port to cool_fox: the library's calls and `protect` / `uffd_watch` /
            // `dirty` here are whole, but this tree's guards (`watch`) are not
            // changed to use them; these two switches only probe.)
            ("VICTORY_NATIVE_PROTECT", "mprotect by the library is there, but this tree's guards do not use it: guards by VirtualProtect as before", "VirtualProtect"),
            ("VICTORY_GUARD_UFFD", "our own userfaultfd (async write-protect) opened, but this tree's guards do not use it: guards by protection as before", "guards by protection"),
            ("VICTORY_NATIVE_THREAD_TIMES", "the meters' thread processor time by getrusage(RUSAGE_THREAD), no wineserver request", "GetThreadTimes"),
        ];
        for (i, (name, native, win32)) in said.into_iter().enumerate() {
            // (The native scan's own warning above says why it fell back.)
            if i == 1 && scan_probe == Some(false) {
                continue;
            }
            match (wanted[i], can[i], &library) {
                (false, ..) => tracing::info!("{name}: off (unset): {win32}"),
                (true, true, _) => tracing::info!("{name}=1: {native} (victory_mem.so)"),
                (true, false, Err(why)) => tracing::warn!("{name}=1 but falling back to {win32}: {why}"),
                (true, false, Ok(_)) => tracing::warn!("{name}=1 but falling back to {win32}: this kernel cannot ({caps:?})"),
            }
        }
        // (The thread times are on because the library is, not because the
        // switch was set: say so, with the way off.)
        if can[5] && wanted[5] && std::env::var("VICTORY_NATIVE_THREAD_TIMES").is_err() {
            tracing::info!(
                "VICTORY_NATIVE_THREAD_TIMES: on because victory_mem.so is loaded (set it to 0 for GetThreadTimes)"
            );
        }
        State {
            direct_pages: wanted[0] && can[0],
            native_scan: wanted[1] && can[1],
            native_present: wanted[2] && can[2],
            native_protect: wanted[3] && can[3],
            guard_uffd: wanted[4] && can[4],
            native_times: wanted[5] && can[5],
            library: library.ok(),
        }
    })
}

/// Whether page mappings are shown by [`show`] (`VICTORY_DIRECT_PAGES=1`,
/// under Wine, the library loaded).
pub fn direct_pages() -> bool {
    state().direct_pages
}

/// Whether [`written`] answers (`VICTORY_NATIVE_SCAN=1`, the kernel can).
pub fn native_scan() -> bool {
    state().native_scan
}

/// Whether [`readable`] answers (`VICTORY_NATIVE_PRESENT=1`, the kernel can).
pub fn native_present() -> bool {
    state().native_present
}

/// Whether [`protect`] is for the guards (`VICTORY_NATIVE_PROTECT=1`).
pub fn native_protect() -> bool {
    state().native_protect
}

/// Whether the guards on told views are our userfaultfd's
/// (`VICTORY_GUARD_UFFD=1`, the kernel can).
pub fn guard_uffd() -> bool {
    state().guard_uffd
}

/// What the guards did, since the last [`guards_line`] (the first two
/// rows) or since the start: see [`G`].
pub static GUARDS: [AtomicU64; G::COUNT] = [const { AtomicU64::new(0) }; G::COUNT];

/// The places of [`GUARDS`].
#[allow(non_snake_case)]
pub mod G {
    /// Changes of protection asked of Windows (`VirtualProtect`, under
    /// `virtual_mutex`), to read-only and back; their blocks (64 KiB).
    pub const WIN_RO: usize = 0;
    pub const WIN_RW: usize = 1;
    pub const WIN_BLOCKS: usize = 2;
    /// `mprotect` runs of [`super::protect`] to read-only and back; blocks.
    pub const NATIVE_RO: usize = 3;
    pub const NATIVE_RW: usize = 4;
    pub const NATIVE_BLOCKS: usize = 5;
    /// Runs write-protected by our userfaultfd; their blocks.
    pub const UFFD_ARMS: usize = 6;
    pub const UFFD_BLOCKS: usize = 7;
    /// Runs registered with it; their blocks.
    pub const UFFD_REGISTERS: usize = 8;
    pub const UFFD_REGISTERED: usize = 9;
    /// Askings for written pages of protected runs; KiB walked; ns; 4 KiB
    /// pages found written; blocks opened for them (what faults were).
    pub const UFFD_SCANS: usize = 10;
    pub const UFFD_KIB: usize = 11;
    pub const UFFD_NS: usize = 12;
    pub const UFFD_PAGES: usize = 13;
    pub const UFFD_OPENED: usize = 14;
    /// Guard faults taken (SIGSEGV to our handler, a block opened).
    pub const FAULTS: usize = 15;
    /// Runs the userfaultfd refused (then `mprotect`), runs `mprotect`
    /// refused (then `VirtualProtect`).
    pub const UFFD_FELL: usize = 16;
    pub const NATIVE_FELL: usize = 17;
    /// Thread times read natively.
    pub const TIMES: usize = 18;
    pub const COUNT: usize = 19;
}

/// Adds to [`GUARDS`].
pub fn guards_count(what: usize, n: u64) {
    GUARDS[what].fetch_add(n, Relaxed);
}

/// [`GUARDS`] since the last line, a frame, for the log.
pub fn guards_line(frames: f64) -> String {
    let n: Vec<f64> = GUARDS.iter().map(|n| n.swap(0, Relaxed) as f64 / frames.max(1.0)).collect();
    let free = n[G::NATIVE_RO] + n[G::NATIVE_RW] + n[G::UFFD_ARMS] + n[G::UFFD_SCANS] + n[G::UFFD_REGISTERS] + n[G::TIMES];
    format!(
        "guards by path per frame: VirtualProtect {:.0} read-only + {:.0} back ({:.0} blocks); native mprotect {:.0} + {:.0} runs \
         ({:.0} blocks); userfaultfd {:.0} runs protected ({:.0} blocks), {:.1} runs registered ({:.1} blocks), {:.0} askings over \
         {:.0} KiB in {:.2} ms found {:.0} written pages and opened {:.0} blocks; guard faults {:.0}; fallbacks userfaultfd to \
         mprotect {:.1}, mprotect to VirtualProtect {:.1}; thread times read natively {:.0}; calls without virtual_mutex {:.0}",
        n[G::WIN_RO],
        n[G::WIN_RW],
        n[G::WIN_BLOCKS],
        n[G::NATIVE_RO],
        n[G::NATIVE_RW],
        n[G::NATIVE_BLOCKS],
        n[G::UFFD_ARMS],
        n[G::UFFD_BLOCKS],
        n[G::UFFD_REGISTERS],
        n[G::UFFD_REGISTERED],
        n[G::UFFD_SCANS],
        n[G::UFFD_KIB],
        n[G::UFFD_NS] / 1e6,
        n[G::UFFD_PAGES],
        n[G::UFFD_OPENED],
        n[G::FAULTS],
        n[G::UFFD_FELL],
        n[G::NATIVE_FELL],
        n[G::TIMES],
        free
    )
}

/// The calling thread's processor time so far (user, kernel) in
/// microseconds, by one system call of its own (the kernel's exact run
/// time, split by its ticks; `GetThreadTimes` under Wine gives ticks).
/// `None`: not on (`VICTORY_NATIVE_THREAD_TIMES`), ask Windows.
pub fn thread_times() -> Option<(u64, u64)> {
    let state = state();
    if !state.native_times {
        return None;
    }
    let mut args = Times { user: 0, kernel: 0, error: 0 };
    if !state.library.as_ref()?.call(4, &mut args) || args.error != 0 {
        return None;
    }
    GUARDS[G::TIMES].fetch_add(1, Relaxed);
    Some((args.user, args.kernel))
}

/// `mprotect` of each run, in order, in one call; `Err`: the error number
/// of the first that failed (those before it are done). Each run must lie
/// in mappings of the caller's that Wine believes committed read-write
/// memory, and stay there for the call; Wine is not told of the change.
pub fn protect(runs: &[ProtRun]) -> Result<(), (usize, isize)> {
    let Some(library) = &state().library else { return Err((0, 0)) };
    let mut args = Protect { runs: runs.as_ptr(), n: runs.len(), done: 0, error: 0 };
    if !library.call(5, &mut args) {
        return Err((0, 0));
    }
    if args.done < runs.len() { Err((args.done, -args.error)) } else { Ok(()) }
}

/// With our userfaultfd: registers each run (`register`) and/or
/// write-protects its pages (`arm`: they read as not written by [`dirty`]
/// until their next write). `Err`: how many runs were done, the error
/// number of the next, and whether it was the registering that failed.
pub fn uffd_watch(runs: &[WatchRun], register: bool, arm: bool) -> Result<(), (usize, isize, bool)> {
    let Some(library) = &state().library else { return Err((0, 0, true)) };
    let flags = usize::from(register) | usize::from(arm) << 1;
    let mut args = Watch { runs: runs.as_ptr(), n: runs.len(), flags, done: 0, error: 0, step: 0 };
    if !library.call(7, &mut args) {
        return Err((0, 0, true));
    }
    if args.done < runs.len() { Err((args.done, -args.error, args.step == 1)) } else { Ok(()) }
}

/// The pages of `[base, base + len)` written since [`uffd_watch`] last
/// protected them, as runs added to `out`; nothing is reset. (Pages of
/// memory not registered read as written, all of them that are there.)
pub fn dirty(base: usize, len: usize, out: &mut Vec<(usize, usize)>) -> bool {
    let Some(library) = &state().library else { return false };
    len != 0 && scan(library, base, len, WRITTEN, 0, out)
}

/// What the library's probe found, if it is loaded.
pub fn caps() -> Option<Caps> {
    state().library.as_ref().map(|l| l.caps)
}

/// Since the start: ops of [`show`]; askings of [`written`], those refused
/// (not write-watched memory), 4 KiB pages they found; askings of
/// [`readable`]; questions about readable pages answered by the views told.
pub static DONE: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

/// [`DONE`], for a log line.
pub fn done_line() -> String {
    let [shown, scans, refused, pages, readable, by_views] = [0, 1, 2, 3, 4, 5].map(|i| DONE[i].load(Relaxed));
    format!(
        "{shown} pages shown by mremap; {scans} native askings for written pages ({refused} refused, {pages} pages found); \
         {readable} native askings for readable pages, {by_views} answered by the views told"
    )
}

/// Shows each op's pages (a shared mapping at `from`) at `at` too, in one
/// call, in order; how many were done (the first failure stops). Each `at`
/// must be a reservation of the caller's that Wine knows as committed
/// read-write memory. `populate`: the pages are faulted in at once.
pub fn show(ops: &[ShowOp], populate: bool) -> usize {
    let Some(library) = &state().library else { return 0 };
    let flags = usize::from(populate && library.caps.populate != 0);
    let mut args = Show { ops: ops.as_ptr(), n: ops.len(), flags, done: 0, error: 0 };
    if !library.call(1, &mut args) {
        return 0;
    }
    if args.done < ops.len() {
        let op = ops[args.done];
        tracing::warn!(
            from = format_args!("{:#x}", op.from),
            at = format_args!("{:#x}", op.at),
            bytes = op.bytes,
            errno = -args.error,
            "mremap of title pages failed"
        );
    }
    DONE[0].fetch_add(args.done as u64, Relaxed);
    args.done
}

/// One walk of the kernel over `[base, base + len)` for pages of
/// `category`; their runs `(start, end)` are added to `out`. False if the
/// kernel refused (with `flags` 2: not write-watched memory).
fn scan(library: &Library, base: usize, len: usize, category: u64, flags: u64, out: &mut Vec<(usize, usize)>) -> bool {
    let mut at = base & !(KERNEL_PAGE - 1);
    let end = base.saturating_add(len).next_multiple_of(KERNEL_PAGE);
    let mut runs = [Run::default(); 256];
    while at < end {
        let mut args = Scan {
            base: at,
            len: end - at,
            out: runs.as_mut_ptr(),
            cap: runs.len(),
            category,
            flags,
            found: 0,
            walk_end: at,
        };
        if !library.call(2, &mut args) || args.found < 0 {
            return false;
        }
        let found = args.found as usize;
        for run in &runs[..found] {
            let (start, stop) = (run.start as usize, run.end as usize);
            match out.last_mut() {
                Some(last) if last.1 == start => last.1 = stop,
                _ => out.push((start, stop)),
            }
        }
        // (Full: the kernel stopped there, and reset nothing after it.)
        if found < runs.len() || args.walk_end <= at {
            break;
        }
        at = args.walk_end;
    }
    true
}

/// Whether Wine registers a `MEM_WRITE_WATCH` reservation with userfaultfd
/// async write-protect, so that a `PAGEMAP_SCAN` of it answers: a one-time
/// probe on an allocation of our own, run before `VICTORY_NATIVE_SCAN=1` is
/// believed. A Wine that keeps write watches by `mprotect` refuses every
/// asking; the switch then falls back instead of reading all watched memory
/// as unwatched.
fn write_watch_works(library: &Library) -> bool {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree,
    };
    const MEM_WRITE_WATCH: u32 = 0x20_0000;
    const BYTES: usize = 64 * 1024;
    // SAFETY: a fresh allocation of ours, written to and freed here.
    let base = unsafe {
        VirtualAlloc(
            std::ptr::null(),
            BYTES,
            MEM_RESERVE | MEM_COMMIT | MEM_WRITE_WATCH,
            PAGE_READWRITE,
        )
    };
    if base.is_null() {
        return false;
    }
    // SAFETY: the first page of the allocation above.
    unsafe { std::ptr::write_bytes(base.cast::<u8>(), 0x5a, KERNEL_PAGE) };
    let mut runs = Vec::new();
    let answered = scan(library, base as usize, BYTES, WRITTEN, 3, &mut runs);
    // SAFETY: our allocation, released whole.
    unsafe { VirtualFree(base, 0, MEM_RELEASE) };
    answered && !runs.is_empty()
}

/// The pages of `[base, base + len)` written since they were last reset, as
/// runs `(start, end)` of whole 4 KiB pages added to `out`, in order; with
/// `reset` they count as not written from now on (what `GetWriteWatch` and
/// its reset flag do, by the same kernel call Wine makes for them, without
/// Wine). `None`: the range is not all write-watched memory
/// (`MEM_WRITE_WATCH` reservations, which Wine registers with userfaultfd),
/// and nothing was reset; or [`native_scan`] is off. A failure after some
/// runs were found and reset returns those runs (`Some`) rather than
/// `None`, so what was reset is never also reported as untouched; the rest
/// of the range was not reset and is found by the next asking.
///
/// Unlike `GetWriteWatch` a range over several write-watched reservations
/// that follow one another is answered. A range must be asked this way or
/// by `GetWriteWatch`, never both: each would reset what the other has not
/// seen.
pub fn written(base: usize, len: usize, reset: bool, out: &mut Vec<(usize, usize)>) -> Option<()> {
    let state = state();
    if !state.native_scan || len == 0 {
        return None;
    }
    let library = state.library.as_ref()?;
    let before = out.len();
    DONE[1].fetch_add(1, Relaxed);
    if !scan(library, base, len, WRITTEN, u64::from(reset) | 2, out) {
        // (The kernel's pre-check covers the whole remaining range before
        // the first reset, so a refusal normally resets nothing. The chunks
        // answered before a later failure were reset and are in `out`: they
        // are reported, not thrown away; what is left was not reset.)
        let pages: usize = out[before..].iter().map(|r| (r.1 - r.0) / KERNEL_PAGE).sum();
        DONE[2].fetch_add(1, Relaxed);
        if out.len() == before {
            return None;
        }
        DONE[3].fetch_add(pages as u64, Relaxed);
        tracing::warn!(
            base = format_args!("{base:#x}"),
            len,
            pages,
            "native scan stopped after some runs; those are reported, the rest was not reset"
        );
        return Some(());
    }
    let pages: usize = out[before..].iter().map(|r| (r.1 - r.0) / KERNEL_PAGE).sum();
    DONE[3].fetch_add(pages as u64, Relaxed);
    Some(())
}

/// The pages of `[base, base + len)` that are in physical memory now, as
/// runs added to `out` (LINUXMEM.md's `mem_present`). A page that is
/// present may still be forbidden to read: [`readable`] says that.
pub fn present(base: usize, len: usize, out: &mut Vec<(usize, usize)>) -> bool {
    let Some(library) = &state().library else { return false };
    library.caps.pagemap_scan != 0 && len != 0 && scan(library, base, len, PRESENT, 0, out)
}

/// Of the 4 KiB pages that hold `[base, base + len)`: how many can be read,
/// and how many were looked at; with `all` it stops at the first that
/// cannot. The kernel reads a byte of each (no fault of ours, no lock of
/// Wine's); a page not touched yet is faulted in, as a read would.
/// `None`: [`native_present`] is off.
pub fn readable(base: usize, len: usize, all: bool) -> Option<(usize, usize)> {
    let state = state();
    if !state.native_present || len == 0 {
        return None;
    }
    let library = state.library.as_ref()?;
    let mut args = Readable { base, len, all: usize::from(all), there: 0, of: 0 };
    if !library.call(3, &mut args) {
        return None;
    }
    DONE[4].fetch_add(1, Relaxed);
    Some((args.there, args.of))
}

/// For tests and `membench`: the library's calls whatever the switches say
/// (loaded on first use if no switch asked for it).
pub mod raw {
    use super::*;

    fn library() -> Option<&'static Library> {
        static OWN: OnceLock<Option<Library>> = OnceLock::new();
        if let Some(library) = &state().library {
            return Some(library);
        }
        OWN.get_or_init(|| load().map_err(|why| tracing::warn!("victory_mem.so: {why}")).ok()).as_ref()
    }

    pub fn caps() -> Option<Caps> {
        library().map(|l| l.caps)
    }

    pub fn show(ops: &[ShowOp], populate: bool) -> usize {
        let Some(library) = library() else { return 0 };
        let mut args = Show { ops: ops.as_ptr(), n: ops.len(), flags: usize::from(populate), done: 0, error: 0 };
        library.call(1, &mut args);
        args.done
    }

    /// [`super::written`]; `checked`: refused unless all write-watched.
    pub fn written(base: usize, len: usize, reset: bool, checked: bool, out: &mut Vec<(usize, usize)>) -> bool {
        library().is_some_and(|l| scan(l, base, len, WRITTEN, u64::from(reset) | u64::from(checked) << 1, out))
    }

    pub fn present(base: usize, len: usize, out: &mut Vec<(usize, usize)>) -> bool {
        library().is_some_and(|l| scan(l, base, len, PRESENT, 0, out))
    }

    pub fn readable(base: usize, len: usize, all: bool) -> Option<(usize, usize)> {
        let mut args = Readable { base, len, all: usize::from(all), there: 0, of: 0 };
        library()?.call(3, &mut args).then_some((args.there, args.of))
    }
}
