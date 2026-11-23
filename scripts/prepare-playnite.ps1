$ErrorActionPreference = 'Stop'
# Compiler/test references only. None of this directory enters the release ZIP.
$root = Join-Path $PSScriptRoot '..\.tools\playnite'
$archive = Join-Path $PSScriptRoot '..\.tools\playnite-10.62.7z'
New-Item -ItemType Directory -Path $root -Force | Out-Null
if (-not (Test-Path $archive)) {
    Invoke-WebRequest 'https://github.com/JosefNemec/Playnite/releases/download/10.62/10.62.7z' -OutFile $archive
}
if ((Get-FileHash $archive).Hash -ne 'A31CD00E9942619A1EFDC5C8474C3EED20DE5F8F1C31F0B1352E336E941E0D1F') {
    throw 'Playnite reference archive checksum mismatch'
}
& 7z x $archive "-o$root" -y | Out-Null
if ($LASTEXITCODE) { throw 'Playnite extraction failed' }
& "$PSScriptRoot\..\importer\build.ps1" -PlayniteRoot $root
if ($LASTEXITCODE) { throw 'Managed helper build failed' }
