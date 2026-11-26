//! The Linux side of `xmem`'s page mappings under Wine (`direct` there
//! loads it): one call that shows pages of a shared mapping at another
//! address. Wine keeps its own account of the address (a committed
//! reservation the runtime made there); only what it shows changes.
//! Built by `build.sh` beside this file; needs no library at all.
#![no_std]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[repr(C)]
pub struct Show {
    /// Where the pages are mapped already (a shared mapping).
    from: usize,
    bytes: usize,
    /// Where to show them too; what is mapped there goes.
    at: usize,
    /// `at`, or a negative error number.
    result: isize,
}

const MREMAP: usize = 25;
const MAYMOVE_FIXED: usize = 1 | 2;

/// `mremap` with an old size of 0: a second mapping of the same shared
/// pages, made in one step in place of what is at `at`.
unsafe extern "C" fn show(args: *mut Show) -> u32 {
    // SAFETY: the caller passes a `Show`; the call replaces the mapping at
    // `at`, which the caller owns.
    unsafe {
        let a = &mut *args;
        let result: isize;
        core::arch::asm!(
            "syscall",
            inlateout("rax") MREMAP as isize => result,
            in("rdi") a.from,
            in("rsi") 0usize,
            in("rdx") a.bytes,
            in("r10") MAYMOVE_FIXED,
            in("r8") a.at,
            out("rcx") _,
            out("r11") _,
            options(nostack)
        );
        a.result = result;
    }
    0
}

/// Wine's table of a unix library's calls.
#[unsafe(no_mangle)]
pub static __wine_unix_call_funcs: [unsafe extern "C" fn(*mut Show) -> u32; 1] = [show];
