# Vendored Typst engine

Upstream: https://github.com/Myriad-Dreamin/typst

Revision: `59b5999da8e74e74583069408d2564fc1f9bc973` (Typst 0.15.1).

This is a physical source snapshot, with no dependency on the prototype or another checkout. Upstream licensing is retained in LICENSE / NOTICE. Workspace members are restricted to the shipped crates.

The editor's transparent math mapping changes are already applied. The patch specification and final layout entry are retained at [engine-patches.json](../../native-adapter/engine-patches.json) and [layout-entry.rs](../../native-adapter/layout-entry.rs). Update these specifications together with the checked-in engine when changing mapping behavior.

No engine clone, checkout, or patch application is required during a normal build.
