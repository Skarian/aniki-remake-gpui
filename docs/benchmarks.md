# Benchmarks

## Startup

The launcher shows its first frame about **0.3 seconds** after the process starts.

| Date | Build | Runs | Process start to first frame |
| --- | --- | --- | --- |
| 2026-11-08 | Pre-release polish build | 3 | 287, 293, 295 ms (median 293 ms) |
| 2026-11-08 | Pre-release controls build | 3 | 299, 312, 327 ms (median 312 ms) |
| 2026-11-20 | `4227f15`, retained-window architecture | 1 | 317 ms |

The time runs from process creation to GPUI's first frame callback for the fullscreen window. These are warm starts with an existing library cache and OS file cache, on a 14-game library. The 2026-11-20 run also logged startup focus at 518 ms and Home music playing at 985 ms.

Test machine: AMD Ryzen 9 9950X3D2, NVIDIA GeForce RTX 5090, 64 GB RAM, 3440×1440 display.

## Resources

From the same 2026-11-20 session (`4227f15`). RAM is the median working set of the launcher plus its FFmpeg helper. CPU is the mean share of the whole 32-thread CPU.

| State | RAM | CPU | GPU memory |
| --- | --- | --- | --- |
| Home, still art and music | 107 MiB | 0.1% | 104 MiB |
| Details, still art and music | 109 MiB | 0.1% | 104 MiB |
| Home, 1080p/60 trailer | 275 MiB | 1.3% | 128 MiB |
| Trailer page, 4K/60 source at 1440p | 381 MiB | 4.9% | 162 MiB |
| Status screen while a game runs | 97 MiB | 0.05% | 189 MiB |
| Details after returning, 1080p/60 trailer | 267 MiB | 1.5% | 213 MiB |

While a game runs, music and video stop and the launcher uses almost no CPU or GPU time. The game in this session was DAVE THE DIVER through Steam.

The benchmark builds loaded SDL and FFmpeg from separate files. Release v0.1.0 links SDL statically and ships its own FFmpeg build. It has not been benchmarked separately.

## Running a benchmark

`tests/benchmark-startup.ps1` starts the launcher three times in `--benchmark` mode and reports the time to first frame from its event log. `tests/measure-runtime.ps1` samples CPU and memory of a running launcher and its FFmpeg helpers. `--trace-memory` and `--trace-motion` add memory and animation traces. All output goes to the ignored `artifacts` folder.
