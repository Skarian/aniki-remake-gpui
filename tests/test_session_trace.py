"""Regression cases for trace interpretation, independent of local display state."""
import json
import unittest
from session_trace import OCCLUDED, validate_fade


def evidence(intermediate=12, terminal=400):
    def row(event, ms, detail=''):
        return dict(pid=42, event=event, ms=ms, process_ms=ms+10, detail=detail)
    rows=[row('session_fade_started',100,'100.0'),
          row('session_reveal',100,'foreground=true;topmost=true'),
          row('motion_frame',90,json.dumps(dict(session_fade=0)))]
    rows += [row('motion_frame',100+i*360/(intermediate+1),json.dumps(dict(session_fade=i/(intermediate+1))))
             for i in range(1,intermediate+1)]
    rows += [row('session_fade_complete',460),
             row('motion_frame',100+terminal,json.dumps(dict(session_fade=1)))]
    return rows


def probes(status=0):
    return [dict(pid=42, process_ms=t, status=status) for t in (200,300,400)]


class FadeTraceTests(unittest.TestCase):
    def test_focus_restore_delay_is_not_subtracted_from_fade_duration(self):
        rows=evidence()
        next(e for e in rows if e['event']=='session_reveal').update(ms=130,process_ms=140)
        self.assertEqual(validate_fade(rows,probes())['fade_ms'],360)

    def test_final_frame_after_old_30ms_cutoff(self):
        result=validate_fade(evidence(12,450),probes())
        self.assertEqual(result['rendered_complete_ms'],450)
        self.assertEqual(result['smoothness'],'passed')

    def test_low_frame_count_still_fails_on_active_display(self):
        with self.assertRaises(AssertionError): validate_fade(evidence(5),probes())

    def test_late_terminal_frame_still_fails_on_active_display(self):
        with self.assertRaises(AssertionError): validate_fade(evidence(12,800),probes())

    def test_occlusion_is_explicitly_not_a_smoothness_pass(self):
        result=validate_fade(evidence(5),probes(OCCLUDED))
        self.assertEqual(result['smoothness'],'not_measurable_compositor_occluded')

    def test_absent_or_single_probe_cannot_excuse_low_frame_count(self):
        for observed in ([],probes(OCCLUDED)[:1]):
            with self.assertRaises(AssertionError): validate_fade(evidence(5),observed)

    def test_mixed_visibility_does_not_skip_smoothness(self):
        observed=probes(OCCLUDED); observed[-1]['status']=0
        with self.assertRaises(AssertionError): validate_fade(evidence(5),observed)

    def test_occlusion_cannot_excuse_missing_terminal_frame(self):
        with self.assertRaises(AssertionError): validate_fade(evidence(5)[:-1],probes(OCCLUDED))

    def test_occlusion_cannot_excuse_nonmonotonic_fade(self):
        rows=evidence(5); rows[4]['detail']=json.dumps(dict(session_fade=.01))
        with self.assertRaises(AssertionError): validate_fade(rows,probes(OCCLUDED))


if __name__=='__main__': unittest.main()
