# Media quality update — 2026-11-20

The launcher now preserves source frame rates up to 60 fps and uses physical display pixels for video sizing. Decoding is capped at the source dimensions, display dimensions and 3840×2160. It does not interpolate frames or enlarge source files. A bounded 1 MiB Windows output pipe removes the transfer bottleneck seen with 4K raw frames. Source probing and decoding run off the UI thread and remain cancellable. Media still stops during gameplay.

Ten trailers were replaced at their existing Playnite paths. Publisher sources:

| Game | Installed source | Resolution / frame rate |
| --- | --- | --- |
| Forza Horizon 6 | [Forza launch trailer](https://www.youtube.com/watch?v=oYhaW-Vr4wg) | 4K / 30 |
| Cyberpunk 2077 | [Cyberpunk gameplay trailer](https://www.youtube.com/watch?v=BO8lX3hDU30) | 4K / 30 |
| NBA 2K27 | [NBA 2K gameplay trailer](https://www.youtube.com/watch?v=vQ2jNNbh1TU) | 4K / 60 |
| God of War Ragnarök | [PlayStation PC launch trailer](https://www.youtube.com/watch?v=nBwxyqq1ON4) | 4K / 60 |
| Hades II | [Supergiant launch trailer](https://www.youtube.com/watch?v=U8lJRcUeEMs) | 4K / 60 |
| HELLDIVERS 2 | [PlayStation Australia launch trailer](https://www.youtube.com/watch?v=QWKWUMPXIPw) | 4K / 30 |
| Mario Kart 8 Deluxe | [Nintendo overview](https://www.youtube.com/watch?v=ZKB9FEXhJ68) | 1080p / 60 |
| Pokémon Legends Arceus | [Nintendo overview](https://www.youtube.com/watch?v=I4RynqpahT8) | 1080p / 30 |
| Octopath Traveler II | [Square Enix trailer](https://www.youtube.com/watch?v=XMpWElRyoxg) | 1080p / 60 |
| Tears of the Kingdom | [Nintendo trailer 3](https://www.youtube.com/watch?v=uHGShqcAHlQ) | 1080p / 30 |

Rates are rounded (29.97/59.94 count as 30/60). These are source encodings, not a claim that every shot contains unique frames at that rate. Smash Ultimate, Dave the Diver, Mario 3D World and Untitled Goose Game retain their existing 1080p files because the inspected candidates did not establish an improvement. Some trailers contain publisher titles, ratings or review quotes.

Octopath's existing music had a quiet 30-second introduction. The replacement skips that introduction, removes the silent tail, and adds a 0.25-second fade in and 1-second fade out. Its duration is now 162.3 seconds. The global music volume remains 0.75.

## Validation and limits

- 38 unit tests passed; the environment-dependent decoder test is excluded from the default run.
- Four explicit release-decoder runs passed: 4K/60, 4K/30, 1440p/60 and 1080p/60. Each measured five seconds after a two-second warmup and checked shutdown within two seconds.
- Each replacement trailer decoded two-second samples at the start, middle and end. One frame per trailer was visually inspected. This is not a full playback review.
- All originals were backed up and their hashes verified before replacement. The installed files were hash-checked against the staged replacements.
- The headless decoder results do not certify GPUI presentation, Apollo streaming frame rate, long-duration audio/video synchronization, or Playnite's Windows video-codec support. The six 4K files use VP9; all replacement MP4 audio uses AAC.
- Existing still-background cache sizing is unchanged. This update concerns video and the Octopath music file.

Local provenance, hashes, measurements and backups are in `artifacts/video-upgrade/installed-manifest.json`, `decoder-results.json` and `backups/`. Downloaded media and machine-specific paths are not committed. To restore a media file, copy its manifest `backup` over its `destination` while that file is not playing. No database, game launch settings or Eden mappings were changed.
