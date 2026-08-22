//! Regenerating Windows import libraries from the Lean DLLs' live export
//! tables (W-356).
//!
//! Official Lean Windows dists bundle import libraries (`<stem>.dll.a` under
//! `lib/lean/`) that can lag the DLL export tables shipped in `bin/`: e.g.
//! in Lean 4.33.0 `l_Lean_Elab_Tactic_tacticElabAttribute` is exported by
//! `libleanshared_1.dll`, but the bundled import-lib chain does not provide
//! it, so the MSVC link fails with LNK2019.
//!
//! This module parses each Lean DLL's PE export table, emits a `.def` file,
//! and runs the rustc-bundled `rust-lld` (`-flavor link -out:<lib>
//! -def:<def>`) to produce an MS-format import library that both `link.exe`
//! and `lld-link` accept. The generated libs replace the bundled ones, so
//! the link always tracks the DLLs the runtime loads.

use crate::impl_::LeanConfig;
use std::collections::HashMap;
use std::path::PathBuf;

/// DLL base names, in link order (matching the historical emit order).
const LEAN_DLL_STEMS: [&str; 4] = [
    "libInit_shared",
    "libleanshared_2",
    "libleanshared_1",
    "libleanshared",
];

/// A symbol a Lean DLL makes available to importers.
#[derive(Debug, Clone)]
struct Export {
    /// Exported name as it appears in the DLL's export table.
    name: String,
    /// `Some((target_dll, target_name))` for a forwarded export; the
    /// importer must import from `target_dll` directly.
    forwarder: Option<(String, String)>,
}

/// Result of regenerating the import libs: the link-search directory holding
/// the generated `.lib` files, one entry per DLL actually generated.
pub struct GeneratedImportLibs {
    pub search_dir: PathBuf,
    /// `(dll stem, generated import-lib file name)`, in link order.
    pub libs: Vec<(String, String)>,
}

/// Locate the Lean DLLs and regenerate MS import libraries from their
/// export tables. Returns `None` when no usable DLL set or tool is present
/// (callers fall back to the dist-bundled import libs).
pub fn regenerate(config: &LeanConfig) -> Option<GeneratedImportLibs> {
    let lld = rust_lld_path()?;
    let cache_root = cache_dir()?;

    // Locate each DLL: official dists and elan toolchains keep them in
    // `bin/`, some installers in the lib dir.
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for stem in LEAN_DLL_STEMS {
        let dll_name = format!("{stem}.dll");
        let bin_dir = config.lean_home.join("bin");
        let path = [bin_dir, config.lean_lib_dir.clone()]
            .into_iter()
            .map(|dir| dir.join(&dll_name))
            .find(|p| p.is_file());
        if let Some(path) = path {
            found.push((dll_name, path));
        }
    }
    // `libleanshared.dll` is mandatory; the split-DLL companions are optional.
    if !found.iter().any(|(name, _)| name == "libleanshared.dll") {
        return None;
    }

    // Global symbol dedupe: the first DLL in link order that provides a
    // symbol owns its import directive, so a symbol exported by both
    // `libleanshared_1.dll` and (forwarded) `libleanshared.dll` is imported
    // from the real provider.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut per_dll: HashMap<String, Vec<Export>> = HashMap::new();
    for (dll_name, path) in &found {
        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(_) => continue,
        };
        let exports = match parse_pe_exports(&data) {
            Ok(exports) => exports,
            Err(_) => continue,
        };
        let kept: Vec<Export> = exports
            .into_iter()
            .filter(|export| seen.insert(export.name.clone()))
            .collect();
        per_dll.insert(dll_name.clone(), kept);
    }

    // Cache key: identity of every input DLL (path + size + mtime).
    let mut key = String::new();
    for (_, path) in &found {
        if let Ok(meta) = path.metadata() {
            let mtime = meta
                .modified()
                .map(|t| {
                    t.duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            key.push_str(&format!("{:?}:{}:{};", path, meta.len(), mtime));
        } else {
            return None;
        }
    }
    let hash = fnv1a_64(key.as_bytes());
    let search_dir = cache_root.join(format!("win-imports-{hash:016x}"));
    if std::fs::create_dir_all(&search_dir).is_err() {
        return None;
    }

    let mut libs = Vec::new();
    for (dll_name, exports) in &per_dll {
        if exports.is_empty() {
            continue;
        }
        let def_name = format!("{}.def", dll_name);
        let lib_name = format!("{}.lib", dll_name);
        let def_path = search_dir.join(&def_name);
        let lib_path = search_dir.join(&lib_name);
        if lib_path.is_file() {
            libs.push((dll_name.clone(), lib_name));
            continue;
        }
        let def = def_file_for(dll_name, exports);
        if std::fs::write(&def_path, def).is_err() {
            return None;
        }
        let status = std::process::Command::new(&lld)
            .arg("-flavor")
            .arg("link")
            .arg(format!("-out:{}", lib_path.to_string_lossy()))
            .arg(format!("-def:{}", def_path.to_string_lossy()))
            .output();
        match status {
            Ok(output) if output.status.success() => {}
            _ => {
                crate::errors::cargo_warn!(
                    "leo3: failed to generate a Windows import library from \
                     {dll_name} with the rustc-bundled LLD; falling back to \
                     the dist-bundled import libraries"
                );
                return None;
            }
        }
        libs.push((dll_name.clone(), lib_name));
    }
    // Keep link order.
    libs.sort_by_key(|(dll_name, _)| {
        LEAN_DLL_STEMS
            .iter()
            .position(|s| *s == dll_name.trim_end_matches(".dll"))
            .unwrap_or(usize::MAX)
    });
    Some(GeneratedImportLibs { search_dir, libs })
}

/// Location of the rustc-bundled `rust-lld` (present in every rustup
/// toolchain's host sysroot since Rust 1.60).
fn rust_lld_path() -> Option<PathBuf> {
    let rustc = std::env::var_os("RUSTC")?;
    let output = std::process::Command::new(&rustc)
        .arg("--print")
        .arg("sysroot")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sysroot = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let host = std::env::var("HOST").unwrap_or_default();
    let exe = if host.ends_with("-pc-windows-msvc") {
        "rust-lld.exe"
    } else {
        "rust-lld"
    };
    let mut bin = sysroot.join("lib").join("rustlib").join(host).join("bin");
    bin.push(exe);
    bin.exists().then_some(bin)
}

/// User-level cache shared across crates and target dirs, so the (potentially
/// large) generated import libs are produced once per Lean dist.
fn cache_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")?.into()
    } else {
        let home = std::env::var_os("HOME")?;
        PathBuf::from(home).join(".cache")
    };
    Some(base.join("leo3"))
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// `LIBRARY`/`EXPORTS` def-file body; forwarded exports become
/// `name = "target.dll:target_name"`.
fn def_file_for(dll_name: &str, exports: &[Export]) -> String {
    let mut out = String::from("LIBRARY ");
    out.push_str(dll_name);
    out.push_str("\nEXPORTS\n");
    for export in exports {
        match &export.forwarder {
            Some((target_dll, target_name)) => {
                out.push_str(&format!(
                    "{} = \"{}:{}\"\n",
                    export.name, target_dll, target_name
                ));
            }
            None => {
                out.push_str(&export.name);
                out.push('\n');
            }
        }
    }
    out
}

/// A real Windows forwarder string is `<dllname>.dll:<symbol>` — short,
/// pure ASCII. Code bytes that happen to contain `:` and `.` are not
/// forwarders.
fn parse_forwarder(candidate: &str) -> Option<(String, String)> {
    let (dll_part, sym_part) = candidate.split_once(':')?;
    if !dll_part.to_ascii_lowercase().ends_with(".dll") {
        return None;
    }
    if dll_part
        .bytes()
        .any(|b| !b.is_ascii_alphanumeric() && !matches!(b, b'.' | b'_' | b'-'))
    {
        return None;
    }
    if sym_part.is_empty() || sym_part.bytes().any(|b| !b.is_ascii_graphic()) {
        return None;
    }
    Some((dll_part.to_string(), sym_part.to_string()))
}

/// Parse the PE export directory of a Windows DLL.
///
/// Handles PE32 and PE32+; a non-zero function RVA that points at a
/// `dll:symbol` string is treated as a forwarded export.
fn parse_pe_exports(data: &[u8]) -> Result<Vec<Export>, String> {
    let u16_at = |off: usize| -> Result<u16, String> {
        let chunk = data
            .get(off..off.checked_add(2).ok_or("offset overflow")?)
            .ok_or_else(|| format!("truncated PE at {off:#x}"))?;
        chunk
            .try_into()
            .map(u16::from_le_bytes)
            .map_err(|_| "truncated PE".into())
    };
    let u32_at = |off: usize| -> Result<u32, String> {
        let chunk = data
            .get(off..off.checked_add(4).ok_or("offset overflow")?)
            .ok_or_else(|| format!("truncated PE at {off:#x}"))?;
        chunk
            .try_into()
            .map(u32::from_le_bytes)
            .map_err(|_| "truncated PE".into())
    };

    let e_lfanew = u32_at(0x3c)? as usize;
    let sig = data
        .get(e_lfanew..e_lfanew.checked_add(4).ok_or("offset overflow")?)
        .ok_or("truncated PE")?;
    if sig != *b"PE\0\0" {
        return Err("not a PE file".into());
    }
    let file_header = e_lfanew + 4;
    let machine = u16_at(file_header)?;
    if machine != 0x8664 && machine != 0x014c {
        return Err(format!("unsupported PE machine {machine:#06x}"));
    }
    let nsections = u16_at(file_header + 2)? as usize;
    let opt_size = u16_at(file_header + 16)? as usize;
    let opt = file_header + 20;
    let magic = u16_at(opt)?;
    // Data directories: PE32 at +96, PE32+ at +112 (after the common fields).
    let dd_offset = match magic {
        0x20b => 112usize,
        0x10b => 96,
        _ => return Err(format!("unsupported PE optional-header magic {magic:#06x}")),
    };
    let export_rva = u32_at(opt + dd_offset)? as usize;
    if export_rva == 0 {
        return Ok(Vec::new());
    }
    let sections = opt + opt_size;
    let rva_to_off = |rva: usize| -> Result<usize, String> {
        for i in 0..nsections {
            let s = sections + i * 40;
            let vsize = u32_at(s + 8)? as usize;
            let vaddr = u32_at(s + 12)? as usize;
            let rawsize = u32_at(s + 16)? as usize;
            let rawptr = u32_at(s + 20)? as usize;
            if rva >= vaddr && rva < vaddr + vsize.max(rawsize) {
                return Ok(rawptr + (rva - vaddr));
            }
        }
        Err(format!("RVA {rva:#x} not in any section"))
    };
    let cstr = |off: usize| -> String {
        let mut end = off;
        while end < data.len() && data[end] != 0 {
            end += 1;
        }
        String::from_utf8_lossy(&data[off..end]).into_owned()
    };

    let exp = rva_to_off(export_rva)?;
    u32_at(exp + 12)?; // export name RVA (not needed)
    let num_funcs = u32_at(exp + 20)? as usize;
    let num_names = u32_at(exp + 24)? as usize;
    let funcs_off = rva_to_off(u32_at(exp + 28)? as usize)?;
    let names_off = rva_to_off(u32_at(exp + 32)? as usize)?;
    let ords_off = rva_to_off(u32_at(exp + 36)? as usize)?;

    let mut out = Vec::new();
    for i in 0..num_names {
        let name_off = match rva_to_off(u32_at(names_off + i * 4)? as usize) {
            Ok(off) => off,
            Err(_) => break,
        };
        let name = cstr(name_off);
        if name.is_empty() {
            continue;
        }
        let ord = match u16_at(ords_off + i * 2) {
            Ok(ord) => ord as usize,
            Err(_) => break,
        };
        if ord >= num_funcs {
            continue;
        }
        let func_rva = match u32_at(funcs_off + ord * 4) {
            Ok(v) => v as usize,
            Err(_) => break,
        };
        // Forwarded export: the "function address" is an RVA pointing at a
        // `dll:symbol` string in a non-code section.
        let forwarder = if func_rva != 0 {
            rva_to_off(func_rva)
                .ok()
                .and_then(|off| {
                    let mut end = off;
                    while end < data.len() && end - off < 256 && data[end] != 0 {
                        end += 1;
                    }
                    if end >= data.len() || end - off >= 256 || data[end] != 0 {
                        return None;
                    }
                    let candidate = std::str::from_utf8(&data[off..end]).ok()?;
                    parse_forwarder(candidate)
                })
                .filter(|(dll, _)| dll != &name)
        } else {
            None
        };
        out.push(Export { name, forwarder });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal PE32+ DLL image whose export table lists `names`
    /// (plain exports, no forwarders) — enough for `parse_pe_exports`.
    fn minimal_pe(names: &[&str]) -> Vec<u8> {
        let put16 = |buf: &mut Vec<u8>, off: usize, x: u16| {
            buf[off..off + 2].copy_from_slice(&x.to_le_bytes());
        };
        let put32 = |buf: &mut Vec<u8>, off: usize, x: u32| {
            buf[off..off + 4].copy_from_slice(&x.to_le_bytes());
        };
        let put64 = |buf: &mut Vec<u8>, off: usize, x: u64| {
            buf[off..off + 8].copy_from_slice(&x.to_le_bytes());
        };
        let mut v = vec![0u8; 64]; // DOS header
        v[0..2].copy_from_slice(b"MZ");
        put32(&mut v, 60, 64); // e_lfanew
        v.extend_from_slice(b"PE\0\0");
        // COFF file header (20 bytes)
        v.extend_from_slice(&[0u8; 20]);
        let fh = v.len() - 20;
        put16(&mut v, fh, 0x8664); // machine: x64
        put16(&mut v, fh + 2, 1); // sections
        put16(&mut v, fh + 16, 240); // size of optional header
                                     // PE32+ optional header (240 bytes)
        let opt = v.len();
        v.extend_from_slice(&[0u8; 240]);
        put16(&mut v, opt, 0x20b); // magic
        put32(&mut v, opt + 20, 0x200); // base of code
        put64(&mut v, opt + 24, 0x2000); // image base (8 bytes in PE32+)
        put32(&mut v, opt + 32, 0x1000); // section alignment
        put32(&mut v, opt + 36, 0x200); // file alignment
        put32(&mut v, opt + 56, 0x4000); // size of image
        put32(&mut v, opt + 60, 0x200); // size of headers
        put16(&mut v, opt + 68, 3); // subsystem
        put32(&mut v, opt + 112, 0x200); // data dir [0].rva: export table
        put32(&mut v, opt + 116, 40); // data dir [0].size
                                      // Section header (40 bytes): .text at RVA 0x200 / file offset 0x200
        v.extend_from_slice(b".text\0\0\0");
        v.extend_from_slice(&[0u8; 32]); // rest of the 40-byte section header
        let sh = v.len() - 40;
        put32(&mut v, sh + 8, 0x1E00); // virtual size
        put32(&mut v, sh + 12, 0x200); // virtual address
        put32(&mut v, sh + 16, 0x200); // size of raw data
        put32(&mut v, sh + 20, 0x200); // pointer to raw data
        v.extend_from_slice(&[0u8; 12]);
        // Cover the whole section data region (export dir @0x200, tables
        // @0x300-0x360, name strings @0x400+, code @0x1000).
        while v.len() < 0x1010 {
            v.push(0);
        }
        // Section data: here file offset == RVA (raw pointer == vaddr).
        let at = |rva: usize| rva;
        // Export directory at RVA 0x200
        let exp = at(0x200);
        let n = names.len() as u32;
        put32(&mut v, exp + 20, n); // number of functions
        put32(&mut v, exp + 24, n); // number of names
        put32(&mut v, exp + 28, 0x300); // address of functions
        put32(&mut v, exp + 32, 0x320); // address of names
        put32(&mut v, exp + 36, 0x340); // address of name ordinals
                                        // Export name strings at RVA 0x400+
        let mut off = 0x400usize;
        let mut name_rvas = Vec::new();
        for name in names {
            name_rvas.push(off);
            let p = at(off);
            if v.len() < p + name.len() + 8 {
                v.resize(p + name.len() + 8, 0);
            }
            v[p..p + name.len()].copy_from_slice(name.as_bytes());
            off += name.len() + 8;
        }
        // Names table: RVA of each name string
        for (i, rva) in name_rvas.iter().enumerate() {
            put32(&mut v, at(0x320 + i * 4), *rva as u32);
        }
        // Ordinal table: identity mapping
        for i in 0..names.len() {
            put16(&mut v, at(0x340 + i * 2), i as u16);
        }
        // Function table: RVA 0x1000 (code, not a forwarder string)
        for i in 0..names.len() {
            put32(&mut v, at(0x300 + i * 4), 0x1000);
        }
        // Code at 0x1000: bytes that cannot parse as a `dll:sym` string
        let p = at(0x1000);
        if v.len() < p + 16 {
            v.resize(p + 16, 0);
        }
        for (b, slot) in v[p..p + 16].iter_mut().enumerate() {
            *slot = 0xc3 + (b % 10) as u8;
        }
        v
    }

    #[test]
    fn test_parse_pe_exports_plain() {
        let pe = minimal_pe(&["l_A", "l_B"]);
        let exports = parse_pe_exports(&pe).unwrap();
        let names: Vec<_> = exports.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["l_A", "l_B"]);
        assert!(exports.iter().all(|e| e.forwarder.is_none()));
    }

    #[test]
    fn test_parse_pe_exports_forwarder() {
        let mut pe = minimal_pe(&["l_Fwd"]);
        // Place a `dll:symbol` forwarder string at RVA 0x800 and point
        // function 0 at it.
        let s = b"libleanshared_1.dll:l_Fwd\0";
        pe[0x800..0x800 + s.len()].copy_from_slice(s);
        pe[0x300..0x304].copy_from_slice(&0x800u32.to_le_bytes());
        let exports = parse_pe_exports(&pe).unwrap();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].name, "l_Fwd");
        assert_eq!(
            exports[0].forwarder.as_ref(),
            Some(&("libleanshared_1.dll".to_string(), "l_Fwd".to_string()))
        );
    }

    #[test]
    fn test_parse_pe_exports_rejects_code_forwarder() {
        let mut pe = minimal_pe(&["l_Code"]);
        // Code bytes that contain both ':' and '.' are not
        // `dll:symbol` strings and must stay a plain export.
        let code = [0x48, 0x3a, 0x2e, 0x8b, 0x03, 0x3a, 0x00];
        let p = 0x1000;
        pe[p..p + code.len()].copy_from_slice(&code);
        let exports = parse_pe_exports(&pe).unwrap();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].name, "l_Code");
        assert!(exports[0].forwarder.is_none());
    }

    #[test]
    fn test_parse_pe_exports_rejects_non_pe() {
        assert!(parse_pe_exports(b"nope").is_err());
    }
}
