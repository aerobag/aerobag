# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

import contextlib
import hashlib
from io import StringIO
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import collect_pixel_review as collector


class EvidenceCollectionTests(unittest.TestCase):
    def test_health_warning_joins_exact_release_and_source_hash(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = b'synthetic source archive for collector hash verification'
            sha = hashlib.sha256(source).hexdigest()
            state = f'20260911T010231Z_{sha[:16]}_png8palette'
            commit = 'abcdef123456'
            release = '2026-09-08.1'
            metrics = {collector.PREFIX+'poor_color_match_count': {'value': 6, 'severity': 'warning'},
                       collector.PREFIX+'palette_error_max': 16}
            sample = dict(sampled_at_utc='2026-09-11T01:04:00Z', metrics=metrics,
                          channel_product_states={'production': {'product_facts_key': [
                              f'/published/{release}-{commit}/build/packaged/product-facts.json']}})
            outside = dict(sample, sampled_at_utc='2026-09-11T02:00:00Z')
            journal = dict(MESSAGE=f'live-feed production published nexrad {state} changed=136 removed=0',
                           __REALTIME_TIMESTAMP=str(int(collector.timestamp('2026-09-11T01:03:45Z')*1_000_000)))
            def run(command, **kwargs):
                self.assertEqual(command[0], 'scp')
                target = Path(command[-1])
                if 'pipeline_health-' in command[1]:
                    target.write_text(json.dumps(sample)+'\n'+json.dumps(outside)+'\n')
                else:
                    self.assertTrue(command[1].endswith('/blobs/'+sha))
                    target.write_bytes(source)
                return subprocess.CompletedProcess(command, 0)
            def ssh(host, command, **kwargs):
                self.assertEqual(host, 'test-host')
                if command.startswith('journalctl'):
                    self.assertIn(release, command)
                    return (json.dumps(journal)+'\n').encode()
                if command == 'python3 -':
                    self.assertIn(b'/mnt/aerobag-data/artifacts/cache/fetch', kwargs['input'])
                    return json.dumps([dict(state_id=state, blob_exists=True, metadata=dict(sha256=sha))]).encode()
                self.assertIn(f'{commit}:', command)
                return b'synthetic deployed file'
            args = ['collect_pixel_review.py', '--capture-dir', str(root), '--host', 'test-host',
                    '--start', '2026-09-11T01:00:00Z', '--end', '2026-09-11T01:10:00Z',
                    '--local-fetch-cache', str(root/'absent-cache')]
            with patch.object(sys, 'argv', args), patch.object(collector.subprocess, 'run', run), \
                    patch.object(collector, 'ssh', ssh), contextlib.redirect_stdout(StringIO()):
                collector.main()
            frames = json.loads((root/'evidence/frames.json').read_text())
            self.assertEqual(len(frames), 1)
            self.assertEqual(frames[0]['state_id'], state)
            self.assertEqual(len(frames[0]['samples']), 1)
            self.assertEqual(frames[0]['samples'][0]['poor_count'], 6)
            self.assertEqual(frames[0]['cache_metadata']['sha256'], sha)
            self.assertEqual((root/'evidence/sources/CONUS_L2_CREF_QCD_20260911_010231.tif.gz').read_bytes(), source)


if __name__ == '__main__':
    unittest.main()
