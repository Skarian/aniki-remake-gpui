$ErrorActionPreference = 'Stop'
$out = Join-Path $PSScriptRoot '..\artifacts\licenses'
New-Item -ItemType Directory -Path $out -Force | Out-Null
$metadata = cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json
if ($LASTEXITCODE) { throw 'Cannot read Cargo metadata' }
$manifest = foreach ($package in $metadata.packages) {
    $directory = Split-Path $package.manifest_path
    $destination = Join-Path $out "$($package.name)-$($package.version)"
    $notices = @(Get-ChildItem $directory | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)' })
    if ($notices.Count) {
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        $notices | Copy-Item -Destination $destination -Recurse
    }
    [ordered]@{ name=$package.name; version=$package.version; license=$package.license; repository=$package.repository }
}
$manifest | ConvertTo-Json | Set-Content (Join-Path $out 'dependencies.json')
