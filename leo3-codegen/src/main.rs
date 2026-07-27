use std::path::{Path, PathBuf};
use std::process::ExitCode;

use leo3_binding_ir::{ClassMetadata, FunctionBinding, ModuleBinding};
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
    eprintln!(
        "leo3-codegen: generate Lean 4 extern declarations from Leo3 cdylib metadata"
    );
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    leo3-codegen [OPTIONS] <cdylib>...");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!(
        "    -o, --output <DIR>    Output directory for generated .lean files (default: .)"
    );
    eprintln!("    -h, --help            Print this help message");
    eprintln!();
    eprintln!("EXAMPLES:");
    eprintln!("    leo3-codegen target/debug/libmy_module.so -o lean/MyModule");
}

fn process_library(lib_path: &Path, output_dir: &Path) -> Result<(), String> {
    let data = std::fs::read(lib_path).map_err(|e| format!("failed to read file: {e}"))?;

    let symbols = extract_metadata_symbols(&data)?;

    if symbols.is_empty() {
        return Err("no leo3 metadata symbols found in library".to_string());
    }

    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("failed to create output directory: {e}"))?;

    let mut generated = Vec::new();

    for (name, json_data) in &symbols {
        if let Some(module_name) = name.strip_prefix("__leo3_module_metadata_json_") {
            let binding: ModuleBinding = serde_json::from_str(json_data).map_err(|e| {
                format!("failed to parse module metadata for `{module_name}`: {e}")
            })?;
            let lean_code = generate_module_lean(&binding);
            let file_name = format!("{}.lean", module_name.replace('.', "/"));
            let file_path = output_dir.join(&file_name);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create directory: {e}"))?;
            }
            std::fs::write(&file_path, &lean_code)
                .map_err(|e| format!("failed to write {}: {e}", file_path.display()))?;
            generated.push(file_path);
        } else if let Some(class_name) = name.strip_prefix("__leo3_class_metadata_json_") {
            let metadata: ClassMetadata = serde_json::from_str(json_data).map_err(|e| {
                format!("failed to parse class metadata for `{class_name}`: {e}")
            })?;
            let lean_code = generate_class_lean(&metadata);
            let file_path = output_dir.join(format!("{class_name}.lean"));
            std::fs::write(&file_path, &lean_code)
                .map_err(|e| format!("failed to write {}: {e}", file_path.display()))?;
            generated.push(file_path);
        }
    }

    for path in &generated {
        println!("{}", path.display());
    }

    Ok(())
}

fn extract_metadata_symbols(data: &[u8]) -> Result<Vec<(String, String)>, String> {
    let obj =
        object::File::parse(data).map_err(|e| format!("failed to parse object file: {e}"))?;

    let mut results = Vec::new();

    for symbol in obj.symbols() {
        let name = match symbol.name() {
            Ok(n) => n,
            Err(_) => continue,
        };

        let is_metadata = name.starts_with("__leo3_module_metadata_json_")
            || name.starts_with("__leo3_class_metadata_json_");
        if !is_metadata {
            continue;
        }

        let address = symbol.address();
        let size = symbol.size();

        let json = if size > 0 {
            read_bytes_at(&obj, address, size as usize)?
        } else {
            read_null_terminated_at(&obj, address)?
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
