/-
W-359 vanilla repro on a stock Lean 4.33 toolchain (no leo3 code involved).

Mirrors leo3's run_command_core flow: each call builds a fresh
`Command.Context` + fresh `Command.State` from the SAME base environment
(the base env is never advanced), then runs `elabCommandTopLevel` on the
parsed command. Measures RSS growth for `#check` (no addDecl) and
`axiom` (addDecl + kernel check).

The benchmark runs as a top-level command so it can capture the current
(base) environment via `getEnv` and keep using it for every iteration.

Loop semantics: each iteration's output environment is DROPPED
immediately (`discard`), mirroring the REPL/caller behavior; only the
fixed base env is kept alive across the loop.
-/
import Lean

open Lean
open Lean.Elab.Command
open Lean.Parser

def rssBytes : IO UInt64 := do
  let s ← IO.FS.readFile "/proc/self/statm"
  let pages := match s.splitOn " " with
    | _ :: p :: _ => p.toNat? |>.getD 0
    | _ => 0
  return pages.toUInt64 * 4096

def runOnce (env : Lean.Environment) (src : String) : IO Lean.Environment := do
  let ctx : Context := {
    fileName := "<stdin>",
    fileMap := FileMap.ofString src,
    ref := Syntax.missing,
    snap? := none,
    cancelTk? := none,
  }
  let st : State := { env := env, maxRecDepth := 1000 }
  let ictx := InputContext.mk src "<stdin>"
  let scope := st.scopes.head!
  let pmctx := { env := st.env, options := scope.opts,
                 currNamespace := scope.currNamespace, openDecls := scope.openDecls }
  let (cmd, _, _) := parseCommand ictx pmctx {} st.messages
  let (_, st') ← ((elabCommandTopLevel cmd) ctx).run st
      |>.toIO (fun _ => IO.userError "elaboration threw an exception")
  return st'.env

syntax (name := benchW359Cmd) "benchW359" : command

@[command_elab benchW359Cmd]
meta def elabBenchW359 : CommandElab
  | _ => do
    let env₀ ← getEnv
    -- Verify the axiom path really elaborates and adds a decl.
    let envAx ← runOnce env₀ "axiom XVERIFY : Nat"
    let okAx := (envAx.find? (Name.mkSimple "XVERIFY")).isSome
    IO.eprintln s!"[w359-vanilla] VERIFY axiom added={okAx}"
    let r0 ← rssBytes
    IO.eprintln s!"[w359-vanilla] base env captured, rss0={r0}"
    for i in [0:200] do
      let src := if i < 100 then "#check 1" else s!"axiom X{i} : Nat"
      discard (runOnce env₀ src)
      if i % 50 == 0 then
        let r ← rssBytes
        IO.eprintln s!"[w359-vanilla] i={i} rss={r} (delta={r - r0})"
    let r1 ← rssBytes
    IO.eprintln s!"[w359-vanilla] done: 100 #check + 100 axiom, total growth={r1 - r0} bytes"

benchW359
