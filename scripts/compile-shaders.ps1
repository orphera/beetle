# Compiles crates/beetle-render/shaders/*.hlsl into embedded .cso bytecode.
# Requires fxc.exe from the Windows 10/11 SDK (developer machines only;
# the game itself never compiles shaders at runtime — ADR-026).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$shaderDir = Join-Path $root 'crates/beetle-render/shaders'
$outDir = Join-Path $shaderDir 'compiled'
New-Item -ItemType Directory -Force $outDir | Out-Null

$fxc = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\fxc.exe' -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $fxc) { throw 'fxc.exe not found. Install the Windows SDK.' }

$src = Join-Path $shaderDir 'ui2d.hlsl'
$entries = @(
    @{ Entry = 'VS_Main';   Profile = 'vs_4_0'; Out = 'ui2d_vs.cso' },
    @{ Entry = 'PS_Sprite'; Profile = 'ps_4_0'; Out = 'ui2d_ps_sprite.cso' },
    @{ Entry = 'PS_Color';  Profile = 'ps_4_0'; Out = 'ui2d_ps_color.cso' }
)
foreach ($e in $entries) {
    & $fxc.FullName /nologo /O3 /T $e.Profile /E $e.Entry /Fo (Join-Path $outDir $e.Out) $src
    if ($LASTEXITCODE -ne 0) { throw "fxc failed for $($e.Entry)" }
}
# Snapshot of the source the .cso files were built from (checked by a unit test).
Copy-Item $src (Join-Path $outDir 'ui2d.hlsl.stamp') -Force
Write-Host "Compiled shaders with $($fxc.FullName)"
