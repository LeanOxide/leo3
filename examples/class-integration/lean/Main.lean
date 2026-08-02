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

  IO.println "== Account (#[leanclass]) =="

  -- Static constructor + #[getter] accessors.
  let acct := Account.new "Alice"
  expect "Account.owner" (Account.owner acct) "Alice"
  expect "Account.balance (new)" (toString (Account.balance acct)) "0"

  -- &mut self methods: pure update threading on the Lean side.
  let acct := Account.deposit acct 100
  let acct := Account.deposit acct 50
  expect "balance after deposits" (toString (Account.balance acct)) "150"

  -- &mut self returning a value surfaces as Prod Account Bool.
  let (acct, ok) := Account.withdraw acct 30
  expect "withdraw 30 succeeds" (toString ok) "true"
  expect "balance after withdraw" (toString (Account.balance acct)) "120"

  let (acct, ok) := Account.withdraw acct 1000
  expect "overdraft refused" (toString ok) "false"
  expect "balance unchanged" (toString (Account.balance acct)) "120"

  -- #[setter] accessor.
  let acct := Account.set_balance acct 42
  expect "set_balance" (toString (Account.balance acct)) "42"

  -- &self method returning a String.
  expect "describe" (Account.describe acct) "Alice (balance: 42)"

  -- Higher-order use: the compiler routes these calls through its own
  -- boxed adapters, which unbox into the same extern symbols.
  let owners := [Account.new "Ada", Account.new "Bob"] |>.map Account.owner
  expect "map Account.owner" (toString owners) "[Ada, Bob]"

  IO.println "All checks passed."
