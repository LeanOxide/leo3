//! Runtime layout assertion for Lean's `Message` object, cross-toolchain.
//!
//! `run_command`'s error reporting reads `Message` at fixed offsets
//! (`leo3/src/meta/repl.rs`): 5 object fields in declaration order
//! (`fileName`, `pos`, `endPos`, `caption`, `data`), then the scalar
//! bytes (`keepFullRange`, `severity`, `isSilent`), with `severity` at
//! byte 41 relative to the object field array. Those assumptions were
//! previously only guarded indirectly by the behavioral suite; this
//! test takes a real error-severity message produced by the elaborator
//! and asserts the layout from the **runtime object header** directly
//! (the object itself is the ground truth, not any source-tree
//! inference), so a Lean release that changes the layout fails loudly
//! here instead of making the FFI misread silently.
//!
//! Expected values (measured against the linked toolchains, stable
//! across 4.25.2 .. 4.33.0-rc1):
//!
//! - header: `m_other = 5` (object field count), `m_cs_sz = 56`
//!   (aligned object size: 8-byte header + 5 * 8 object fields + 3
//!   scalar bytes, rounded up to a multiple of 8; mimalloc builds
//!   store the aligned size in `m_cs_sz`), `m_tag = 0` (single
//!   constructor);
//! - `severity` at object-relative offset `5 * 8 + 1 = 41` is `2`
//!   (error);
//! - `caption` (object field 3) is a `String`, `data` (object field 4)
//!   is a `MessageData`, and the text the production path renders from
//!   them is non-empty and carries the failing command's token.
//!
//! Runs under the `--all-features --workspace` legs (CI's
//! `compat-runtime-matrix` / `heavy-careful` / `heavy-asan`: always on
//! non-PR events, on PRs with the `CI-build-full` label) — the same
//! gating as the other meta tests; on layout drift the assertions below
//! fail with the measured value in the message.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows")
))]

use leo3::meta::*;
use leo3::prelude::*;

/// The `Message` layout under test: 5 object fields followed by 3
/// scalar bytes. `HEADER` is the 8-byte `lean_object` header; the probe
/// reads `HEADER + 5*8 + 3` bytes total.
const NUM_OBJECT_FIELDS: usize = 5;
const NUM_SCALAR_BYTES: usize = 3;
const HEADER_SIZE: usize = 8;
const PROBE_SIZE: usize = HEADER_SIZE + NUM_OBJECT_FIELDS * 8 + NUM_SCALAR_BYTES;

#[test]
fn test_message_runtime_layout() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        // A command that fails elaboration: the elaborator records an
        // `error`-severity `Message` for the unknown constant in the
        // command log.
        let (msg, rendered) = leo3::meta::repl::test_first_error_message(
            lean,
            &metam,
            "theorem bad : unknown_constant_xyz = 1 := rfl",
        )?
        .expect("a failing command must record an error-severity message");
        let p = msg.as_ptr() as *const u8;

        // --- Object header (8 bytes):
        // { i32 m_rc; u16 m_cs_sz; u8 m_other; u8 m_tag; } ---
        let header = unsafe { std::slice::from_raw_parts(p, HEADER_SIZE) };
        let m_rc = i32::from_le_bytes(header[0..4].try_into().unwrap());
        let m_cs_sz = u16::from_le_bytes(header[4..6].try_into().unwrap());
        let m_other = header[6];
        let m_tag = header[7];
        assert!(m_rc >= 1, "Message must be a live object (m_rc = {m_rc})");
        assert_eq!(
            m_other, NUM_OBJECT_FIELDS as u8,
            "Message object-field count drifted: expected 5 \
             (fileName, pos, endPos, caption, data), got {m_other}"
        );
        assert_eq!(
            m_cs_sz, 56,
            "Message object size drifted: expected 56 (8 header + 5*8 \
             object fields + 3 scalar bytes, aligned to 8), got {m_cs_sz}"
        );
        assert_eq!(
            m_tag, 0,
            "Message constructor tag drifted: expected 0, got {m_tag}"
        );

        // The object-field and scalar probes below read `PROBE_SIZE`
        // bytes; in mimalloc builds `m_cs_sz` is the aligned allocation
        // size, so it must cover the probe region (a smaller allocation
        // means the layout shrank — fail before reading out of bounds).
        assert!(
            (m_cs_sz as usize) >= PROBE_SIZE,
            "Message allocation too small for the 5-object/3-scalar \
             layout: m_cs_sz = {m_cs_sz}, probe needs {PROBE_SIZE} bytes"
        );
        let bytes = unsafe { std::slice::from_raw_parts(p, PROBE_SIZE) };

        // --- Severity: second scalar byte, at object-relative offset
        // 5 * 8 + 1 = 41 (absolute byte 49). ---
        let severity = bytes[HEADER_SIZE + NUM_OBJECT_FIELDS * 8 + 1];
        assert_eq!(
            severity, 2,
            "error Message must have severity 2 (error) at \
             object-relative offset 41, got {severity}"
        );

        // --- caption (object field 3): a Lean String object. ---
        let caption_ptr = u64::from_le_bytes(
            bytes[HEADER_SIZE + 3 * 8..HEADER_SIZE + 4 * 8]
                .try_into()
                .unwrap(),
        ) as *const u8;
        assert!(
            !caption_ptr.is_null(),
            "caption (object field 3) must be an object, got null"
        );
        assert!(
            (caption_ptr as usize) & 1 == 0,
            "caption (object field 3) must be a Lean String object, \
             got a scalar"
        );
        let caption_text = {
            let bound = unsafe {
                LeanBound::<LeanString>::from_borrowed_ptr(lean, caption_ptr as *const _)
            };
            String::from_lean(&bound)?
        };

        // --- data (object field 4): a MessageData object. ---
        let data_ptr = u64::from_le_bytes(
            bytes[HEADER_SIZE + 4 * 8..HEADER_SIZE + 5 * 8]
                .try_into()
                .unwrap(),
        ) as *const u8;
        assert!(
            !data_ptr.is_null(),
            "data (object field 4) must be an object, got null"
        );
        assert!(
            (data_ptr as usize) & 1 == 0,
            "data (object field 4) must be a MessageData object, \
             got a scalar"
        );

        // --- The text the production path renders from fields 3/4. ---
        assert!(
            !rendered.is_empty(),
            "rendered error text must be non-empty \
             (caption = {caption_text:?})"
        );
        assert!(
            rendered.contains("unknown_constant_xyz") || rendered.contains("command failed"),
            "rendered error text must contain the failing command's \
             token, got: {rendered}"
        );
        Ok(())
    });
    result.unwrap();
}
