"""Validate fade completion; assess cadence only with an active compositor.

motion_frame describes GPUI renders, not physical scanout. A throttled/occluded
desktop cannot establish visible smoothness. The lifecycle harness records the
Windows compositor status during each fade to make that distinction explicit.
"""
import json
import sys
from pathlib import Path

OCCLUDED = 0xC01E0006


def validate_fade(own, probes):
    reveal = next(e for e in own if e['event']=='session_reveal')
    complete = next(e for e in own if e['event']=='session_fade_complete')
    started = next((e for e in own if e['event']=='session_fade_started'), None)
    assert started is not None, 'Missing actual fade-clock instrumentation'
    origin = float(started['detail'])
    assert 'foreground=true;topmost=true' == reveal['detail'], reveal
    frames = [(json.loads(e['detail']).get('sample_ms',e['ms']),json.loads(e['detail'])['session_fade']) for e in own
              if e['event']=='motion_frame']
    assert frames and frames[0][1] == 0, frames
    # The timer can expire between renders. Find the terminal frame instead of
    # assuming it arrives within 30 ms of the timer notification.
    terminal = next((i for i,(t,v) in enumerate(frames)
                     if t >= reveal['ms'] and v == 1), None)
    assert terminal is not None, 'No fully visible frame was rendered'
    frames = frames[:terminal+1]
    assert all(a[1] <= b[1] for a,b in zip(frames,frames[1:])), frames
    mid = [(t,v) for t,v in frames if 0 < v < 1]
    duration = complete['ms']-origin
    assert 350 <= duration <= 500, duration
    during = [p for p in probes if p['pid']==reveal['pid']
              and reveal['process_ms'] <= p['process_ms'] <= complete['process_ms']]
    # Require repeated contemporaneous observations before excluding cadence.
    # Missing evidence retains the original strict check.
    occluded = len(during)>=2 and all(p['status']==OCCLUDED for p in during)
    assert frames[-1][0]-origin <= (1000 if occluded else 530), 'Fade did not finish rendering promptly'
    if not occluded:
        assert len(mid)>=8, f'Only {len(mid)} intermediate frames; probes={during}'
    return dict(pid=reveal['pid'], fade_ms=duration,
                rendered_complete_ms=frames[-1][0]-origin,
                intermediate_frames=len(mid), focus_restored=True,
                smoothness='not_measurable_compositor_occluded' if occluded else 'passed',
                compositor_probes=during)


def main(app):
    events = [json.loads(s) for s in (app/'artifacts/session.jsonl').read_text(encoding='utf-8').splitlines()]
    host = next(e['pid'] for e in reversed(events) if e['event']=='host_started')
    start = next(i for i,e in enumerate(events) if e['pid']==host and e['event']=='host_started')
    events = events[max(0,start-3):]
    probe_path = app/'artifacts/compositor-probes.json'
    probes = json.loads(probe_path.read_text()) if probe_path.exists() else []
    pids = list(dict.fromkeys(e['pid'] for e in events if e['event']=='session_reveal'))
    assert pids, 'No managed frontend was revealed'
    result = {'host_pid':host, 'frontends':[]}
    for pid in pids:
        result['frontends'].append(validate_fade([e for e in events if e['pid']==pid], probes))
    host_events = [e for e in events if e['pid']==host]
    result['game_cycles'] = sum(e['event']=='host_game_ended' for e in host_events)
    result['frontend_exits_during_game'] = sum(e['event']=='ui_exited' and 'state=Playing' in e['detail'] for e in host_events)
    result['smoothness_measured'] = all(f['smoothness']=='passed' for f in result['frontends'])
    result['passed'] = True
    (app/'artifacts/session-trace.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
    print(json.dumps(result,indent=2))


if __name__=='__main__':
    main(Path(sys.argv[1]).resolve())
