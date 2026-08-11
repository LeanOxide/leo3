use std::path::{Path, PathBuf};
use std::process::ExitCode;

use leo3_binding_ir::{
    parse_metadata_entries, ClassMetadata, FunctionBinding, ModuleBinding, METADATA_SECTION_MARKER,
};
use object::{Object, ObjectSection, ObjectSymbol};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_usage();
        return if args.len() < 2 {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }

    let mut output_dir: Option<PathBuf> = None;
    let mut lib_paths: Vec<PathBuf> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --output requires a path argument");
                    return ExitCode::FAILURE;
                }
                output_dir = Some(PathBuf::from(&args[i]));
            }
            other => {
                lib_paths.push(PathBuf::from(other));
            }
        }
        i += 1;
    }

    if lib_paths.is_empty() {
        eprintln!("error: no library path specified");
        return ExitCode::FAILURE;
    }

    let output_dir = output_dir.unwrap_or_else(|| PathBuf::from("."));

    for lib_path in &lib_paths {
        if let Err(err) = process_library(lib_path, &output_dir) {
            eprintln!("error: {}: {err}", lib_path.display());
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

fn print_usage() {
    eprintln!("leo3-codegen: generate Lean 4 extern declarations from Leo3 cdylib metadata");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    leo3-codegen [OPTIONS] <cdylib>...");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("    -o, --output <DIR>    Output directory for generated .lean files (default: .)");
    eprintln!("    -h, --help            Print this help message");
    eprintln!();
    eprintln!("EXAMPLES:");
    eprintln!("    leo3-codegen target/debug/libmy_module.so -o lean/MyModule");
}

/// The kind of leo3 metadata a symbol name carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataKind {
    Module,
    Class,
}

/// Classify a metadata symbol name into its kind and base name.
///
/// Module symbols are `__leo3_module_metadata_json_<name>` and class symbols
/// are `__leo3_class_metadata_json_<name>`. Anything else is not metadata.
fn metadata_symbol_kind(name: &str) -> Option<(MetadataKind, &str)> {
    if let Some(rest) = name.strip_prefix("__leo3_module_metadata_json_") {
        Some((MetadataKind::Module, rest))
    } else if let Some(rest) = name.strip_prefix("__leo3_class_metadata_json_") {
        Some((MetadataKind::Class, rest))
    } else {
        None
    }
}

fn process_library(lib_path: &Path, output_dir: &Path) -> Result<(), String> {
    let data = std::fs::read(lib_path).map_err(|e| format!("failed to read file: {e}"))?;

    let obj = object::File::parse(data.as_slice())
        .map_err(|e| format!("failed to parse object file: {e}"))?;

    let symbols = collect_metadata(&obj)?;

    if symbols.is_empty() {
        return Err(
            "no leo3 metadata found in library (checked symbol table and metadata section)"
                .to_string(),
        );
    }

    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("failed to create output directory: {e}"))?;

    let mut generated = Vec::new();

    // Class metadata may arrive from multiple symbols for the same class
    // (the `#[leanclass]` impl block and the `#[get]`/`#[set]` field
    // accessors emit separate entries). Merge them by `rust_name` so each
    // class produces exactly one `.lean` file containing all its methods.
    let mut classes: std::collections::BTreeMap<String, ClassMetadata> =
        std::collections::BTreeMap::new();

    for (name, json_data) in &symbols {
        match metadata_symbol_kind(name) {
            Some((MetadataKind::Module, module_name)) => {
                let binding: ModuleBinding = serde_json::from_str(json_data).map_err(|e| {
                    format!("failed to parse module metadata for `{module_name}`: {e}")
                })?;
                let lean_code = generate_module_lean(&binding);
                // File paths follow the real (dotted) module name so nested
                // modules land where Lean's import resolution expects them
                // (`A.B` -> `A/B.lean`). The symbol-derived `module_name`
                // has dots sanitized away and would flatten the hierarchy.
                let file_name = format!("{}.lean", binding.name.replace('.', "/"));
                let file_path = output_dir.join(&file_name);
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create directory: {e}"))?;
                }
                std::fs::write(&file_path, &lean_code)
                    .map_err(|e| format!("failed to write {}: {e}", file_path.display()))?;
                generated.push(file_path);
            }
            Some((MetadataKind::Class, _class_name)) => {
                let metadata: ClassMetadata = serde_json::from_str(json_data)
                    .map_err(|e| format!("failed to parse class metadata for `{name}`: {e}"))?;
                merge_class_metadata(&mut classes, metadata);
            }
            None => {}
        }
    }

    for (class_name, metadata) in &classes {
        let lean_code = generate_class_lean(metadata);
        let file_path = output_dir.join(format!("{class_name}.lean"));
        std::fs::write(&file_path, &lean_code)
            .map_err(|e| format!("failed to write {}: {e}", file_path.display()))?;
        generated.push(file_path);
    }

    for path in &generated {
        println!("{}", path.display());
    }

    Ok(())
}

/// Merge one class metadata entry into the per-class map.
///
/// Methods are deduplicated by `(rust_name, lean_name)` so a class that
/// arrives across multiple entries (the `#[leanclass]` impl block plus the
/// `#[get]`/`#[set]` field accessors) still produces exactly one `.lean` file
/// containing each method once.
fn merge_class_metadata(
    classes: &mut std::collections::BTreeMap<String, ClassMetadata>,
    metadata: ClassMetadata,
) {
    match classes.get_mut(&metadata.rust_name) {
        Some(existing) => {
            for method in metadata.methods {
                if !existing
                    .methods
                    .iter()
                    .any(|m| m.rust_name == method.rust_name && m.lean_name == method.lean_name)
                {
                    existing.methods.push(method);
                }
            }
        }
        None => {
            classes.insert(metadata.rust_name.clone(), metadata);
        }
    }
}

fn collect_metadata(obj: &object::File) -> Result<Vec<(String, String)>, String> {
    let mut results: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Primary path: the symbol table. Works on ELF (Linux), where the
    // `#[no_mangle] #[used]` metadata globals land in the dynamic symbol table.
    for (name, json) in extract_metadata_symbols(obj)? {
        if seen.insert(name.clone()) {
            results.push((name, json));
        }
    }

    // Fallback / complement: scan the dedicated metadata section. On Mach-O
    // (macOS) dylibs the linker does not surface those unreferenced data symbols
    // in the symbol table, but the macros also embed a self-describing framed
    // copy into a dedicated section that we can recover here.
    for (name, json) in extract_metadata_from_sections(obj) {
        if seen.insert(name.clone()) {
            results.push((name, json));
        }
    }

    // PE (Windows) complement: linked DLLs no longer carry a COFF symbol table,
    // but `#[no_mangle]` metadata statics are present in the export table.
    for (name, json) in extract_metadata_from_exports(obj) {
        if seen.insert(name.clone()) {
            results.push((name, json));
        }
    }

    Ok(results)
}

fn extract_metadata_from_exports(obj: &object::File) -> Vec<(String, String)> {
    use object::read::pe::ExportTarget;

    let export_tables: Vec<_> = match obj {
        object::File::Pe32(pe) => vec![pe.export_table()],
        object::File::Pe64(pe) => vec![pe.export_table()],
        _ => Vec::new(),
    };

    let mut results = Vec::new();
    for table in export_tables.into_iter().flatten().flatten() {
        let Ok(exports) = table.exports() else {
            continue;
        };
        for export in exports {
            let Some(name_bytes) = export.name else {
                continue;
            };
            let Ok(name) = std::str::from_utf8(name_bytes) else {
                continue;
            };
            if metadata_symbol_kind(name).is_none() {
                continue;
            }
            let ExportTarget::Address(address) = export.target else {
                continue;
            };
            // Export addresses are RVAs, matching the section addresses used by
            // the readers below. Exports carry no size; the JSON statics are
            // NUL-terminated.
            let Ok(json) = read_null_terminated_at(obj, u64::from(address)) else {
                continue;
            };
            results.push((name.to_string(), json));
        }
    }
    results
}

fn extract_metadata_from_sections(obj: &object::File) -> Vec<(String, String)> {
    let mut results = Vec::new();
    for section in obj.sections() {
        let name = match section.name() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if !name.contains(METADATA_SECTION_MARKER) {
            continue;
        }
        let data = match section.data() {
            Ok(d) => d,
            Err(_) => continue,
        };
        results.extend(parse_metadata_entries(data));
    }
    results
}

fn extract_metadata_symbols(obj: &object::File) -> Result<Vec<(String, String)>, String> {
    let mut results = Vec::new();

    for symbol in obj.symbols() {
        let name = match symbol.name() {
            Ok(n) => n,
            Err(_) => continue,
        };

        if metadata_symbol_kind(name).is_none() {
            continue;
        }

        let address = symbol.address();
        let size = symbol.size();

        let json = if size > 0 {
            read_bytes_at(obj, address, size as usize)?
        } else {
            read_null_terminated_at(obj, address)?
        };

        results.push((name.to_string(), json));
    }

    Ok(results)
}

fn read_bytes_at(obj: &object::File, address: u64, size: usize) -> Result<String, String> {
    for section in obj.sections() {
        let section_addr = section.address();
        let section_size = section.size();
        if address >= section_addr && address + size as u64 <= section_addr + section_size {
            let offset = (address - section_addr) as usize;
            let data = section
                .data()
                .map_err(|e| format!("failed to read section data: {e}"))?;
            let bytes = &data[offset..offset + size];
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            return String::from_utf8(bytes[..end].to_vec())
                .map_err(|e| format!("metadata is not valid UTF-8: {e}"));
        }
    }
    Err(format!(
        "no section contains address {address:#x} (size {size})"
    ))
}

fn read_null_terminated_at(obj: &object::File, address: u64) -> Result<String, String> {
    for section in obj.sections() {
        let section_addr = section.address();
        let section_size = section.size();
        if address >= section_addr && address < section_addr + section_size {
            let offset = (address - section_addr) as usize;
            let data = section
                .data()
                .map_err(|e| format!("failed to read section data: {e}"))?;
            let slice = &data[offset..];
            let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
            return String::from_utf8(slice[..end].to_vec())
                .map_err(|e| format!("metadata is not valid UTF-8: {e}"));
        }
    }
    Err(format!("no section contains address {address:#x}"))
}

fn generate_module_lean(binding: &ModuleBinding) -> String {
    let mut out = String::new();
    out.push_str("-- Generated by leo3-codegen. Do not edit.\n");
    out.push_str(&format!("-- Module: {}\n\n", binding.name));

    for export in &binding.exports {
        out.push_str(&generate_function_extern(export));
        out.push('\n');
    }

    for submodule in &binding.submodules {
        out.push_str(&format!("-- Submodule: {}\n", submodule.path));
        for export in &submodule.exports {
            out.push_str(&generate_function_extern(export));
            out.push('\n');
        }
    }

    out
}

fn generate_class_lean(metadata: &ClassMetadata) -> String {
    let mut out = String::new();
    out.push_str("-- Generated by leo3-codegen. Do not edit.\n");
    out.push_str(&format!("-- Class: {}\n\n", metadata.lean_name));
    out.push_str(&metadata.opaque_decl);
    out.push_str("\n\n");

    for method in &metadata.methods {
        if let Some(decl) = &method.lean_decl {
            out.push_str(decl);
            out.push('\n');
        }
    }

    out
}

fn generate_function_extern(func: &FunctionBinding) -> String {
    if let Some(decl) = &func.lean_decl {
        return format!("{decl}\n");
    }

    let mut parts: Vec<String> = Vec::new();
    for param in &func.params {
        let lean_ty = param
            .ty
            .lean
            .clone()
            .unwrap_or_else(|| param.ty.rust.clone());
        parts.push(lean_ty);
    }
    let ret = func
        .return_type
        .lean
        .clone()
        .unwrap_or_else(|| func.return_type.rust.clone());
    parts.push(ret);

    format!(
        "@[extern \"{}\"] opaque {} : {}\n",
        func.ffi_symbol,
        func.lean_name,
        parts.join(" → ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use leo3_binding_ir::{
        frame_metadata_entry, parse_metadata_entries, BindingKind, BindingSemantics,
        ParameterBinding, PassingStyle, ReceiverStyle, SubmoduleBinding, TypeBinding, TypeShape,
        BINDING_SCHEMA_VERSION,
    };

    // ------------------------------------------------------------------
    // Pure helpers
    // ------------------------------------------------------------------

    #[test]
    fn metadata_symbol_kind_parses_prefixes() {
        assert_eq!(
            metadata_symbol_kind("__leo3_module_metadata_json_Foo"),
            Some((MetadataKind::Module, "Foo"))
        );
        assert_eq!(
            metadata_symbol_kind("__leo3_class_metadata_json_Bar"),
            Some((MetadataKind::Class, "Bar"))
        );
        assert_eq!(
            metadata_symbol_kind("__leo3_module_metadata_json_"),
            Some((MetadataKind::Module, ""))
        );
        assert_eq!(
            metadata_symbol_kind("__leo3_class_metadata_json_A.B"),
            Some((MetadataKind::Class, "A.B"))
        );
        assert_eq!(metadata_symbol_kind("some_other_symbol"), None);
        assert_eq!(metadata_symbol_kind("__leo3_module_metadata_"), None);
        // No trailing underscore after the prefix.
        assert_eq!(metadata_symbol_kind("__leo3_module_metadata_json"), None);
        assert_eq!(metadata_symbol_kind(""), None);
    }

    fn type_binding(rust: &str, lean: Option<&str>) -> TypeBinding {
        TypeBinding {
            rust: rust.to_string(),
            lean: lean.map(str::to_string),
            shape: TypeShape::Scalar,
        }
    }

    fn method(name: &str) -> FunctionBinding {
        FunctionBinding {
            rust_name: name.to_string(),
            lean_name: name.to_string(),
            owner: Some("Counter".to_string()),
            ffi_symbol: format!("__lean_ffi_{}", name),
            receiver: ReceiverStyle::None,
            params: vec![],
            return_type: type_binding("u32", Some("UInt32")),
            semantics: BindingSemantics::Value,
            kind: BindingKind::Method,
            lean_decl: None,
        }
    }

    fn class_metadata(name: &str, methods: Vec<FunctionBinding>) -> ClassMetadata {
        ClassMetadata {
            schema_version: BINDING_SCHEMA_VERSION,
            rust_name: name.to_string(),
            lean_name: name.to_string(),
            opaque_decl: format!("opaque {name} : Type\n"),
            methods_decl: String::new(),
            methods,
        }
    }

    fn module_binding(
        name: &str,
        exports: Vec<FunctionBinding>,
        submodules: Vec<SubmoduleBinding>,
    ) -> ModuleBinding {
        ModuleBinding {
            name: name.to_string(),
            exports,
            submodules,
        }
    }

    #[test]
    fn merge_class_metadata_dedups_methods_by_rust_and_lean_name() {
        let mut classes = std::collections::BTreeMap::new();
        merge_class_metadata(
            &mut classes,
            class_metadata("Counter", vec![method("inc"), method("get")]),
        );
        // Same rust_name: only the *new* method (set_value) is added; inc and
        // get are deduplicated.
        merge_class_metadata(
            &mut classes,
            class_metadata("Counter", vec![method("inc"), method("set_value")]),
        );
        // Different rust_name → a separate entry.
        merge_class_metadata(&mut classes, class_metadata("Other", vec![method("new")]));

        assert_eq!(classes.len(), 2);
        let counter = &classes["Counter"];
        let names: Vec<&str> = counter
            .methods
            .iter()
            .map(|m| m.lean_name.as_str())
            .collect();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"inc"));
        assert!(names.contains(&"get"));
        assert!(names.contains(&"set_value"));
        assert_eq!(classes["Other"].methods.len(), 1);
    }

    #[test]
    fn generate_module_lean_emits_exports_and_submodules() {
        let sub = SubmoduleBinding {
            path: "A.B.Inner".to_string(),
            exports: vec![method("inner_fn")],
        };
        let binding = module_binding("A.B", vec![method("outer_fn")], vec![sub]);
        let text = generate_module_lean(&binding);

        assert!(text.starts_with("-- Generated by leo3-codegen. Do not edit."));
        assert!(text.contains("-- Module: A.B"));
        assert!(text.contains("-- Submodule: A.B.Inner"));
        assert!(text.contains("@[extern \"__lean_ffi_outer_fn\"] opaque outer_fn : UInt32"));
        assert!(text.contains("@[extern \"__lean_ffi_inner_fn\"] opaque inner_fn : UInt32"));
    }

    #[test]
    fn generate_class_lean_emits_opaque_decl_and_methods() {
        let mut with_decl = method("decl_method");
        with_decl.lean_decl = Some("opaque DeclMethod : UInt32".to_string());
        // Methods without a pre-rendered decl are not emitted by
        // generate_class_lean (the class decls are authoritative).
        let metadata = class_metadata("Counter", vec![with_decl, method("plain")]);
        let text = generate_class_lean(&metadata);

        assert!(text.contains("-- Class: Counter"));
        assert!(text.contains("opaque Counter : Type"));
        assert!(text.contains("opaque DeclMethod : UInt32\n"));
        assert!(!text.contains("plain"));
    }

    #[test]
    fn generate_function_extern_uses_decl_or_synthesizes() {
        let mut func = method("f");
        func.lean_decl = Some("opaque Custom : Unit".to_string());
        assert_eq!(generate_function_extern(&func), "opaque Custom : Unit\n");

        // Params with no Lean type fall back to the Rust type name.
        let mut plain = method("g");
        plain.params = vec![ParameterBinding {
            name: "x".to_string(),
            ty: type_binding("u64", None),
            passing: PassingStyle::Borrowed,
        }];
        plain.return_type = type_binding("()", Some("Unit"));
        let text = generate_function_extern(&plain);
        assert_eq!(text, "@[extern \"__lean_ffi_g\"] opaque g : u64 → Unit\n");
    }

    #[test]
    fn framed_metadata_round_trip() {
        let framed = frame_metadata_entry(
            "__leo3_module_metadata_json_RoundTrip",
            r#"{"name":"RoundTrip","exports":[],"submodules":[]}"#,
        );
        let parsed = parse_metadata_entries(&framed);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "__leo3_module_metadata_json_RoundTrip");
        assert!(parsed[0].1.contains("RoundTrip"));
    }

    // ------------------------------------------------------------------
    // Fake object-file fixtures (minimal ELF64)
    // ------------------------------------------------------------------

    fn push_u16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn push_u32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn push_u64(out: &mut Vec<u8>, v: u64) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    const ELF64_IDENT: [u8; 16] = [0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    struct FakeSection {
        /// Raw name bytes stored in `.shstrtab` (may be invalid UTF-8).
        name: Vec<u8>,
        ty: u32,
        /// `None` produces a header whose size points beyond the file, so
        /// reading the section data fails.
        data: Option<Vec<u8>>,
        align: u64,
        entsize: u64,
        link: u32,
        info: u32,
    }

    /// Build a minimal ELF64 relocatable object.
    fn fake_elf(sections: &[FakeSection]) -> Vec<u8> {
        let mut shstr = vec![0u8];
        let mut name_offsets = Vec::new();
        for s in sections {
            name_offsets.push(shstr.len() as u32);
            shstr.extend_from_slice(&s.name);
            shstr.push(0);
        }

        // Data layout: ELF header (64) | .shstrtab | section data | section headers.
        let mut offset = 64u64;
        let shstr_offset = offset;
        offset += shstr.len() as u64;

        let mut sh_offsets = vec![0u64; sections.len()];
        let mut sh_sizes = vec![0u64; sections.len()];
        for (i, s) in sections.iter().enumerate() {
            let align = s.align.max(1);
            offset = offset.div_ceil(align) * align;
            sh_offsets[i] = offset;
            match &s.data {
                Some(d) => {
                    sh_sizes[i] = d.len() as u64;
                    offset += d.len() as u64;
                }
                None => {
                    // Claim a range far beyond the file.
                    sh_sizes[i] = 0x1000;
                }
            }
        }

        // The section header table must be 8-byte aligned for ELF64 (the
        // `object` crate rejects unaligned tables, unlike readelf).
        let shoff = offset.div_ceil(8) * 8;
        let shnum = sections.len() as u16 + 2; // null + .shstrtab + named

        let mut out = Vec::new();
        out.extend_from_slice(&ELF64_IDENT);
        push_u16(&mut out, 1); // e_type: ET_REL
        push_u16(&mut out, 0x3e); // e_machine: x86_64
        push_u32(&mut out, 1); // e_version
        push_u64(&mut out, 0); // e_entry
        push_u64(&mut out, 0); // e_phoff
        push_u64(&mut out, shoff);
        push_u32(&mut out, 0); // e_flags
        push_u16(&mut out, 64); // e_ehsize
        push_u16(&mut out, 0); // e_phentsize
        push_u16(&mut out, 0); // e_phnum
        push_u16(&mut out, 64); // e_shentsize
        push_u16(&mut out, shnum);
        push_u16(&mut out, 1); // e_shstrndx: .shstrtab
        assert_eq!(out.len(), 64);

        out.extend_from_slice(&shstr);
        for (i, s) in sections.iter().enumerate() {
            let pad = sh_offsets[i] as usize - out.len();
            out.extend(std::iter::repeat_n(0u8, pad));
            if let Some(d) = &s.data {
                out.extend_from_slice(d);
            }
        }
        let pad = shoff as usize - out.len();
        out.extend(std::iter::repeat_n(0u8, pad));

        // Null section header (name, type, flags, addr, offset, size, link,
        // info, align, entsize — 64 bytes).
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u64(&mut out, 0);
        push_u64(&mut out, 0);
        push_u64(&mut out, 0);
        push_u64(&mut out, 0);
        push_u32(&mut out, 0);
        push_u32(&mut out, 0);
        push_u64(&mut out, 0);
        push_u64(&mut out, 0);

        // .shstrtab header (name offset 1: right after the leading NUL).
        push_u32(&mut out, 1);
        push_u32(&mut out, 3); // SHT_STRTAB
        push_u64(&mut out, 0); // flags
        push_u64(&mut out, 0); // addr
        push_u64(&mut out, shstr_offset);
        push_u64(&mut out, shstr.len() as u64);
        push_u32(&mut out, 0); // link
        push_u32(&mut out, 0); // info
        push_u64(&mut out, 1); // align
        push_u64(&mut out, 0); // entsize

        for (i, s) in sections.iter().enumerate() {
            push_u32(&mut out, name_offsets[i]);
            push_u32(&mut out, s.ty);
            push_u64(&mut out, 0); // flags
            push_u64(&mut out, 0); // addr
            push_u64(&mut out, sh_offsets[i]);
            push_u64(&mut out, sh_sizes[i]);
            push_u32(&mut out, s.link);
            push_u32(&mut out, s.info);
            push_u64(&mut out, s.align.max(1));
            push_u64(&mut out, s.entsize);
        }

        out
    }

    /// A `leo3meta` section carrying the given framed metadata entries.
    fn fake_elf_with_meta_section(entries: &[(String, String)]) -> Vec<u8> {
        let mut meta = Vec::new();
        for (name, json) in entries {
            meta.extend_from_slice(&frame_metadata_entry(name, json));
        }
        fake_elf(&[FakeSection {
            name: b"leo3meta".to_vec(),
            ty: 1, // SHT_PROGBITS
            data: Some(meta),
            align: 1,
            entsize: 0,
            link: 0,
            info: 0,
        }])
    }

    /// A `leo3meta` section plus a symbol table. Section indices are 0 null,
    /// 1 `.shstrtab`, 2 `.strtab`, 3 `.symtab`, 4 `leo3meta`, so `.symtab`'s
    /// `sh_link` is 2 (the `.strtab` section).
    fn fake_elf_with_symbols(symbols: &[(String, u64, u64)], meta: &[u8]) -> Vec<u8> {
        let mut strtab = vec![0u8];
        // ELF requires the null symbol at index 0; readers start at index 1.
        let mut symtab = vec![0u8; 24];
        for (name, addr, size) in symbols {
            let name_off = strtab.len() as u32;
            strtab.extend_from_slice(name.as_bytes());
            strtab.push(0);

            push_u32(&mut symtab, name_off);
            symtab.push(0x11); // st_info: GLOBAL | OBJECT
            symtab.push(0); // st_other
            push_u16(&mut symtab, 0); // st_shndx: SHN_UNDEF
            push_u64(&mut symtab, *addr); // st_value
            push_u64(&mut symtab, *size); // st_size
        }
        fake_elf(&[
            FakeSection {
                name: b".strtab".to_vec(),
                ty: 3, // SHT_STRTAB
                data: Some(strtab),
                align: 1,
                entsize: 0,
                link: 0,
                info: 0,
            },
            FakeSection {
                name: b".symtab".to_vec(),
                ty: 2, // SHT_SYMTAB
                data: Some(symtab),
                align: 8,
                entsize: 24,
                link: 2, // index of .strtab
                info: 1,
            },
            FakeSection {
                name: b"leo3meta".to_vec(),
                ty: 1,
                data: Some(meta.to_vec()),
                align: 1,
                entsize: 0,
                link: 0,
                info: 0,
            },
        ])
    }

    /// A `leo3meta` section plus a symbol table with raw `st_name` offsets
    /// (for exercising unreadable symbol names). Section indices are 0 null,
    /// 1 `.shstrtab`, 2 `.strtab`, 3 `.symtab`, 4 `leo3meta`, so `.symtab`'s
    /// `sh_link` is 2.
    fn fake_elf_with_raw_symbols(
        strtab: &[u8],
        entries: &[(u32, u64, u64)], // (st_name, st_value, st_size)
        meta: &[u8],
    ) -> Vec<u8> {
        // ELF requires the null symbol at index 0; readers start at index 1.
        let mut symtab = vec![0u8; 24];
        for (name_off, addr, size) in entries {
            push_u32(&mut symtab, *name_off);
            symtab.push(0x11); // st_info: GLOBAL | OBJECT
            symtab.push(0); // st_other
            push_u16(&mut symtab, 0); // st_shndx: SHN_UNDEF
            push_u64(&mut symtab, *addr); // st_value
            push_u64(&mut symtab, *size); // st_size
        }
        fake_elf(&[
            FakeSection {
                name: b".strtab".to_vec(),
                ty: 3,
                data: Some(strtab.to_vec()),
                align: 1,
                entsize: 0,
                link: 0,
                info: 0,
            },
            FakeSection {
                name: b".symtab".to_vec(),
                ty: 2,
                data: Some(symtab),
                align: 8,
                entsize: 24,
                link: 2,
                info: 1,
            },
            FakeSection {
                name: b"leo3meta".to_vec(),
                ty: 1,
                data: Some(meta.to_vec()),
                align: 1,
                entsize: 0,
                link: 0,
                info: 0,
            },
        ])
    }

    fn temp_unique_dir(stem: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "leo3-codegen-test-{}-{}-{}",
            stem,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ------------------------------------------------------------------
    // process_library integration-style tests over fake objects
    // ------------------------------------------------------------------

    #[test]
    fn process_library_generates_dotted_modules_and_merges_classes() {
        // Class methods only appear in the generated file when they carry a
        // pre-rendered `lean_decl`, so build them with one.
        let mut inc = method("inc");
        inc.lean_decl = Some("opaque Counter.inc : Counter → UInt32".to_string());
        let mut get = method("get");
        get.lean_decl = Some("opaque Counter.get : Counter → UInt32".to_string());

        let module_json =
            serde_json::to_string(&module_binding("A.B", vec![method("inc")], vec![])).unwrap();
        let class_json1 =
            serde_json::to_string(&class_metadata("Counter", vec![inc.clone()])).unwrap();
        let class_json2 =
            serde_json::to_string(&class_metadata("Counter", vec![inc.clone(), get.clone()]))
                .unwrap();

        let entries = vec![
            ("__leo3_module_metadata_json_AB".to_string(), module_json),
            (
                "__leo3_class_metadata_json_Counter".to_string(),
                class_json1,
            ),
            // Field-accessor entries carry a distinct symbol name
            // (`..._fields`), so both survive `collect_metadata` and the merge
            // in `process_library` deduplicates by rust_name.
            (
                "__leo3_class_metadata_json_Counter_fields".to_string(),
                class_json2,
            ),
        ];
        let dir = temp_unique_dir("out");
        let obj_path = dir.join("libfake.so");
        std::fs::write(&obj_path, fake_elf_with_meta_section(&entries)).unwrap();

        process_library(&obj_path, &dir).unwrap();

        // Dotted module name → A/B.lean under the output dir.
        let module_text = std::fs::read_to_string(dir.join("A").join("B.lean")).unwrap();
        assert!(module_text.contains("-- Module: A.B"));
        assert!(module_text.contains("@[extern \"__lean_ffi_inc\"] opaque inc : UInt32"));

        // Class → a single file with deduplicated methods.
        let class_text = std::fs::read_to_string(dir.join("Counter.lean")).unwrap();
        assert!(class_text.contains("-- Class: Counter"));
        assert_eq!(class_text.matches("opaque Counter.inc").count(), 1);
        assert!(class_text.contains("opaque Counter.get"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn process_library_reports_bad_json() {
        let dir = temp_unique_dir("badjson");
        let obj_path = dir.join("libbad.so");

        let entries = vec![(
            "__leo3_module_metadata_json_Bad".to_string(),
            "{not json".to_string(),
        )];
        std::fs::write(&obj_path, fake_elf_with_meta_section(&entries)).unwrap();
        let err = process_library(&obj_path, &dir).unwrap_err();
        assert!(
            err.contains("failed to parse module metadata for `Bad`"),
            "{err}"
        );

        let entries = vec![(
            "__leo3_class_metadata_json_Bad".to_string(),
            "{not json".to_string(),
        )];
        std::fs::write(&obj_path, fake_elf_with_meta_section(&entries)).unwrap();
        let err = process_library(&obj_path, &dir).unwrap_err();
        assert!(err.contains("failed to parse class metadata"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn process_library_reports_read_and_parse_errors() {
        let dir = temp_unique_dir("errors");
        let missing = dir.join("missing.so");
        let err = process_library(&missing, &dir).unwrap_err();
        assert!(err.contains("failed to read file"), "{err}");

        let text = dir.join("not_an_object.txt");
        std::fs::write(&text, "hello world").unwrap();
        let err = process_library(&text, &dir).unwrap_err();
        assert!(err.contains("failed to parse object file"), "{err}");

        // A valid object with no metadata entries at all.
        let no_meta = dir.join("libempty.so");
        std::fs::write(&no_meta, fake_elf(&[])).unwrap();
        let err = process_library(&no_meta, &dir).unwrap_err();
        assert!(err.contains("no leo3 metadata found"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn process_library_handles_symbol_table_paths() {
        let dir = temp_unique_dir("symtab");
        let obj_path = dir.join("libsym.so");

        // Symbol pointing outside every section → read_bytes_at error.
        let meta = frame_metadata_entry(
            "__leo3_module_metadata_json_Foo",
            r#"{"name":"Foo","exports":[],"submodules":[]}"#,
        );
        let data = fake_elf_with_symbols(
            &[(
                "__leo3_module_metadata_json_BadAddr".to_string(),
                0xdead_beef,
                4,
            )],
            &meta,
        );
        std::fs::write(&obj_path, data).unwrap();
        let err = process_library(&obj_path, &dir).unwrap_err();
        assert!(err.contains("no section contains address"), "{err}");

        // Symbol with size 0 → NUL-terminated read from the section.
        let mut meta = frame_metadata_entry(
            "__leo3_module_metadata_json_Foo",
            r#"{"name":"Foo","exports":[],"submodules":[]}"#,
        );
        let json = r#"{"name":"Bar","exports":[],"submodules":[]}"#;
        let json_addr = meta.len() as u64;
        meta.extend_from_slice(json.as_bytes());
        meta.push(0);
        let data = fake_elf_with_symbols(
            &[("__leo3_module_metadata_json_Bar".to_string(), json_addr, 0)],
            &meta,
        );
        std::fs::write(&obj_path, data).unwrap();
        process_library(&obj_path, &dir).unwrap();
        assert!(dir.join("Bar.lean").is_file());
        assert!(dir.join("Foo.lean").is_file());

        // Size-0 symbol outside every section → read_null_terminated_at error.
        let data = fake_elf_with_raw_symbols(
            b"\0__leo3_module_metadata_json_NoAddr\0",
            &[(1, 0xdead_beef, 0)],
            &meta,
        );
        std::fs::write(&obj_path, data).unwrap();
        let err = process_library(&obj_path, &dir).unwrap_err();
        assert!(err.contains("no section contains address"), "{err}");

        // Symbol whose name cannot be resolved → skipped, not fatal.
        let data = fake_elf_with_raw_symbols(b"\0", &[(99, 0x1000, 4)], &meta);
        std::fs::write(&obj_path, data).unwrap();
        process_library(&obj_path, &dir).unwrap();
        assert!(dir.join("Foo.lean").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn process_library_tolerates_unreadable_sections() {
        let dir = temp_unique_dir("badsections");
        let obj_path = dir.join("libweird.so");

        let good = frame_metadata_entry(
            "__leo3_module_metadata_json_Good",
            r#"{"name":"Good","exports":[],"submodules":[]}"#,
        );
        let sections = vec![
            // Invalid UTF-8 section name → skipped.
            FakeSection {
                name: vec![0xff, 0xfe, 0x80],
                ty: 1,
                data: Some(vec![1, 2, 3]),
                align: 1,
                entsize: 0,
                link: 0,
                info: 0,
            },
            // Data range beyond EOF → skipped.
            FakeSection {
                name: b"leo3meta".to_vec(),
                ty: 1,
                data: None,
                align: 1,
                entsize: 0,
                link: 0,
                info: 0,
            },
            // Valid section whose name contains the marker.
            FakeSection {
                name: b"leo3meta2".to_vec(),
                ty: 1,
                data: Some(good),
                align: 1,
                entsize: 0,
                link: 0,
                info: 0,
            },
        ];
        std::fs::write(&obj_path, fake_elf(&sections)).unwrap();

        process_library(&obj_path, &dir).unwrap();
        assert!(dir.join("Good.lean").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
