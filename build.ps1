param([switch]$Release, [switch]$Test)
$ErrorActionPreference='Continue'
$root=$PSScriptRoot
if(Test-Path -LiteralPath (Join-Path $root '.tools\cargo\bin\cargo.exe')){
 $env:RUSTUP_HOME=Join-Path $root '.tools\rustup'
 $env:CARGO_HOME=Join-Path $root '.tools\cargo'
 $env:PATH=(Join-Path $env:CARGO_HOME 'bin')+';'+$env:PATH
}
if(-not (Get-Command cmake -ErrorAction SilentlyContinue)){
 $vswhere=Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
 if(Test-Path -LiteralPath $vswhere){
  $cmake=& $vswhere -latest -products '*' -find 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe' | Select-Object -First 1
  if($cmake){$env:PATH=(Split-Path $cmake)+';'+$env:PATH}
 }
}
if($Test){ & cargo test --locked } elseif($Release){ & cargo build --release --locked } else { & cargo build }
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
