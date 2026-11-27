//! Helper shaders of ours compiled by DXC before the build, when one is
//! there ([`bundled`]).
//!
//! vkd3d-shader, the `d3dcompiler_47` Proton provides, defines no `Load4`,
//! `Store` or `Interlocked*` method on byte-address buffers: the card
//! compare (`RWByteAddressBuffer.Load4`), the up check (`InterlockedAdd`) and
//! the argument clamp and batched clamp (`InterlockedOr`) do not compile
//! under Wine, and each feature then falls back: title copies stay in the
//! PC's memory, indirect executions are dropped. DXC compiles the same
//! sources into DXIL, which vkd3d-proton runs like any title shader, and a
//! DXIL container keeps its root signature (`CreateRootSignature` with its
//! bytes works there; a Proton run of the probe said ROOT OK and PSO OK).
//! The sources are compiled by `build.rs` when `VICTORY_BUILD_DXC` names a
//! compiler, or else when `dxc` is on `PATH`.
//!
//! Only Wine takes them: Windows keeps compiling at run time as it always
//! did, so a fork run there cannot change because of this file.

/// Whether we run under Wine (its ntdll exports `wine_get_version`).
pub(crate) fn on_wine() -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    static ON_WINE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON_WINE.get_or_init(|| {
        // SAFETY: plain module and export lookups on a name the system has.
        unsafe {
            let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr().cast());
            !ntdll.is_null() && GetProcAddress(ntdll, c"wine_get_version".as_ptr().cast()).is_some()
        }
    })
}

/// The DXIL of the helper shader `name`, if the build had a compiler.
///
/// Names: `card_compare`, `upcheck`, `clamp_<widest>_<rest>` and
/// `batch_<widest>_<rest>` (the two the argument clamp is made in, each with
/// its `WIDEST` and `REST` the run asked for). `None` for a build without
/// DXC, and for a name this file does not hold.
#[allow(unused_variables)]
pub(crate) fn bundled(name: &str) -> Option<&'static [u8]> {
    #[cfg(victory_dxil)]
    {
        Some(match name {
            "card_compare" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/card_compare.dxil"))[..],
            "upcheck" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/upcheck.dxil"))[..],
            "clamp_65535_0" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/clamp_65535_0.dxil"))[..],
            "clamp_65535_1" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/clamp_65535_1.dxil"))[..],
            "clamp_1048576_0" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/clamp_1048576_0.dxil"))[..],
            "clamp_1048576_1" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/clamp_1048576_1.dxil"))[..],
            "batch_65535_0" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/batch_65535_0.dxil"))[..],
            "batch_65535_1" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/batch_65535_1.dxil"))[..],
            "batch_1048576_0" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/batch_1048576_0.dxil"))[..],
            "batch_1048576_1" => &include_bytes!(concat!(env!("VICTORY_DXIL_DIR"), "/batch_1048576_1.dxil"))[..],
            _ => return None,
        })
    }
    #[cfg(not(victory_dxil))]
    {
        None
    }
}

/// [`bundled`] only under Wine; `None` on Windows, which keeps its run-time
/// compile.
pub(crate) fn bundled_on_wine(name: &str) -> Option<&'static [u8]> {
    if on_wine() { bundled(name) } else { None }
}
