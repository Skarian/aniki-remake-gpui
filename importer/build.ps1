param([string]$PlayniteRoot='D:\Playnite',[string]$OutputDirectory=$PSScriptRoot)
$ErrorActionPreference='Stop'
$out=$OutputDirectory
New-Item -ItemType Directory -Path $out -Force | Out-Null
$csc='C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $csc /nologo /target:exe "/out:$out\Aniki.Import.exe" "/reference:$PlayniteRoot\LiteDB.dll" "/reference:$PlayniteRoot\Newtonsoft.Json.dll" "$PSScriptRoot\Export.cs"
if($LASTEXITCODE){exit $LASTEXITCODE}
& $csc /nologo /target:library "/out:$out\Aniki.LiveExport.dll" "/reference:$PlayniteRoot\Playnite.SDK.dll" "/reference:$PlayniteRoot\LiteDB.dll" "/reference:$PlayniteRoot\Newtonsoft.Json.dll" "$PSScriptRoot\Export.cs" "$PSScriptRoot\LiveExport.cs"
if($LASTEXITCODE){exit $LASTEXITCODE}
$framework=Split-Path $csc
& $csc /nologo /target:library "/out:$out\Aniki.PluginHost.dll" "/reference:$PlayniteRoot\Playnite.SDK.dll" "/reference:$PlayniteRoot\Newtonsoft.Json.dll" /reference:System.Web.Extensions.dll /reference:System.Xaml.dll "/reference:$framework\WPF\WindowsBase.dll" "/reference:$framework\WPF\PresentationFramework.dll" "/reference:$framework\WPF\PresentationCore.dll" "$PSScriptRoot\PluginHost.cs"
exit $LASTEXITCODE
