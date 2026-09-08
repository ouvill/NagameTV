#!/usr/bin/env python3
"""Test flag resolution only; never start Qt, playback or profilers."""
import itertools
import os
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parent.parent
NAMES = ('MIRAKURUN_DIAGNOSTICS', 'MIRAKURUN_GC_LOG', 'MIRAKURUN_HEAPTRACK')

class LogOptions(unittest.TestCase):
    def run_options(self, launcher, args=(), settings=None):
        env = {k:v for k,v in os.environ.items() if k not in NAMES}
        env.update(settings or {})
        return subprocess.run(['bash', f'scripts/{launcher}.sh', *args, '--print-log-settings'],
                              cwd=ROOT, env=env, text=True, capture_output=True)

    def test_defaults(self):
        for launcher, expected in [('run-viewer','1 0 0'),('profile-memory','1 1 1')]:
            result=self.run_options(launcher)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(' '.join(x.split('=')[1] for x in result.stdout.split()),expected)

    def test_every_switch_combination_and_cli_precedence(self):
        for launcher in ('run-viewer','profile-memory'):
            for bits in itertools.product('01',repeat=3):
                expected=f'diagnostics={bits[0]} gc-log={bits[1]} heaptrack={bits[2]}\n'
                env=dict(zip(NAMES,bits))
                result=self.run_options(launcher,settings=env)
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertEqual(result.stdout,expected)
                args=[f'--{flag}={bit}' for flag,bit in zip(('diagnostics','gc-log','heaptrack'),bits)]
                result=self.run_options(launcher,args,dict(zip(NAMES,('1' if b=='0' else '0' for b in bits))))
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertEqual(result.stdout,expected)

    def test_invalid_settings_fail_before_device_access(self):
        for launcher in ('run-viewer','profile-memory'):
            for args,env in [(['--gc-log=wrong'],{}),(['--diagnostics'],{}),([],{'MIRAKURUN_HEAPTRACK':''})]:
                result=self.run_options(launcher,args,env)
                self.assertEqual(result.returncode,2,result.stderr)
                self.assertEqual(result.stdout,'')

    def test_feature_isolation(self):
        self.assertIn('diagnostics=0',self.run_options('run-viewer',['--features=epg']).stdout)
        self.assertIn('diagnostics=1',self.run_options('run-viewer',['--features=epg','--diagnostics=1']).stdout)

    def test_remaining_arguments_preserve_boundaries(self):
        result=subprocess.run(['bash','-c','source scripts/log-options.sh; parse_log_options run "$@"; printf "%s\\0" "${viewer_args[@]}"',
                               'test','--diagnostics=0','--features=epg','a b','--gc-log=1'],cwd=ROOT,capture_output=True,check=True)
        self.assertEqual(result.stdout,b'--features=epg\0a b\0')

if __name__=='__main__':
    unittest.main()
