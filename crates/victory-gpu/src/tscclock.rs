//! A clock read without a system call (`VICTORY_TSC_CLOCK=1`, off by
//! default; LINUXMEM.md stage S0).
//!
//! Under Wine (Valve's tree) `QueryPerformanceCounter` is a system call
//! through Wine's dispatcher every time, which saves and restores the whole
//! extended processor state on the way: every `Instant::now` of ours and
//! every reading of the title's, 6.6 ms a frame over all threads. Here the
//! processor's own counter (`rdtsc`) is read and scaled to the host's ticks:
//!
//! `ticks = base + (tsc - tsc0) * rate`
//!
//! anchored to the host's real counter at the start and again every second
//! by a thread of its own, so it cannot drift from the counter other
//! libraries (d3d12, the driver) still read. An anchoring never makes the
//! clock jump: the new line starts where the old one is and aims at where
//! the real counter will be a second later.
//!
//! Bound where wrappers are bound already: the title's import of
//! `QueryPerformanceCounter` ([`import`]) and the entries of our own import
//! table ([`start`]), so `std::time::Instant` gets it too. Until the first
//! anchoring is done (some 50 ms), and for good if the processor's counter
//! does not run at one rate whatever the core does (`invariant TSC`), the
//! wrapper calls the host.

use std::ffi::CStr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

type Qpc = unsafe extern "system" fn(*mut i64) -> i32;

/// The host's own `QueryPerformanceCounter`.
static REAL: AtomicUsize = AtomicUsize::new(0);

/// The line the clock is on, behind a sequence number (odd while it is
/// being changed; 0: none yet): the counter's value at the anchor, the
/// ticks there, and ticks per count as a 32.32 fixed-point number.
static SEQ: AtomicU64 = AtomicU64::new(0);
static TSC0: AtomicU64 = AtomicU64::new(0);
static BASE: AtomicU64 = AtomicU64::new(0);
static RATE: AtomicU64 = AtomicU64::new(0);

/// (Unused), anchorings, and the largest step an
/// anchoring found between the clock and the host's, in ticks.
pub static SERVED: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];

fn wanted() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("VICTORY_TSC_CLOCK").is_ok_and(|v| v.trim() == "1"))
}

/// The processor's counter, read once everything before it has run
/// (`lfence`): read out of order, ahead of a load still waiting for another
/// core's store, it gave a thread a time before the one that store had
/// published (membench: 1 reading in 750 against a shared latest).
#[inline]
fn tsc() -> u64 {
    // SAFETY: every x86-64 processor has the instructions.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        core::arch::x86_64::_rdtsc()
    }
}

/// Whether the counter runs at one rate in every power state and on every
/// core (CPUID 8000_0007h, EDX bit 8).
fn invariant() -> bool {
    use core::arch::x86_64::__cpuid;
    __cpuid(0x8000_0000).eax >= 0x8000_0007 && __cpuid(0x8000_0007).edx & (1 << 8) != 0
}

fn real() -> Option<Qpc> {
    let at = REAL.load(Ordering::Relaxed);
    // SAFETY: the host's function of that name, found by `find_real`.
    (at != 0).then(|| unsafe { std::mem::transmute::<usize, Qpc>(at) })
}

fn find_real() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    if REAL.load(Ordering::Relaxed) != 0 {
        return true;
    }
    for dll in [c"kernelbase.dll", c"kernel32.dll"] {
        // SAFETY: NUL-terminated names; modules every process has.
        let found = unsafe {
            let module = GetModuleHandleA(dll.as_ptr().cast());
            if module.is_null() { None } else { GetProcAddress(module, c"QueryPerformanceCounter".as_ptr().cast()) }
        };
        if let Some(f) = found {
            REAL.store(f as *const () as usize, Ordering::Relaxed);
            return true;
        }
    }
    false
}

/// The line now, if there is one: (tsc0, base, rate).
#[inline]
fn line() -> Option<(u64, u64, u64)> {
    loop {
        let seq = SEQ.load(Ordering::Acquire);
        if seq == 0 {
            return None;
        }
        if seq & 1 == 0 {
            let line = (TSC0.load(Ordering::Relaxed), BASE.load(Ordering::Relaxed), RATE.load(Ordering::Relaxed));
            if SEQ.load(Ordering::Acquire) == seq {
                return Some(line);
            }
        }
        std::hint::spin_loop();
    }
}

#[inline]
fn on_line((tsc0, base, rate): (u64, u64, u64), tsc: u64) -> u64 {
    // (A core whose counter is a few counts behind the anchor's: the anchor.)
    let counts = tsc.saturating_sub(tsc0);
    base + ((u128::from(counts) * u128::from(rate)) >> 32) as u64
}

/// Puts the clock on a new line from the counter's value now, which `new`
/// is given, with the ticks there and the rate it returns. Readers wait for
/// the few instructions between the reading of the counter and the new
/// line: one that read the old line any later than that could be ahead of
/// what the next reads from the new (the clock would go backwards between
/// two threads).
fn set_line(new: impl FnOnce(u64) -> (u64, u64)) {
    // (One thread only ever writes.)
    let seq = SEQ.load(Ordering::Relaxed);
    SEQ.store(seq + 1, Ordering::SeqCst);
    let at = tsc();
    let (base, rate) = new(at);
    TSC0.store(at, Ordering::Relaxed);
    BASE.store(base, Ordering::Relaxed);
    RATE.store(rate, Ordering::Relaxed);
    SEQ.store(seq + 2, Ordering::SeqCst);
}

/// The ticks now, by the processor's counter; `None` before the first
/// anchoring (or with the clock off).
#[inline]
pub fn ticks() -> Option<u64> {
    let line = line()?;
    Some(on_line(line, tsc()))
}

/// The host's own counter now (a system call under Wine), once [`start`]
/// or [`import`] found it.
pub fn host_ticks() -> Option<u64> {
    let mut ticks = 0i64;
    // SAFETY: a valid out pointer.
    unsafe { real()?(&mut ticks) };
    Some(ticks as u64)
}

/// What `QueryPerformanceCounter` is bound to.
unsafe extern "system" fn counter(out: *mut i64) -> i32 {
    if let Some(ticks) = ticks() {
        // (As the host: a null pointer faults.)
        // SAFETY: the caller's own out pointer.
        unsafe { out.write(ticks as i64) };
        return 1;
    }
    match real() {
        // SAFETY: the caller's own call.
        Some(real) => unsafe { real(out) },
        None => 0,
    }
}

/// The counter and the host's ticks at one moment: the reading of several
/// that the host answered fastest, its middle.
fn pair(real: Qpc) -> (u64, u64) {
    let mut best = (u64::MAX, 0, 0);
    for _ in 0..8 {
        let mut ticks = 0i64;
        let before = tsc();
        // SAFETY: a valid out pointer.
        unsafe { real(&mut ticks) };
        let after = tsc();
        if after.wrapping_sub(before) < best.0 {
            best = (after.wrapping_sub(before), before + after.wrapping_sub(before) / 2, ticks as u64);
        }
    }
    (best.1, best.2)
}

/// Ticks per count (32.32) between two pairs.
fn rate_between(a: (u64, u64), b: (u64, u64)) -> Option<u64> {
    let (counts, ticks) = (b.0.checked_sub(a.0)?, b.1.checked_sub(a.1)?);
    if counts == 0 || ticks == 0 {
        return None;
    }
    u64::try_from((u128::from(ticks) << 32) / u128::from(counts)).ok().filter(|&r| r != 0)
}

/// The anchoring thread: the first line after 50 ms, then one a second.
fn keep(real: Qpc) {
    use std::time::Duration;
    let first = pair(real);
    std::thread::sleep(Duration::from_millis(50));
    let mut last = pair(real);
    let Some(mut true_rate) = rate_between(first, last) else {
        tracing::warn!("VICTORY_TSC_CLOCK=1 but falling back to the host's QueryPerformanceCounter: the processor's counter does not move with it");
        return;
    };
    // (From the counter now, on the line through the second pair.)
    set_line(|at| (last.1 + ((u128::from(at.saturating_sub(last.0)) * u128::from(true_rate)) >> 32) as u64, true_rate));
    tracing::info!(
        counts_per_tick = format_args!("{:.3}", (1u64 << 32) as f64 / true_rate as f64),
        "VICTORY_TSC_CLOCK=1: QueryPerformanceCounter answered from rdtsc, anchored to the host's every second"
    );
    // The rate over a longer and longer stretch (up to 64 s), which is what
    // the host's clock itself is steered by.
    let mut far = first;
    let mut since = 0u32;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = pair(real);
        let Some(line) = line() else { return };
        if now.0 <= last.0 || now.1 < last.1 {
            continue; // (the counter went backwards: no line from that)
        }
        if let Some(rate) = rate_between(far, now) {
            true_rate = rate;
        }
        since += 1;
        if since >= 64 {
            (far, since) = (last, 0);
        }
        // Where the clock is now, and where the host's will be a second on:
        // the new line joins the two, never steeper or flatter than the
        // true rate by more than 1 in 2,000 (a step of 500 us is worked
        // off over a second; more, over as many as it takes).
        let step = on_line(line, now.0) as i64 - now.1 as i64;
        SERVED[1].fetch_add(1, Ordering::Relaxed);
        SERVED[2].fetch_max(step.unsigned_abs(), Ordering::Relaxed);
        let correction = ((i128::from(-step) << 32) / i128::from(now.0 - last.0))
            .clamp(-i128::from(true_rate / 2000), i128::from(true_rate / 2000));
        let rate = (i128::from(true_rate) + correction).max(1) as u64;
        // From where the old line is at that moment: no jump.
        set_line(|at| (on_line(line, at), rate));
        last = now;
    }
}

/// Starts the clock, if asked for and the processor can: finds the host's
/// counter, points our own import table's entries for it at ours, and
/// starts the anchoring thread. Once; later calls do nothing.
pub fn start() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    if !wanted() {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| tracing::info!("VICTORY_TSC_CLOCK: off (unset): the host's QueryPerformanceCounter"));
        return;
    }
    ONCE.call_once(|| {
        if !invariant() {
            tracing::warn!("VICTORY_TSC_CLOCK=1 but falling back to the host's QueryPerformanceCounter: the processor's counter is not invariant");
            return;
        }
        let Some(real) = find_real().then(real).flatten() else {
            tracing::warn!("VICTORY_TSC_CLOCK=1 but falling back to the host's QueryPerformanceCounter: it was not found");
            return;
        };
        // SAFETY: our own module's mapped image.
        let changed = unsafe { wrap_imports() };
        tracing::info!(changed, "VICTORY_TSC_CLOCK=1: our own imports of QueryPerformanceCounter are bound to the clock");
        if std::thread::Builder::new().name("tsc clock".into()).spawn(move || keep(real)).is_err() {
            tracing::warn!("VICTORY_TSC_CLOCK=1: no thread to anchor the clock; the host's counter answers");
        }
    });
}

/// What the title's import `name`, which the host has at `address`, is
/// bound to.
pub fn import(name: &str, address: usize) -> usize {
    if name != "QueryPerformanceCounter" || !wanted() || !invariant() {
        return address;
    }
    if REAL.load(Ordering::Relaxed) == 0 {
        REAL.store(address, Ordering::Relaxed);
    }
    start();
    counter as *const () as usize
}

/// Points the entries of our own import table for the counter at
/// [`counter`]: how many (as `timeline::hooks` does for its wrappers).
///
/// # Safety
/// The process's module is mapped as the loader left it.
unsafe fn wrap_imports() -> usize {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Memory::{PAGE_READWRITE, VirtualProtect};
    // SAFETY: a plain query.
    let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
    let mut changed = 0;
    // SAFETY: the headers, the import directory and the tables it names lie
    // in the mapped image, as the loader read them; an entry is eight bytes,
    // aligned, and made writable for the store.
    unsafe {
        let at = |rva: u32| base + rva as usize;
        let read = |at: usize| (at as *const u32).read_unaligned();
        let headers = at(read(base + 0x3c));
        // The second data directory of the 64-bit optional header: imports.
        let directory = read(headers + 0x18 + 0x70 + 8);
        if directory == 0 {
            return 0;
        }
        let mut descriptor = at(directory);
        loop {
            let (names, first) = (read(descriptor), read(descriptor + 16));
            if first == 0 {
                break;
            }
            let names = if names != 0 { names } else { first };
            for i in 0.. {
                let entry = (at(names) as *const u64).add(i).read_unaligned();
                if entry == 0 {
                    break;
                }
                if entry >> 63 != 0 {
                    continue; // by number
                }
                let name = CStr::from_ptr((at(entry as u32) + 2) as *const _);
                if name.to_bytes() != b"QueryPerformanceCounter" {
                    continue;
                }
                let slot = (at(first) as *mut usize).add(i);
                let (mut old, mut ignored) = (0, 0);
                if VirtualProtect(slot.cast(), 8, PAGE_READWRITE, &mut old) != 0 {
                    (*slot.cast::<AtomicUsize>()).store(counter as *const () as usize, Ordering::SeqCst);
                    VirtualProtect(slot.cast(), 8, old, &mut ignored);
                    changed += 1;
                }
            }
            descriptor += 20;
        }
    }
    changed
}
