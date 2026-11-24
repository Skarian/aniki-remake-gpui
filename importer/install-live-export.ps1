param([string]$PlayniteRoot='D:\Playnite',[string]$Destination=(Join-Path (Split-Path $PSScriptRoot) 'cache\live.json'),[string]$PlayniteDataRoot=$PlayniteRoot)
$ErrorActionPreference='Stop'
$target=Join-Path $PlayniteDataRoot 'Extensions\LumenLiveExport_c4825127-e280-4484-9ae2-30c5a5d7a2fd'
New-Item -ItemType Directory -Path $target -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'PlayniteFullscreen.LiveExport.dll') -Destination $target -Force
$manifest=@'
Id: LumenLiveExport_c4825127-e280-4484-9ae2-30c5a5d7a2fd
Name: Playnite Fullscreen GPUI Library Export
Author: Local
Version: 0.1
Module: PlayniteFullscreen.LiveExport.dll
Type: GenericPlugin
'@
[IO.File]::WriteAllText((Join-Path $target 'extension.yaml'),$manifest)
[IO.File]::WriteAllText((Join-Path $target 'destination.txt'),$Destination)
Write-Output "Installed read-only library exporter: $target"
Write-Output 'Playnite loads this extension on its next startup.'
