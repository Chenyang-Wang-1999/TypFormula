# Vendored Typst engine

Upstream: https://github.com/Myriad-Dreamin/typst

Revision: `59b5999da8e74e74583069408d2564fc1f9bc973` (Typst 0.15.1).

This is a physical source snapshot, with no dependency on the prototype or another checkout. Upstream licensing is retained in LICENSE / NOTICE. Workspace members are restricted to the shipped crates.

License note: this snapshot stays under upstream's Apache-2.0, while the surrounding project is GPL-2.0-**or-later**. The combination is only valid through the "or later" route (GPLv3, which is compatible with Apache-2.0); GPLv2-only would not be. Keep both LICENSE texts with any packaged distribution, including the VSIX.

The editor's transparent math mapping changes are already applied. The patch specification and final layout entry are retained at [engine-patches.json](../../native-adapter/engine-patches.json) and [layout-entry.rs](../../native-adapter/layout-entry.rs). Update these specifications together with the checked-in engine when changing mapping behavior.

No engine clone, checkout, or patch application is required during a normal build.
