param([switch]$Portable,[string]$FFmpegRoot=(Join-Path $PSScriptRoot '.tools\ffmpeg'),[string]$ImporterRoot=(Join-Path $PSScriptRoot 'importer'),[string]$OutputDirectory=(Join-Path $PSScriptRoot 'dist\playnite-fullscreen-gpui'))
$ErrorActionPreference='Stop'
$root=$PSScriptRoot
if(-not (Test-Path -LiteralPath (Join-Path $ImporterRoot 'PlayniteFullscreen.PluginHost.dll'))){throw 'Missing plugin host. Run importer/build.ps1 first.'}
$promptRoot=Join-Path $root 'assets\prompts'
$prompts=Get-Content -LiteralPath (Join-Path $promptRoot 'SOURCE.json') -Raw | ConvertFrom-Json
foreach($name in $prompts.files.PSObject.Properties.Name){
 if(-not (Test-Path -LiteralPath (Join-Path $promptRoot "$name.png"))){throw "Missing input prompt: $name"}
}
if($Portable){
 $OutputDirectory=[IO.Path]::GetFullPath($OutputDirectory)
 # An explicit allowlist prevents personal configuration, caches and Playnite DLLs
 # from entering a release package. Use a fresh folder for each package.
 if(Test-Path -LiteralPath $OutputDirectory){throw 'Output folder already exists. Choose a new OutputDirectory.'}
 foreach($file in @('bin\ffmpeg.exe','bin\ffprobe.exe','LICENSE.txt')){
  if(-not (Test-Path -LiteralPath (Join-Path $FFmpegRoot $file))){throw "Missing FFmpeg build file: $file"}
 }
 New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
 $helpers=Join-Path $OutputDirectory 'importer'
 $media=Join-Path $OutputDirectory 'tools\ffmpeg'
 New-Item -ItemType Directory -Path $helpers,(Join-Path $media 'bin') | Out-Null
 foreach($file in @('PlayniteFullscreen.Import.exe','PlayniteFullscreen.PluginHost.dll')){Copy-Item -LiteralPath (Join-Path $ImporterRoot $file) -Destination $helpers}
 foreach($file in @('ffmpeg.exe','ffprobe.exe')){Copy-Item -LiteralPath (Join-Path $FFmpegRoot "bin\$file") -Destination (Join-Path $media 'bin')}
 Copy-Item -LiteralPath (Join-Path $FFmpegRoot 'LICENSE.txt') -Destination $media
 foreach($file in @('LICENSE-dav1d.txt','SOURCES.txt')){
  if(Test-Path -LiteralPath (Join-Path $FFmpegRoot $file)){Copy-Item -LiteralPath (Join-Path $FFmpegRoot $file) -Destination $media}
 }
 & (Join-Path $media 'bin\ffmpeg.exe') -version | Set-Content -LiteralPath (Join-Path $media 'build-info.txt')
 if($LASTEXITCODE -ne 0){throw 'Packaged FFmpeg did not run'}
 & (Join-Path $media 'bin\ffprobe.exe') -version | Out-Null
 if($LASTEXITCODE -ne 0){throw 'Packaged ffprobe did not run'}
 Copy-Item -LiteralPath (Join-Path $root 'target\release\playnite-fullscreen-gpui.exe') -Destination $OutputDirectory
 Copy-Item -LiteralPath (Join-Path $root 'playnite-fullscreen-gpui.example.json') -Destination (Join-Path $OutputDirectory 'playnite-fullscreen-gpui.json')
 foreach($file in @('README.md','THIRD_PARTY_NOTICES.txt')){Copy-Item -LiteralPath (Join-Path $root $file) -Destination $OutputDirectory}
 Copy-Item -LiteralPath (Join-Path $root 'docs') -Destination $OutputDirectory -Recurse
 Copy-Item -LiteralPath (Join-Path $root 'playnite-fullscreen-gpui.example.json') -Destination $OutputDirectory
 $licenses=Join-Path $root 'artifacts\licenses'
 if(Test-Path -LiteralPath $licenses){Copy-Item -LiteralPath $licenses -Destination $OutputDirectory -Recurse}
 $packedPrompts=Join-Path $OutputDirectory 'assets\prompts'
 $packedApollo=Join-Path $OutputDirectory 'assets\apollo'
 New-Item -ItemType Directory -Path $packedPrompts,$packedApollo | Out-Null
 foreach($name in $prompts.files.PSObject.Properties.Name){Copy-Item -LiteralPath (Join-Path $promptRoot "$name.png") -Destination $packedPrompts}
 foreach($file in @('LICENSE','SOURCE.json')){Copy-Item -LiteralPath (Join-Path $promptRoot $file) -Destination $packedPrompts}
 Copy-Item -LiteralPath (Join-Path $root 'assets\apollo\playnite-fullscreen-gpui-cover.png') -Destination $packedApollo
 Copy-Item -LiteralPath (Join-Path $root 'vendor\gpui\LICENSE-APACHE') -Destination (Join-Path $OutputDirectory 'LICENSE-GPUI.txt')
 $files=Get-ChildItem -LiteralPath $OutputDirectory -Recurse -File | ForEach-Object {
  [ordered]@{path=$_.FullName.Substring($OutputDirectory.TrimEnd('\').Length+1);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
 }
 $files | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'package-files.json')
 Write-Output "Portable package: $OutputDirectory"
 return
}
Copy-Item -LiteralPath (Join-Path $root 'target\release\playnite-fullscreen-gpui.exe') -Destination (Join-Path $root 'playnite-fullscreen-gpui.exe') -Force
Write-Output "Ready: $root\playnite-fullscreen-gpui.exe"
Write-Output "Apollo command: `"$root\playnite-fullscreen-gpui.exe`""
