//! Whether the layer runs under Wine.
//!
//! Upstream keeps this check beside the helper shaders DXC precompiles
//! (`b52f4e3`, `dxil.rs`); this tree compiles the helpers at run time
//! against a native `d3dcompiler_47.dll`, so only the Wine check is
//! imported here. The callers (texture refresh, remap waits) need it as
//! `crate::dxil::on_wine()`.

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
