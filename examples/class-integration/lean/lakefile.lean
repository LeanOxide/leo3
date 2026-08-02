import Lake
open Lake DSL

package «ClassIntegration» where
  leanOptions := #[
    ⟨`autoImplicit, false⟩
  ]

-- `Account` is generated at the library root by leo3-codegen (class files
-- are not namespaced), so it is listed as an extra library root next to the
-- `ClassIntegration` module tree.
@[default_target]
lean_lib «ClassIntegration» where
  roots := #[`ClassIntegration, `Account]
  moreLinkArgs := #["-L", "../native/target/release", "-l", "class_integration"]

lean_exe «app» where
  root := `Main
  moreLinkArgs := #["-L", "../native/target/release", "-l", "class_integration"]
