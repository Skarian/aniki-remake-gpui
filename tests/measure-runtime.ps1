param([string]$Name='idle', [int]$Seconds=5)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot
$launcher=@(Get-Process -Name aniki-remake-gpui)
if($launcher.Count -ne 1){throw 'Expected one running launcher'}
$children=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($launcher[0].Id)" | Where-Object Name -eq 'ffmpeg.exe')
$ids=@($launcher[0].Id)+@($children | ForEach-Object ProcessId)
$before=@{}
foreach($idValue in $ids){$before[$idValue]=(Get-Process -Id $idValue).TotalProcessorTime.TotalSeconds}
$timer=[Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds $Seconds
$elapsed=$timer.Elapsed.TotalSeconds
$rows=@(foreach($idValue in $ids){
 $p=Get-Process -Id $idValue
 $cpu=$p.TotalProcessorTime.TotalSeconds-$before[$idValue]
 [pscustomobject]@{Pid=$idValue;Name=$p.ProcessName;CpuSeconds=$cpu;OneCorePercent=100*$cpu/$elapsed;MachinePercent=100*$cpu/$elapsed/[Environment]::ProcessorCount;WorkingSetMiB=$p.WorkingSet64/1MB;PrivateBytesMiB=$p.PrivateMemorySize64/1MB}
})
$result=[pscustomobject]@{Mode=$Name;ElapsedSeconds=$elapsed;LogicalProcessors=[Environment]::ProcessorCount;Processes=$rows;DecoderCommand=@($children | ForEach-Object CommandLine)}
$result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root "artifacts/polish-runtime-$Name.json") -Encoding UTF8
$result | ConvertTo-Json -Depth 5
