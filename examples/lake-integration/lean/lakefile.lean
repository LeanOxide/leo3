import Lake
open Lake DSL

package «Leo3Example» where
  leanOptions := #[
    ⟨`autoImplicit, false⟩
  ]

@[default_target]
lean_lib «Leo3Example» where
  moreLinkArgs := #["-L", "../native/target/release", "-l", "leo3_lake_example"]

lean_exe «app» where
  root := `Main
  moreLinkArgs := #["-L", "../native/target/release", "-l", "leo3_lake_example"]
