param([switch]$Release, [switch]$Test)
$ErrorActionPreference='Continue'
$root=$PSScriptRoot
$env:RUSTUP_HOME=Join-Path $root '.tools\rustup'
$env:CARGO_HOME=Join-Path $root '.tools\cargo'
$env:PATH=(Join-Path $env:CARGO_HOME 'bin')+';'+$env:PATH
if($Test){ & cargo test --locked } elseif($Release){ & cargo build --release --locked } else { & cargo build }
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
