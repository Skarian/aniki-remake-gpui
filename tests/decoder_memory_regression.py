"""Verify thread caps preserve crop/pad pixels for every installed trailer.

Also decode across EOF with looping enabled. No output files or library writes.
This checks decode correctness, not presentation pacing or audio synchronization.
"""
import hashlib
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[1]
out = root / 'artifacts/memory-optimization/decoder-regression.json'
config = json.loads((root / 'aniki-remake-gpui.json').read_text(encoding='utf-8-sig'))
library = json.loads((root / 'cache/library.json').read_text(encoding='utf-8-sig'))
results = []

def decode(game, mode, threads, loop=False):
    cmd = [config['ffmpeg'], '-hide_banner', '-loglevel', 'error']
    if loop:
        cmd += ['-stream_loop', '-1', '-sseof', '-0.25']
    if threads:
        cmd += ['-threads', '2']
    cmd += ['-i', game['video']]
    if threads:
        cmd += ['-filter_threads', '2', '-threads', '2']
    sizing = ('increase:flags=lanczos,crop=1920:1080' if mode == 'background'
              else 'decrease:flags=lanczos,pad=1920:1080:(ow-iw)/2:(oh-ih)/2')
    frames = 30 if loop else 12
    cmd += ['-an', '-vf', f'scale=1920:1080:force_original_aspect_ratio={sizing},fps=30',
            '-frames:v', str(frames), '-pix_fmt', 'bgra', '-f', 'rawvideo', 'pipe:1']
    process = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, creationflags=0x08000000)
    digest = hashlib.sha256()
    count = 0
    while block := process.stdout.read(1024 * 1024):
        digest.update(block)
        count += len(block)
    process.wait(timeout=10)
    error = process.stderr.read().decode(errors='replace')
    row = dict(game=game['name'], mode=mode, bounded=threads, loop=loop,
               bytes=count, sha256=digest.hexdigest(), exit_code=process.returncode, stderr=error)
    results.append(row)
    out.write_text(json.dumps(results, indent=2), encoding='utf-8')
    assert process.returncode == 0 and count == 1920 * 1080 * 4 * frames, row
    return row

for game in library['games']:
    for mode in ['background', 'watch']:
        before = decode(game, mode, False)
        after = decode(game, mode, True)
        assert before['sha256'] == after['sha256'], (before, after)
    decode(game, 'background', True, loop=True)
    print(f"PASS crop, pad, loop: {game['name']}", flush=True)
