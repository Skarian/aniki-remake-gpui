param([Parameter(Mandatory)][string]$Package)
$ErrorActionPreference = 'Stop'
$Package = (Resolve-Path $Package).Path
$dlls = @(Get-ChildItem $Package -Filter *.dll -Recurse)
if ($dlls.Count -ne 1 -or $dlls[0].Name -ne 'Aniki.PluginHost.dll') { throw 'Unexpected DLLs in package' }
foreach ($entry in (Get-Content "$Package/package-files.json" -Raw | ConvertFrom-Json)) {
    if ((Get-FileHash (Join-Path $Package $entry.path)).Hash -ne $entry.sha256) { throw "Bad package hash: $($entry.path)" }
}
$config = Get-Content "$Package/aniki-remake-gpui.json" -Raw | ConvertFrom-Json
if ($config.ffmpeg -ne '' -or (Test-Path "$Package/cache")) { throw 'Package contains local state' }
$ffmpeg = "$Package/tools/ffmpeg/bin/ffmpeg.exe"
$probe = "$Package/tools/ffmpeg/bin/ffprobe.exe"
$scratch = Join-Path $env:TEMP ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory $scratch | Out-Null
# Exercise the same scaling, frame-rate, BGRA and stereo PCM outputs as the app.
& $ffmpeg -v error -f lavfi -i 'testsrc2=size=320x180:rate=60' -f lavfi -i 'sine=frequency=440:sample_rate=48000' -t 1 -c:v mpeg4 -c:a aac "$scratch/sample.mp4"
if ($LASTEXITCODE) { throw 'Media fixture encode failed' }
& $probe -v error -show_entries stream=codec_name,width,height -of json "$scratch/sample.mp4"
if ($LASTEXITCODE) { throw 'Media probe failed' }
& $ffmpeg -v error -i "$scratch/sample.mp4" -an -vf "scale=320:180:flags=lanczos,fps='min(source_fps,60)'" -pix_fmt bgra -f rawvideo "$scratch/video.raw"
if ($LASTEXITCODE -or (Get-Item "$scratch/video.raw").Length -lt 230400) { throw 'Video decode failed' }
& $ffmpeg -v error -i "$scratch/sample.mp4" -vn -af volume=0.5 -ar 48000 -ac 2 -f s16le "$scratch/audio.raw"
if ($LASTEXITCODE -or (Get-Item "$scratch/audio.raw").Length -lt 48000) { throw 'Audio decode failed' }
Write-Output 'PASS: allowlist, hashes, clean config, probe, video and audio decode'
