"""Measure speaker loopback only (never the microphone), without changing volume.
python tests/audio_probe.py label [seconds]. Keeps local WAV + measured levels.
"""
import json, sys, subprocess, time
from pathlib import Path
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'.tools/python'))
sys.coinit_flags=0  # Match SoundCard's MTA apartment for read-only session meters.
import numpy as np
import soundcard as sc
import soundfile as sf
from pycaw.pycaw import AudioUtilities
device=sc.default_speaker()
sessions=[]
for s in AudioUtilities.GetAllSessions():
    sessions.append(dict(process=s.Process.name() if s.Process else 'system',pid=s.ProcessId,volume=s.SimpleAudioVolume.GetMasterVolume(),muted=bool(s.SimpleAudioVolume.GetMute())))
print(json.dumps(dict(device=device.name,sessions=sessions)),flush=True)
if len(sys.argv)<2: sys.exit(0)
folder=root/'artifacts/audio-qa';folder.mkdir(parents=True,exist_ok=True)
label=sys.argv[1]; seconds=float(sys.argv[2]) if len(sys.argv)>2 else 12
assert label.replace('-','').replace('_','').isalnum()
loopback=sc.get_microphone(id=device.id,include_loopback=True)
assert loopback.isloopback
audio=loopback.record(samplerate=48000,numframes=int(seconds*48000))
wav=folder/(label+'.wav');sf.write(wav,audio,48000,subtype='FLOAT')
config=json.loads((root/'aniki-remake-gpui.json').read_text(encoding='utf-8-sig'))
result=subprocess.run([config['ffmpeg'],'-hide_banner','-i',str(wav),'-af','loudnorm=I=-18:TP=-1:LRA=11:print_format=json','-f','null','-'],capture_output=True,text=True)
meter=result.stderr[result.stderr.rfind('{'):].strip()
data=dict(label=label,seconds=seconds,device=device.name,sessions=sessions,peak_dbfs=float(20*np.log10(max(np.abs(audio).max(),1e-12))),rms_dbfs=float(20*np.log10(max(np.sqrt(np.mean(audio**2)),1e-12))),loudness=json.JSONDecoder().raw_decode(meter)[0] if meter.startswith('{') else result.stderr[-500:])
(folder/(label+'.json')).write_text(json.dumps(data,indent=2))
print(json.dumps(data,indent=2),flush=True)
