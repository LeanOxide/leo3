import ClassIntegration

/-- Fail the run (non-zero exit) when an actual value does not match. -/
def expect (what : String) (actual expected : String) : IO Unit := do
  if actual == expected then
    IO.println s!"ok   {what} = {actual}"
  else
    throw <| IO.userError s!"FAIL {what}: expected \"{expected}\", got \"{actual}\""

def main : IO Unit := do
  IO.println "== ClassIntegration.Native (#[leanmodule] + #[leanfn]) =="

  -- Scalar-only export: unboxed UInt64 in, unboxed UInt64 out.
  let sum := ci_add 20 22
  IO.println s!"ci_add(20, 22) = {sum}"
  expect "ci_add" (toString sum) "42"

  -- Mixed export: boxed String + unboxed Int32 in, boxed String out.
  let banner := ci_banner "counter" 8
  IO.println s!"ci_banner(\"counter\", 8) = {banner}"
  expect "ci_banner" banner "counter has 8 ticks"

  -- Container export: Array UInt64 in, unboxed UInt64 out.
  let total := ci_sum #[(10 : UInt64), 20, 30]
  IO.println s!"ci_sum(#[10, 20, 30]) = {total}"
  expect "ci_sum" (toString total) "60"

  IO.println "-- BISECT: Account section removed"
