"""Read-only loudness survey of the first 30 seconds of cached library music."""
import concurrent.futures, json, subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[1]
config=json.loads((root/'aniki-remake-gpui.json').read_text(encoding='utf-8-sig'))
games=json.loads((root/'cache/library.json').read_text(encoding='utf-8-sig'))['games']
def measure(game):
    result=subprocess.run([config['ffmpeg'],'-hide_banner','-i',game['music'],'-t','30','-af','loudnorm=I=-18:TP=-1:LRA=11:print_format=json','-f','null','-'],capture_output=True,text=True,encoding='utf-8',errors='replace')
    meter=result.stderr[result.stderr.rfind('{'):]
    return dict(game=game['name'],source=game['music'],seconds=30,levels=json.JSONDecoder().raw_decode(meter)[0])
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    results=list(pool.map(measure,games))
folder=root/'artifacts/audio-qa';folder.mkdir(parents=True,exist_ok=True)
(folder/'source-levels.json').write_text(json.dumps(results,indent=2))
for r in results: print(r['game'], r['levels']['input_i'],r['levels']['input_tp'])
