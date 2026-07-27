-- Lean 4 extern declarations for the leo3-lake-example native library.
-- These match the raw `extern "C"` functions in native/src/lib.rs.

@[extern "native_add"] opaque native_add : UInt64 → UInt64 → UInt64

@[extern "native_mul"] opaque native_mul : UInt64 → UInt64 → UInt64

@[extern "native_greet"] opaque native_greet : String → String
