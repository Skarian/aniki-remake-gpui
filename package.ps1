$ErrorActionPreference='Stop'
$root=$PSScriptRoot
$promptRoot=Join-Path $root 'assets\prompts'
$prompts=Get-Content -LiteralPath (Join-Path $promptRoot 'SOURCE.json') -Raw | ConvertFrom-Json
foreach($name in $prompts.files.PSObject.Properties.Name){
 if(-not (Test-Path -LiteralPath (Join-Path $promptRoot "$name.png"))){throw "Missing input prompt: $name"}
}
if(-not (Test-Path -LiteralPath (Join-Path $root 'SDL2.dll'))){throw 'Missing 64-bit SDL2.dll controller runtime. See README.md.'}
$bytes=[IO.File]::ReadAllBytes((Join-Path $root 'SDL2.dll'))
$header=[BitConverter]::ToInt32($bytes,0x3c)
if([BitConverter]::ToUInt16($bytes,$header+4) -ne 0x8664){throw 'SDL2.dll must be the x64 build, not the 32-bit Playnite DLL.'}
Copy-Item -LiteralPath (Join-Path $root 'target\release\aniki-remake-gpui.exe') -Destination (Join-Path $root 'aniki-remake-gpui.exe') -Force
Write-Output "Ready: $root\aniki-remake-gpui.exe"
Write-Output "Apollo command: `"$root\aniki-remake-gpui.exe`""
