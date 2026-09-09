# Shared prototype fonts

NewCMMath-Regular.otf is the existing New Computer Modern Math font asset,
copied from the original formula prototype without modification. See NOTICE
for its bundled third-party license notices.

multi-formula-rust serves this shared asset directly. It does not load any
lyx-rust code, WASM, page, or native adapter. The original prototype's font
files were left in place so its build/start workflow is unchanged.
