//! Fingerprints the shader translator, so that cached translations
//! (`src/shader_cache.rs`) last until the translator itself changes and not
//! only until the next build of anything.

use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn main() {
    let here = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let crates = here.join("..");
    let root = crates.join("..");
    // The translator, what it is built from, and the form entries are kept in.
    let mut all = vec![
        crates.join("victory-shader/Cargo.toml"),
        here.join("src/shader_cache.rs"),
        root.join("Cargo.lock"),
        root.join("rust-toolchain.toml"),
    ];
    let source = crates.join("victory-shader/src");
    println!("cargo:rerun-if-changed={}", source.display());
    files(&source, &mut all);
    all.sort();
    let mut hash = blake3::Hasher::new();
    for path in &all {
        println!("cargo:rerun-if-changed={}", path.display());
        // Names and line endings as they are in every checkout.
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let bytes: Vec<u8> = std::fs::read(path).unwrap_or_default().into_iter().filter(|&b| b != 13).collect();
        hash.update(name.as_bytes());
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-env=VICTORY_TRANSLATOR={}", hash.finalize().to_hex());
    bundle_shaders();
}

/// One helper shader of ours the build compiles into DXIL.
struct Bundled {
    /// The name `crate::dxil::bundled` looks it up by.
    name: &'static str,
    source: &'static str,
    entry: &'static str,
    profile: &'static str,
    /// `#define`s the run-time path writes above the same source.
    defines: &'static [&'static str],
}

/// Every shader `crate::dxil` can hand the run, and the names the run asks
/// for. The variants of the argument clamp are made from the `WIDEST` and
/// `REST` the run would have defined (`crate::indirect`), so a name is
/// `<kind>_<widest>_<rest>`.
fn bundled_shaders() -> Vec<Bundled> {
    let mut all = vec![
        Bundled {
            name: "card_compare",
            source: "card_compare.hlsl",
            entry: "main",
            profile: "cs_6_0",
            defines: &[],
        },
        Bundled { name: "upcheck", source: "upcheck.hlsl", entry: "main", profile: "cs_6_0", defines: &[] },
        // The two-plane composer (Series S presents a scene and an interface
        // plane): vkd3d-shader refuses its source, so the composer was never
        // made under Wine and the second plane was lost.
        Bundled { name: "compose_vs", source: "compose.hlsl", entry: "vs", profile: "vs_6_0", defines: &[] },
        Bundled { name: "compose_ps", source: "compose.hlsl", entry: "ps", profile: "ps_6_0", defines: &[] },
    ];
    const WIDEST: [(&str, &str); 2] = [("65535", "65535u"), ("1048576", "1048576u")];
    const REST: [&str; 2] = ["0", "1"];
    // The names are built from the constants above; `crate::dxil` lists the
    // same ten, and its names must match or the build fails to find them.
    let clamp = [
        ("clamp_", "indirect_clamp.hlsl"),
        ("batch_", "indirect_batch.hlsl"),
    ];
    for (prefix, source) in clamp {
        for (widest_name, widest) in WIDEST {
            for rest in REST {
                let name: &'static str = Box::leak(format!("{prefix}{widest_name}_{rest}").into_boxed_str());
                let defines: &'static [&'static str] = Box::leak(
                    vec![
                        Box::leak(format!("WIDEST={widest}").into_boxed_str()) as &'static str,
                        Box::leak(format!("REST={rest}").into_boxed_str()) as &'static str,
                    ]
                    .into_boxed_slice(),
                );
                all.push(Bundled { name, source, entry: "main", profile: "cs_6_0", defines });
            }
        }
    }
    all
}

/// A DXC that can compile for this build, if there is one.
fn dxc() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("VICTORY_BUILD_DXC") {
        return (!named.is_empty()).then(|| PathBuf::from(named));
    }
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(if cfg!(windows) { "dxc.exe" } else { "dxc" }))
            .find(|candidate| candidate.is_file())
    })
}

/// Compiles every helper shader of ours into `$OUT_DIR`, and tells the crate
/// it may take them ([`dxil`]). Without a compiler nothing is bundled and
/// every shader is compiled at run time, as Windows always has.
fn bundle_shaders() {
    println!("cargo:rerun-if-env-changed=VICTORY_BUILD_DXC");
    println!("cargo:rustc-check-cfg=cfg(victory_dxil)");
    let Some(dxc) = dxc() else {
        println!(
            "cargo:warning=no dxc: helper shaders are compiled at run time; Wine's vkd3d-shader \
             cannot compile the byte-address ones (card compare, up check, argument clamps)"
        );
        return;
    };
    let here = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let shaders = here.join("shaders");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    for shader in bundled_shaders() {
        let source = shaders.join(shader.source);
        println!("cargo:rerun-if-changed={}", source.display());
        let dxil = out.join(format!("{}.dxil", shader.name));
        let mut command = std::process::Command::new(&dxc);
        command.args(["-T", shader.profile, "-E", shader.entry, "-O3", "-Fo"]).arg(&dxil);
        for define in shader.defines {
            command.arg(format!("-D{define}"));
        }
        command.arg(&source);
        let output = command.output().unwrap_or_else(|e| panic!("could not start {}: {e}", dxc.display()));
        assert!(
            output.status.success(),
            "dxc failed for {}: {}",
            shader.source,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    println!("cargo:rustc-env=VICTORY_DXIL_DIR={}", out.display());
    println!("cargo:rustc-cfg=victory_dxil");
    println!("cargo:warning=helper shaders bundled as DXIL with {}", dxc.display());
}
