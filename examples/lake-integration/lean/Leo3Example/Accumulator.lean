-- Lean 4 extern declarations for the Accumulator (state-passing style).
-- The Rust side uses plain i64 as state, so Lean uses Int64 directly.

@[extern "native_accumulator_new"] opaque Accumulator.new : Int64 → Int64

@[extern "native_accumulator_add"] opaque Accumulator.add : Int64 → Int64 → Int64

@[extern "native_accumulator_get"] opaque Accumulator.get : Int64 → Int64
