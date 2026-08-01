//! Binary embedding format for Leo3 metadata.
//!
//! The `#[leanmodule]` / `#[leanclass]` macros embed metadata JSON into a cdylib
//! so that `leo3-codegen` can recover it and generate Lean declarations.
//!
//! Historically the JSON was only reachable through the symbol table
//! (`#[no_mangle] #[used] pub static __leo3_*_metadata_json_*`). That works on
//! ELF (Linux) where such globals land in the dynamic symbol table, but on
//! Mach-O (macOS) the linker does not surface these unreferenced data symbols
//! in the dylib's symbol table, so `leo3-codegen` could not find them.
//!
//! To be robust across platforms the macros *also* emit each entry into a
//! dedicated link section using a self-describing framing defined here.
//! `leo3-codegen` scans that section as a fallback (and merges the result with
//! any symbols it finds). The framing is deliberately self-synchronizing: every
//! entry starts with a magic marker and carries explicit lengths, so the scanner
//! can recover entry boundaries even if the linker inserts padding between
//! entries or reorders them within the section.

/// Magic bytes marking the start of a framed metadata entry.
pub const METADATA_ENTRY_MAGIC: [u8; 4] = *b"L3MJ";

/// Substring used to locate the Leo3 metadata section in a binary.
///
/// The section is named `leo3meta` on ELF/PE and placed in the `__DATA`
/// segment on Mach-O (`__DATA,__leo3meta`), where the section name reported by
/// readers is just `__leo3meta`. Matching on this substring works for all of
/// them without needing to know the exact per-platform spelling.
///
/// The non-Apple name is deliberately at most 8 bytes: PE section names longer
/// than that are truncated by MSVC's `link.exe`, which would break the match.
pub const METADATA_SECTION_MARKER: &str = "leo3meta";

/// The link section used on non-Apple targets (kept <= 8 bytes for PE).
pub const METADATA_SECTION_NAME: &str = "leo3meta";

/// The link section used on Apple (Mach-O) targets: `segment,section`.
pub const METADATA_SECTION_NAME_APPLE: &str = "__DATA,__leo3meta";

/// Build a self-describing framed metadata entry.
///
/// Layout: `MAGIC | name_len (u32 LE) | json_len (u32 LE) | name | json`.
///
/// `name` is the full metadata symbol name (e.g.
/// `__leo3_module_metadata_json_Foo`); consumers use its prefix to distinguish
/// module metadata from class metadata.
pub fn frame_metadata_entry(name: &str, json: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(METADATA_ENTRY_MAGIC.len() + 8 + name.len() + json.len());
    out.extend_from_slice(&METADATA_ENTRY_MAGIC);
    out.extend_from_slice(&(name.len() as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(json.as_bytes());
    out
}

/// Scan a blob of section data for framed metadata entries.
///
/// Returns `(name, json)` pairs. The scan is resilient to padding and ordering:
/// it searches for the magic marker and validates lengths/UTF-8 before accepting
/// an entry, advancing byte-by-byte when out of sync.
pub fn parse_metadata_entries(data: &[u8]) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let header_len = METADATA_ENTRY_MAGIC.len() + 8;
    let mut i = 0usize;

    while i + header_len <= data.len() {
        if data[i..i + METADATA_ENTRY_MAGIC.len()] != METADATA_ENTRY_MAGIC {
            i += 1;
            continue;
        }

        let name_len =
            u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as usize;
        let json_len =
            u32::from_le_bytes([data[i + 8], data[i + 9], data[i + 10], data[i + 11]]) as usize;

        let name_start = i + header_len;
        let json_start = match name_start.checked_add(name_len) {
            Some(v) => v,
            None => {
                i += 1;
                continue;
            }
        };
        let end = match json_start.checked_add(json_len) {
            Some(v) => v,
            None => {
                i += 1;
                continue;
            }
        };

        if end <= data.len() {
            let name_bytes = &data[name_start..json_start];
            let json_bytes = &data[json_start..end];
            if let (Ok(name), Ok(json)) = (
                std::str::from_utf8(name_bytes),
                std::str::from_utf8(json_bytes),
            ) {
                results.push((name.to_string(), json.to_string()));
                i = end;
                continue;
            }
        }

        i += 1;
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_single_entry() {
        let framed = frame_metadata_entry("__leo3_module_metadata_json_Foo", "{\"a\":1}");
        let parsed = parse_metadata_entries(&framed);
        assert_eq!(
            parsed,
            vec![(
                "__leo3_module_metadata_json_Foo".to_string(),
                "{\"a\":1}".to_string()
            )]
        );
    }

    #[test]
    fn round_trip_multiple_with_padding() {
        let mut blob = Vec::new();
        blob.extend_from_slice(&[0u8; 3]); // leading padding
        blob.extend_from_slice(&frame_metadata_entry(
            "__leo3_module_metadata_json_A",
            "{\"m\":1}",
        ));
        blob.extend_from_slice(&[0u8; 5]); // inter-entry padding
        blob.extend_from_slice(&frame_metadata_entry(
            "__leo3_class_metadata_json_B",
            "{\"c\":2}",
        ));
        blob.extend_from_slice(&[0u8; 2]); // trailing padding

        let parsed = parse_metadata_entries(&blob);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "__leo3_module_metadata_json_A");
        assert_eq!(parsed[0].1, "{\"m\":1}");
        assert_eq!(parsed[1].0, "__leo3_class_metadata_json_B");
        assert_eq!(parsed[1].1, "{\"c\":2}");
    }

    #[test]
    fn ignores_garbage() {
        let blob = [0u8, 1, 2, 3, b'L', b'3', 0, 0, 9, 9];
        assert!(parse_metadata_entries(&blob).is_empty());
    }
}
