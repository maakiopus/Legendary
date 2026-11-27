//! `membench`: light checks of the Linux memory backend (`victory_mem.so`)
//! and the clock without system calls, outside the game (LINUXMEM.md, test
//! plan: T1 show/unshow, T2 written pages, T5 clock; residency too). Run
//! under Proton's wine in a scratch prefix, with `victory_mem.so` beside the
//! exe or named by `VICTORY_MEM_SO`:
//!
//!     wine membench.exe [t1] [t2] [readable] [t5] [seconds for t5]
//!
//! Every case runs a second or two on one thread. Exit code: failed checks.

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn main() {}

#[cfg(all(windows, target_arch = "x86_64"))]
fn main() {
    imp::main()
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod imp {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
    use std::time::{Duration, Instant};

    use victory_hle::linuxmem::{ShowOp, raw};
    use victory_hle::tscclock;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, FILE_MAP_WRITE, GetWriteWatch, MEM_COMMIT, MEM_PRESERVE_PLACEHOLDER, MEM_RELEASE,
        MEM_REPLACE_PLACEHOLDER, MEM_RESERVE, MEM_RESERVE_PLACEHOLDER, MEMORY_BASIC_INFORMATION,
        MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, MapViewOfFile3, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE,
        SEC_COMMIT, UnmapViewOfFile2, VirtualAlloc, VirtualAlloc2, VirtualFree, VirtualProtect, VirtualQuery,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    const PAGE: usize = 64 << 10;
    const SMALL: usize = 4096;
    const MEM_WRITE_WATCH: u32 = 0x20_0000;

    static FAILED: AtomicU32 = AtomicU32::new(0);

    macro_rules! check {
        ($ok:expr, $($what:tt)*) => {{
            let ok = $ok;
            println!("  {} {}", if ok { "ok  " } else { "FAIL" }, format_args!($($what)*));
            if !ok {
                FAILED.fetch_add(1, Relaxed);
            }
        }};
    }

    fn query(at: usize) -> (u32, u32, u32) {
        // SAFETY: plain data the query fills in.
        unsafe {
            let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
            VirtualQuery(at as *const _, &mut info, size_of_val(&info));
            (info.State, info.Type, info.Protect)
        }
    }

    /// Lines of `/proc/self/maps` (the process's mappings as Linux has them).
    fn mappings() -> usize {
        std::fs::read_to_string(r"Z:\proc\self\maps").map_or(0, |s| s.lines().count())
    }

    fn reserve_placeholder(bytes: usize) -> usize {
        // SAFETY: a new placeholder reservation.
        unsafe {
            VirtualAlloc2(
                GetCurrentProcess(),
                std::ptr::null(),
                bytes,
                MEM_RESERVE | MEM_RESERVE_PLACEHOLDER,
                PAGE_NOACCESS,
                std::ptr::null_mut(),
                0,
            ) as usize
        }
    }

    fn split(at: usize, bytes: usize) {
        // SAFETY: cuts a piece out of a placeholder of ours.
        unsafe { VirtualFree(at as *mut c_void, bytes, MEM_RELEASE | MEM_PRESERVE_PLACEHOLDER) };
    }

    /// As `xmem`'s `show_direct`: a committed reservation in place of the
    /// placeholder, then the section's pages over it.
    fn show_direct(at: usize, from: usize, bytes: usize) -> bool {
        // SAFETY: replaces a placeholder of ours of exactly that size.
        let got = unsafe {
            VirtualAlloc2(
                GetCurrentProcess(),
                at as *const c_void,
                bytes,
                MEM_RESERVE | MEM_COMMIT | MEM_REPLACE_PLACEHOLDER,
                PAGE_READWRITE,
                std::ptr::null_mut(),
                0,
            ) as usize
        };
        got == at && raw::show(&[ShowOp { from, at, bytes }], false) == 1
    }

    fn unshow_direct(at: usize, bytes: usize) -> bool {
        // SAFETY: a reservation of ours, back to a placeholder.
        unsafe { VirtualFree(at as *mut c_void, bytes, MEM_RELEASE | MEM_PRESERVE_PLACEHOLDER) != 0 }
    }

    fn t1() {
        println!("T1 show/unshow (VICTORY_DIRECT_PAGES)");
        const PAGES: usize = 64;
        // SAFETY: a pagefile section and a view of all of it, as `xmem` makes.
        let (section, window) = unsafe {
            let section =
                CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE | SEC_COMMIT, 0, (PAGES * PAGE) as u32, std::ptr::null());
            (section, MapViewOfFile(section, FILE_MAP_WRITE, 0, 0, PAGES * PAGE).Value as usize)
        };
        check!(window != 0, "section of {PAGES} pages and its whole view at {window:#x}");
        const SPOTS: usize = 4096;
        let hole = reserve_placeholder(SPOTS * PAGE);
        check!(hole != 0, "placeholder of {SPOTS} pages at {hole:#x}");
        let at = hole + 5 * PAGE;
        split(at, PAGE);
        let before = query(at);
        check!(show_direct(at, window + 3 * PAGE, PAGE), "page 3 shown at {at:#x} by mremap");
        // SAFETY: the alias and the window are mapped read-write.
        unsafe {
            (at as *mut u64).write_volatile(0x1122_3344_5566_7788);
            check!(((window + 3 * PAGE) as *const u64).read_volatile() == 0x1122_3344_5566_7788, "a write at the alias is in the window");
            ((window + 3 * PAGE + 8) as *mut u64).write_volatile(0xaabb);
            check!(((at + 8) as *const u64).read_volatile() == 0xaabb, "a write in the window is at the alias");
        }
        let shown = query(at);
        println!("       VirtualQuery before (state, type, protection) {before:x?}, shown {shown:x?} (0x20000 private: the lie)");
        check!(shown.0 == MEM_COMMIT && shown.2 == PAGE_READWRITE, "Windows' eye: committed, read-write");
        let mut old = 0;
        // SAFETY: our own page.
        check!(unsafe { VirtualProtect(at as *const _, PAGE, PAGE_READONLY, &mut old) } != 0, "VirtualProtect read-only on the alias");
        // SAFETY: the window stays writable; the alias is read.
        unsafe {
            ((window + 3 * PAGE + 16) as *mut u64).write_volatile(7);
            check!(((at + 16) as *const u64).read_volatile() == 7, "read-only alias still shows the section's pages");
        }
        check!(raw::readable(at, PAGE, true) == Some((16, 16)), "alias readable, 16 of 16 pages");
        check!(unshow_direct(at, PAGE), "unshown (VirtualFree to a placeholder)");
        let gone = query(at);
        check!(gone.0 == MEM_RESERVE, "Windows' eye after: reserved ({gone:x?})");
        check!(raw::readable(at, PAGE, true) == Some((0, 1)), "a read there would fault");
        // SAFETY: the window is still mapped.
        check!(unsafe { ((window + 3 * PAGE) as *const u64).read_volatile() } == 0x1122_3344_5566_7788, "the window keeps the bytes");
        check!(show_direct(at, window + 4 * PAGE, PAGE) && unshow_direct(at, PAGE), "shown and unshown again in the same place");

        // Several pages in one call, and many aliases alive at once.
        let maps_before = mappings();
        for i in 0..SPOTS {
            split(hole + i * PAGE, PAGE);
        }
        let started = Instant::now();
        let mut ok = true;
        for i in 0..SPOTS {
            // (Neighbours show pages that follow one another, and others.)
            ok &= show_direct(hole + i * PAGE, window + (i * 7 % PAGES) * PAGE, PAGE);
        }
        let shown_us = started.elapsed().as_secs_f64() * 1e6 / SPOTS as f64;
        let maps_shown = mappings();
        check!(ok, "{SPOTS} aliases alive at once: {shown_us:.2} us each to show; mappings {maps_before} -> {maps_shown}");
        // SAFETY: both mapped.
        check!(unsafe { ((hole + 9 * PAGE) as *const u64).read_volatile() == ((window + (9 * 7 % PAGES) * PAGE) as *const u64).read_volatile() }, "alias 9 shows its page");
        let started = Instant::now();
        for i in 0..SPOTS {
            ok &= unshow_direct(hole + i * PAGE, PAGE);
        }
        let unshown_us = started.elapsed().as_secs_f64() * 1e6 / SPOTS as f64;
        check!(ok, "all unshown: {unshown_us:.2} us each; mappings now {}", mappings());

        // The same as views of the section, as on Windows (the wineserver's way).
        let started = Instant::now();
        let mut views = 0;
        for i in 0..SPOTS {
            // SAFETY: maps the section over a placeholder of ours of that size.
            let view = unsafe {
                MapViewOfFile3(
                    section,
                    GetCurrentProcess(),
                    (hole + i * PAGE) as *const c_void,
                    ((i * 7 % PAGES) * PAGE) as u64,
                    PAGE,
                    MEM_REPLACE_PLACEHOLDER,
                    PAGE_READWRITE,
                    std::ptr::null_mut(),
                    0,
                )
            };
            views += usize::from(view.Value as usize == hole + i * PAGE);
        }
        let mapped_us = started.elapsed().as_secs_f64() * 1e6 / SPOTS as f64;
        let started = Instant::now();
        for i in 0..SPOTS {
            // SAFETY: a view of ours, back to a placeholder.
            unsafe { UnmapViewOfFile2(GetCurrentProcess(), MEMORY_MAPPED_VIEW_ADDRESS { Value: (hole + i * PAGE) as *mut c_void }, MEM_PRESERVE_PLACEHOLDER) };
        }
        let unmapped_us = started.elapsed().as_secs_f64() * 1e6 / SPOTS as f64;
        check!(views == SPOTS, "{SPOTS} views of the section instead: {mapped_us:.2} us each to map, {unmapped_us:.2} to unmap");
        println!("       pair: direct {:.2} us, views {:.2} us", shown_us + unshown_us, mapped_us + unmapped_us);

        // One call for a batch.
        let ops: Vec<ShowOp> = (0..64).map(|i| ShowOp { from: window + i * PAGE, at: hole + i * PAGE, bytes: PAGE }).collect();
        let mut reserved = true;
        for op in &ops {
            // SAFETY: replaces a placeholder of ours of exactly that size.
            reserved &= unsafe {
                VirtualAlloc2(GetCurrentProcess(), op.at as *const c_void, PAGE, MEM_RESERVE | MEM_COMMIT | MEM_REPLACE_PLACEHOLDER, PAGE_READWRITE, std::ptr::null_mut(), 0)
            } as usize
                == op.at;
        }
        let started = Instant::now();
        let done = raw::show(&ops, true);
        let batch_us = started.elapsed().as_secs_f64() * 1e6;
        check!(reserved && done == ops.len(), "64 pages shown by one call, faulted in: {batch_us:.1} us in all");
        // SAFETY: both mapped.
        check!(unsafe { ((hole + 3 * PAGE) as *const u64).read_volatile() } == 0x1122_3344_5566_7788, "page 3 at its new place");
        let mut present = Vec::new();
        check!(raw::present(hole, 64 * PAGE, &mut present) && present == [(hole, hole + 64 * PAGE)], "all 64 present after the populating call ({} runs)", present.len());
        // A bad op stops the batch and says where.
        let bad = [ops[0], ShowOp { from: 0x1000, at: hole + PAGE, bytes: PAGE }, ops[2]];
        check!(raw::show(&bad, false) == 1, "a batch stops at the op that fails");
        for op in &ops {
            unshow_direct(op.at, PAGE);
        }
    }

    fn get_write_watch(base: usize, len: usize, reset: bool) -> Option<Vec<usize>> {
        let mut found = vec![0usize; len / SMALL];
        let mut count = found.len();
        let mut granularity = 0;
        // SAFETY: the answer buffer holds `count` entries.
        let failed = unsafe { GetWriteWatch(u32::from(reset), base as *const _, len, found.as_mut_ptr().cast(), &mut count, &mut granularity) } != 0;
        found.truncate(count);
        (!failed).then_some(found)
    }

    fn native(base: usize, len: usize, reset: bool) -> Option<Vec<usize>> {
        let mut runs = Vec::new();
        raw::written(base, len, reset, true, &mut runs)
            .then(|| runs.iter().flat_map(|&(a, b)| (a.max(base)..b.min(base + len)).step_by(SMALL)).collect())
    }

    fn t2() {
        println!("T2 written pages of a write-watched reservation (VICTORY_NATIVE_SCAN)");
        const LEN: usize = 8 << 20;
        // SAFETY: new allocations of ours.
        let (base, plain) = unsafe {
            (
                VirtualAlloc(std::ptr::null(), LEN, MEM_RESERVE | MEM_COMMIT | MEM_WRITE_WATCH, PAGE_READWRITE) as usize,
                VirtualAlloc(std::ptr::null(), LEN, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE) as usize,
            )
        };
        check!(base != 0 && plain != 0, "8 MiB write-watched at {base:#x}, 8 MiB plain at {plain:#x}");
        let touch = |pages: &[usize]| {
            for &p in pages {
                // SAFETY: inside the committed range.
                unsafe { ((base + p * SMALL + 5) as *mut u8).write_volatile(1) };
            }
        };
        let addresses = |pages: &[usize]| pages.iter().map(|p| base + p * SMALL).collect::<Vec<_>>();
        check!(native(base, LEN, false) == Some(vec![]) && get_write_watch(base, LEN, false) == Some(vec![]), "fresh: nothing written, both agree");
        let first = [0, 1, 2, 17, 255, 256, 1000, 2047];
        touch(&first);
        let (n, w) = (native(base, LEN, false), get_write_watch(base, LEN, false));
        check!(n == w && n == Some(addresses(&first)), "8 pages written: native and GetWriteWatch agree page for page, without reset");
        check!(native(base + 16 * SMALL, 300 * SMALL, false) == Some(addresses(&[17, 255, 256])), "a part of the range: its pages only");
        check!(native(base, LEN, true) == Some(addresses(&first)), "native with reset tells them");
        check!(get_write_watch(base, LEN, false) == Some(vec![]) && native(base, LEN, false) == Some(vec![]), "after the native reset GetWriteWatch finds nothing");
        let second = [3, 4, 1999];
        touch(&second);
        check!(get_write_watch(base, LEN, true) == Some(addresses(&second)), "written again: GetWriteWatch with reset tells the new ones");
        check!(native(base, LEN, true) == Some(vec![]), "after its reset the native asking finds nothing");
        // More runs than one call's room (256): every other page of 600.
        let many: Vec<usize> = (0..600).map(|i| i * 2).collect();
        touch(&many);
        check!(native(base, LEN, true) == Some(addresses(&many)), "600 separate pages (more than one call holds) all told");
        check!(native(base, LEN, false) == Some(vec![]), "and all reset");
        // Memory that is not write-watched is refused, and nothing reset.
        check!(native(plain, LEN, true).is_none() && get_write_watch(plain, LEN, true).is_none(), "plain memory: both refuse");
        touch(&first);
        let mixed_refused = if plain == base + LEN {
            native(base, 2 * LEN, true).is_none()
        } else if base == plain + LEN {
            native(plain, 2 * LEN, true).is_none()
        } else {
            // Not neighbours: a reservation of 16 MiB, half of it replaced.
            println!("       (the two are not neighbours: the mixed range is skipped)");
            true
        };
        check!(mixed_refused && native(base, LEN, false) == Some(addresses(&first)), "a range half write-watched is refused and resets nothing");
        native(base, LEN, true);

        // Cost of an asking with reset, 16 pages written before each.
        const ROUNDS: usize = 2000;
        let hot: Vec<usize> = (0..16).map(|i| i * 100).collect();
        let mut spent = [Duration::ZERO; 2];
        let mut same = true;
        for round in 0..ROUNDS {
            touch(&hot);
            let started = Instant::now();
            let got = if round % 2 == 0 { native(base, LEN, true) } else { get_write_watch(base, LEN, true) };
            spent[round % 2] += started.elapsed();
            same &= got.is_some_and(|g| g.len() == hot.len());
        }
        let us = spent.map(|d| d.as_secs_f64() * 1e6 / (ROUNDS / 2) as f64);
        check!(same, "{ROUNDS} askings with reset over 8 MiB, 16 pages written: native {:.2} us, GetWriteWatch {:.2} us each", us[0], us[1]);
        let small = 64 << 10;
        let mut spent = [Duration::ZERO; 2];
        for round in 0..ROUNDS {
            touch(&[1]);
            let started = Instant::now();
            let got = if round % 2 == 0 { native(base, small, true) } else { get_write_watch(base, small, true) };
            spent[round % 2] += started.elapsed();
            same &= got.is_some_and(|g| g.len() == 1);
        }
        let us = spent.map(|d| d.as_secs_f64() * 1e6 / (ROUNDS / 2) as f64);
        check!(same, "the same over 64 KiB, 1 page written: native {:.2} us, GetWriteWatch {:.2} us each", us[0], us[1]);
    }

    fn readable() {
        println!("Readable pages (VICTORY_NATIVE_PRESENT)");
        const LEN: usize = 1 << 20;
        // SAFETY: new allocations of ours.
        let (base, reserved) = unsafe {
            (
                VirtualAlloc(std::ptr::null(), LEN, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE) as usize,
                VirtualAlloc(std::ptr::null(), LEN, MEM_RESERVE, PAGE_NOACCESS) as usize,
            )
        };
        let pages = LEN / SMALL;
        check!(raw::readable(base, LEN, true) == Some((pages, pages)), "committed, never touched: all {pages} pages readable");
        check!(raw::readable(base + 100, 1, true) == Some((1, 1)) && raw::readable(base + SMALL - 1, 2, true) == Some((2, 2)), "a byte: its page; two bytes over a boundary: two pages");
        check!(raw::readable(reserved, LEN, true) == Some((0, 1)) && raw::readable(reserved, LEN, false) == Some((0, pages)), "reserved only: none (stopping at the first, and not)");
        let mut old = 0;
        // SAFETY: our own pages.
        unsafe { VirtualProtect((base + 10 * SMALL) as *const _, 2 * SMALL, PAGE_NOACCESS, &mut old) };
        check!(raw::readable(base, LEN, false) == Some((pages - 2, pages)), "two pages made no-access (though present) are not readable");
        check!(raw::readable(base, LEN, true) == Some((10, 11)), "and stop the asking that wants all");
        // SAFETY: our own pages.
        unsafe { VirtualProtect((base + 20 * SMALL) as *const _, SMALL, PAGE_READONLY, &mut old) };
        check!(raw::readable(base + 20 * SMALL, SMALL, true) == Some((1, 1)), "a read-only page is readable");
        let mut present = Vec::new();
        let asked = raw::present(base, LEN, &mut present);
        check!(asked && !present.is_empty(), "present pages asked of the kernel: {} runs, first {:x?}", present.len(), present.first());
        const ROUNDS: usize = 20_000;
        let started = Instant::now();
        for i in 0..ROUNDS {
            std::hint::black_box(raw::readable(base + 32 * SMALL + (i % 64) * SMALL, 1, true));
        }
        let one = started.elapsed().as_secs_f64() * 1e9 / ROUNDS as f64;
        let started = Instant::now();
        for _ in 0..ROUNDS / 10 {
            std::hint::black_box(raw::readable(base + 32 * SMALL, 64 << 10, true));
        }
        let block = started.elapsed().as_secs_f64() * 1e9 / (ROUNDS / 10) as f64;
        println!("       one page {one:.0} ns, a 64 KiB block {block:.0} ns an asking");
    }

    fn t5(seconds: u64) {
        println!("T5 clock from rdtsc (VICTORY_TSC_CLOCK), {seconds} s");
        // Before: every reading is the host's.
        const READS: u32 = 1_000_000;
        let per_read = || {
            let started = Instant::now();
            for _ in 0..READS {
                std::hint::black_box(Instant::now());
            }
            started.elapsed().as_secs_f64() * 1e9 / f64::from(READS)
        };
        let host_ns = per_read();
        // SAFETY: before any other thread of ours runs.
        unsafe { std::env::set_var("VICTORY_TSC_CLOCK", "1") };
        tscclock::start();
        let began = Instant::now();
        while tscclock::ticks().is_none() && began.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(5));
        }
        check!(tscclock::ticks().is_some(), "the clock is anchored after {:.0} ms", began.elapsed().as_secs_f64() * 1e3);
        if tscclock::ticks().is_none() {
            return;
        }
        let ours_ns = per_read();
        check!(ours_ns < host_ns, "Instant::now: {host_ns:.1} ns by the host, {ours_ns:.1} ns by the clock (our own imports are bound)");
        // Monotonic, and close to the host's, for the seconds asked.
        let (mut last, mut backwards, mut reads) = (0u64, 0u64, 0u64);
        let (mut worst, mut first_gap, mut last_gap) = (0i64, None, 0i64);
        let until = Instant::now() + Duration::from_secs(seconds);
        let mut next_look = Instant::now();
        let mut gaps: Vec<i64> = Vec::new();
        loop {
            // A burst of readings, then a sleep: light on the machine.
            for _ in 0..20_000 {
                let now = tscclock::ticks().unwrap_or(0);
                backwards += u64::from(now < last);
                last = now;
                reads += 1;
            }
            let now = Instant::now();
            if now >= next_look {
                next_look = now + Duration::from_millis(100);
                // The host's reading between two of ours.
                // (The tightest of a few: a thread taken off the core in
                // between says nothing of the clock.)
                let (a, host, b) = (0..5)
                    .map(|_| (tscclock::ticks().unwrap_or(0), tscclock::host_ticks().unwrap_or(0), tscclock::ticks().unwrap_or(0)))
                    .min_by_key(|&(a, _, b)| b.saturating_sub(a))
                    .unwrap_or((0, 0, 0));
                let gap = (a + b.saturating_sub(a) / 2) as i64 - host as i64;
                first_gap.get_or_insert(gap);
                last_gap = gap;
                gaps.push(gap);
                worst = worst.max(gap.abs());
            }
            if now >= until {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        check!(backwards == 0, "{reads} readings on one thread, none backwards");
        // Ticks are 100 ns under Wine; the host's reading is cut to a whole one.
        let settled = &gaps[gaps.len() / 2..];
        let (lo, hi) = (settled.iter().min().copied().unwrap_or(0), settled.iter().max().copied().unwrap_or(0));
        check!(worst <= 50, "against the host's counter: at most {worst} ticks apart (first {}, last {last_gap}; second half between {lo} and {hi}); 1 tick = 100 ns", first_gap.unwrap_or(0));
        check!(hi - lo <= 20, "drift in the second half: {} ticks over {:.1} s", hi - lo, seconds as f64 / 2.0);
        let [_, anchored, step] = [0, 1, 2].map(|i| tscclock::SERVED[i].load(Relaxed));
        println!("       anchored {anchored} times; the largest step an anchoring found: {step} ticks");
        // Several threads never see it go backwards either: a reading taken
        // after another thread's was published is not before it. In bursts
        // over 2.5 s, so that anchorings fall among them.
        let threads: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    static SEEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                    let (mut backwards, mut reads) = (0u64, 0u64);
                    let until = Instant::now() + Duration::from_millis(2500);
                    while Instant::now() < until {
                        for _ in 0..20_000 {
                            let published = SEEN.load(std::sync::atomic::Ordering::SeqCst);
                            let now = tscclock::ticks().unwrap_or(0);
                            backwards += u64::from(now < published);
                            SEEN.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
                            reads += 1;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    (backwards, reads)
                })
            })
            .collect();
        let (backwards, reads) = threads.into_iter().map(|t| t.join().unwrap_or((1, 0))).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        check!(backwards == 0, "4 threads, {reads} readings against one shared latest: {backwards} backwards");
    }

    pub(super) fn main() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let wants = |case: &str| !args.iter().any(|a| a.starts_with('t') || a == "readable") || args.iter().any(|a| a == case);
        let seconds = args.iter().find_map(|a| a.parse().ok()).unwrap_or(4);
        match raw::caps() {
            Some(caps) => println!("victory_mem.so loaded: {caps:?}"),
            None => println!("victory_mem.so NOT loaded (not under Wine, or not beside the exe / in VICTORY_MEM_SO): memory cases are skipped"),
        }
        if raw::caps().is_some() {
            if wants("t1") {
                t1();
            }
            if wants("t2") {
                t2();
            }
            if wants("readable") {
                readable();
            }
        }
        if wants("t5") {
            t5(seconds);
        }
        let failed = FAILED.load(Relaxed);
        println!("{}", if failed == 0 { "all checks passed".to_string() } else { format!("{failed} checks FAILED") });
        std::process::exit(failed as i32);
    }
}
