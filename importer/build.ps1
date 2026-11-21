param([string]$PlayniteRoot='D:\Playnite')
$ErrorActionPreference='Stop'
$out=$PSScriptRoot
$csc='C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'
foreach($dll in @('LiteDB.dll','Newtonsoft.Json.dll')){Copy-Item -LiteralPath (Join-Path $PlayniteRoot $dll) -Destination $out -Force}
& $csc /nologo /target:exe "/out:$out\Aniki.Import.exe" "/reference:$out\LiteDB.dll" "/reference:$out\Newtonsoft.Json.dll" "$out\Export.cs"
if($LASTEXITCODE){exit $LASTEXITCODE}
& $csc /nologo /target:library "/out:$out\Aniki.LiveExport.dll" "/reference:$PlayniteRoot\Playnite.SDK.dll" "/reference:$out\LiteDB.dll" "/reference:$out\Newtonsoft.Json.dll" "$out\Export.cs" "$out\LiveExport.cs"
if($LASTEXITCODE){exit $LASTEXITCODE}
$framework=Split-Path $csc
& $csc /nologo /target:library "/out:$out\Aniki.PluginHost.dll" "/reference:$PlayniteRoot\Playnite.SDK.dll" "/reference:$out\Newtonsoft.Json.dll" /reference:System.Web.Extensions.dll /reference:System.Xaml.dll "/reference:$framework\WPF\WindowsBase.dll" "/reference:$framework\WPF\PresentationFramework.dll" "/reference:$framework\WPF\PresentationCore.dll" "$out\PluginHost.cs"
exit $LASTEXITCODE
