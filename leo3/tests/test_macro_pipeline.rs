//! Integration test covering the public macro golden path.

#![cfg(all(feature = "macros", feature = "runtime-tests"))]

use leo3::prelude::*;

#[derive(Clone, Debug, PartialEq)]
#[leanclass]
struct PipelineCounter {
    value: i32,
}

#[leanclass]
impl PipelineCounter {
    fn new(initial: i32) -> Self {
        Self { value: initial }
    }

    fn increment(&mut self, delta: i32) {
        self.value += delta;
    }

    fn get(&self) -> i32 {
        self.value
    }
}

#[leanmodule(name = "MacroPipeline")]
mod macro_pipeline {
    use leo3::prelude::*;

    #[leanfn(name = "macro_pipeline_add")]
    pub fn add(a: u64, b: u64) -> u64 {
        a + b
    }

    #[leanfn(name = "macro_pipeline_banner")]
    pub fn banner(name: String, count: i32) -> String {
        format!("{name} has {count} ticks")
    }
}

#[test]
fn test_macro_pipeline_rust_and_metadata_surface() {
    assert_eq!(macro_pipeline::add(20, 22), 42);
    assert_eq!(
        macro_pipeline::banner("counter".to_string(), 8),
        "counter has 8 ticks"
    );
    assert_eq!(
        PIPELINECOUNTER_LEAN_CLASS_DECL,
        "opaque PipelineCounter.ffi : NonemptyType\n\
         def PipelineCounter : Type := PipelineCounter.ffi.val\n\
         instance : Nonempty PipelineCounter := PipelineCounter.ffi.property"
    );
    assert!(PIPELINECOUNTER_LEAN_METHODS_DECL.contains("__lean_ffi_PipelineCounter_new"));
    assert!(PIPELINECOUNTER_LEAN_METHODS_DECL.contains("opaque PipelineCounter.increment"));
    assert_eq!(
        macro_pipeline::__leo3_metadata_add().name,
        "macro_pipeline_add"
    );
    let add_metadata = macro_pipeline::__leo3_metadata_add();
    assert_eq!(
        add_metadata.schema_version,
        leo3::LEO3_BINDING_SCHEMA_VERSION
    );
    assert_eq!(add_metadata.rust_name, "add");
    assert_eq!(add_metadata.ffi_symbol, "macro_pipeline_add");
    assert_eq!(add_metadata.receiver, leo3::LeanBindingReceiver::None);
    assert_eq!(add_metadata.params.len(), 2);
    assert_eq!(add_metadata.params[0].name, "a");
    assert_eq!(add_metadata.params[0].ty.lean, Some("UInt64"));
    assert_eq!(
        add_metadata.params[0].passing,
        leo3::LeanPassingStyle::Owned
    );
    assert_eq!(add_metadata.return_type.lean, Some("UInt64"));
    assert_eq!(add_metadata.semantics, leo3::LeanBindingSemantics::Value);
    assert_eq!(
        macro_pipeline::__leo3_metadata_banner().name,
        "macro_pipeline_banner"
    );
    let module_metadata = macro_pipeline::__leo3_module_metadata();
    let export_names: Vec<_> = module_metadata
        .exports
        .iter()
        .map(|item| item.name)
        .collect();
    assert_eq!(
        module_metadata.schema_version,
        leo3::LEO3_BINDING_SCHEMA_VERSION
    );
    assert_eq!(module_metadata.name, "MacroPipeline");
    assert_eq!(
        export_names,
        vec!["macro_pipeline_add", "macro_pipeline_banner"]
    );
    assert_eq!(module_metadata.exports[1].params[0].ty.lean, Some("String"));
    assert_eq!(module_metadata.exports[1].params[1].ty.lean, Some("Int32"));
    assert_eq!(module_metadata.exports[1].return_type.lean, Some("String"));

    let class_metadata = __leo3_class_metadata_PipelineCounter();
    assert_eq!(
        class_metadata.schema_version,
        leo3::LEO3_BINDING_SCHEMA_VERSION
    );
    assert_eq!(class_metadata.rust_name, "PipelineCounter");
    assert_eq!(class_metadata.lean_name, "PipelineCounter");
    assert_eq!(class_metadata.opaque_decl, PIPELINECOUNTER_LEAN_CLASS_DECL);
    assert!(class_metadata.opaque_decl.contains("NonemptyType"));
    assert!(class_metadata
        .methods_decl
        .contains("PipelineCounter.increment"));
    assert_eq!(class_metadata.methods.len(), 3);
    assert_eq!(
        class_metadata.methods[1].receiver,
        leo3::LeanBindingReceiver::MutRef
    );
    assert_eq!(
        class_metadata.methods[1].semantics,
        leo3::LeanBindingSemantics::MutatesSelf
    );
    assert_eq!(
        class_metadata.methods[2].receiver,
        leo3::LeanBindingReceiver::Ref
    );
    assert_eq!(class_metadata.methods[2].return_type.lean, Some("Int32"));

    let _init_fn: unsafe extern "C" fn(u8, *mut std::ffi::c_void) -> *mut std::ffi::c_void =
        initialize_MacroPipeline;
}

#[test]
fn test_macro_pipeline_ffi_round_trip() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> LeanResult<()> {
        // Scalar-only exports use the unboxed extern ABI end to end.
        let sum = unsafe { macro_pipeline::macro_pipeline_add(20, 22) };
        assert_eq!(sum, 42);

        let message = unsafe {
            let name = LeanString::mk(lean, "counter")?;
            // `String` crosses boxed, `i32` crosses unboxed.
            let ptr = macro_pipeline::macro_pipeline_banner(name.into_ptr(), 8);
            let message = LeanBound::<LeanString>::from_owned_ptr(lean, ptr);
            LeanString::cstr(&message)?.to_owned()
        };
        assert_eq!(message, "counter has 8 ticks");

        let value = unsafe {
            let counter_ptr = __lean_ffi_PipelineCounter_new(5);
            let counter_ptr = __lean_ffi_PipelineCounter_increment(counter_ptr, 3);
            __lean_ffi_PipelineCounter_get(counter_ptr)
        };
        assert_eq!(value, 8);

        Ok(())
    })
    .unwrap();
}
