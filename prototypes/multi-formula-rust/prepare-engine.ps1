param([switch]$Online)
$ErrorActionPreference = 'Stop'
$engineRoot = Join-Path (Split-Path -Parent $PSScriptRoot) 'typst-engine'
$revision = '59b5999da8e74e74583069408d2564fc1f9bc973'
$patchPath = Join-Path $PSScriptRoot 'engine-patches/math-origins.patch'

if (!(Test-Path -LiteralPath $engineRoot)) {
    if (!$Online) { throw 'Shared engine is missing. Run build.cmd --online once.' }
    git clone --no-checkout https://github.com/Myriad-Dreamin/typst.git $engineRoot
    if ($LASTEXITCODE -ne 0) { throw 'Could not clone the shared Typst engine.' }
    git -C $engineRoot checkout --detach $revision
    if ($LASTEXITCODE -ne 0) { throw 'Could not check out the pinned engine revision.' }
}
$actual = git -C $engineRoot rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $actual -ne $revision) {
    throw 'Shared engine revision differs. Existing files were preserved; resolve the version manually.'
}

# Idempotent, and does not overwrite unrelated local engine changes.
# Native stderr redirection in Windows PowerShell 5 can raise NativeCommandError.
$savedPreference = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
git -C $engineRoot apply --reverse --check $patchPath *> $null
$alreadyApplied = $LASTEXITCODE -eq 0
$ErrorActionPreference = $savedPreference
if ($alreadyApplied) {
    Write-Host 'Shared Typst engine and source-origin patch are ready.'
    exit 0
}
git -C $engineRoot apply --check $patchPath
if ($LASTEXITCODE -ne 0) {
    throw 'Source-origin patch conflicts with local changes. No changes applied; inspect engine-patches/math-origins.patch.'
}
git -C $engineRoot apply $patchPath
if ($LASTEXITCODE -ne 0) { throw 'Could not apply the source-origin patch.' }
Write-Host 'Applied the opt-in source-origin patch to the shared Typst engine.'
