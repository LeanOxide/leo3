import Leo3Example

def main : IO Unit := do
  let sum := native_add 20 22
  IO.println s!"native_add(20, 22) = {sum}"

  let product := native_mul 6 7
  IO.println s!"native_mul(6, 7) = {product}"

  let greeting := native_greet "Lean"
  IO.println s!"native_greet(\"Lean\") = {greeting}"

  let acc := Accumulator.new 0
  let acc := Accumulator.add acc 10
  let acc := Accumulator.add acc 32
  let total := Accumulator.get acc
  IO.println s!"Accumulator total (10 + 32) = {total}"
