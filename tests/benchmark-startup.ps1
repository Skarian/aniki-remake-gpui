param([int]$Runs=3)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot
$results=@()
for($i=1;$i -le $Runs;$i++){
 $timer=[Diagnostics.Stopwatch]::StartNew()
 $process=Start-Process -FilePath (Join-Path $root 'aniki-remake-gpui.exe') -ArgumentList '--benchmark' -WorkingDirectory $root -WindowStyle Normal -PassThru
 if(-not $process.WaitForExit(15000)){throw 'Benchmark app did not exit within 15 seconds'}
 $events=Get-Content -LiteralPath (Join-Path $root 'artifacts\session.jsonl') | ForEach-Object {$_ | ConvertFrom-Json} | Where-Object pid -eq $process.Id
 $frame=$events | Where-Object event -eq 'first_frame_callback' | Select-Object -First 1
 if(-not $frame){throw "Missing first-frame marker for PID $($process.Id)"}
 $results += [pscustomobject]@{Run=$i;Pid=$process.Id;ProcessToFrameMs=$frame.process_ms;MainToFrameMs=$frame.ms;ProcessToSnapshotMs=($events | Where-Object event -eq 'snapshot_loaded' | Select-Object -First 1).process_ms}
}
$results | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'artifacts\startup-results.json') -Encoding UTF8
$results | Format-Table
