$ErrorActionPreference = 'Stop'
$prototypeRoot = Split-Path -Parent $PSScriptRoot
$prototypesRoot = Split-Path -Parent $prototypeRoot
$engineRoot = Join-Path $prototypesRoot 'typst-engine'
$revision = '59b5999da8e74e74583069408d2564fc1f9bc973'
if (!(Test-Path -LiteralPath $engineRoot)) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $engineRoot) | Out-Null
    git clone --no-checkout https://github.com/Myriad-Dreamin/typst.git $engineRoot
    if ($LASTEXITCODE -ne 0) { throw '无法取得固定版本 Typst 源码' }
    git -C $engineRoot checkout --detach $revision
    if ($LASTEXITCODE -ne 0) { throw '无法检出固定的 Typst revision' }
}
$actual = git -C $engineRoot rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $actual -ne $revision) { throw 'prototypes/typst-engine 版本不匹配；请保留现有文件并检查该目录' }
$mathPath = Join-Path $engineRoot 'crates/typst-layout/src/math/mod.rs'
$libPath = Join-Path $engineRoot 'crates/typst-layout/src/lib.rs'
$marker = '// visual-typst editor bridge'
$mathSource = [IO.File]::ReadAllText($mathPath)
$originalMathSource = $mathSource
$offset = $mathSource.IndexOf($marker)
if ($offset -ge 0) { $mathSource = $mathSource.Substring(0, $offset) }
$entry = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'layout-entry.rs'))
$boxHook = '    let frame = crate::inline::layout_box('
$mappedHook = @'
    if let Some(frame) = editor_mapped_box(item, ctx, styles)? {
        ctx.push(FrameFragment::new(props, styles, frame));
        return Ok(());
    }
'@
if (!$mathSource.Contains('editor_mapped_box(item, ctx, styles)?')) {
    if (!$mathSource.Contains($boxHook)) { throw 'Typst box bridge anchor missing' }
    $mathSource = $mathSource.Replace($boxHook, $mappedHook + "`n" + $boxHook)
}
$patchedMathSource = $mathSource.TrimEnd() + "`n`n$marker`n" + $entry
if ($patchedMathSource -ne $originalMathSource) {
    [IO.File]::WriteAllText($mathPath, $patchedMathSource)
}
$libSource = [IO.File]::ReadAllText($libPath)
if (!$libSource.Contains('pub use self::math::editor_math_frame;')) {
    [IO.File]::WriteAllText($libPath, $libSource + "`n$marker`npub use self::math::editor_math_frame;`n")
}
