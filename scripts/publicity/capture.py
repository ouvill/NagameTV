"""Capture the unmodified application inside the validated private GUI session."""
from contextlib import ExitStack
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import json
import os
import subprocess
import threading
import time
import fixture
import schedule
import shutil
ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'build/publicity'
OUTPUT = BASE / 'captures'
WIDTH, HEIGHT = (1440, 810)
JST = schedule.JST
NAMES = ['ながめシネマ', 'そよかぜテレビ', 'くらしチャンネル', 'アトリエTV', 'みらいサイエンス', 'まちかど放送', 'BSながめ', 'BSトラベル']
COLORS = ['#94b99a', '#8bb9c9', '#d2b88c', '#b29bc8', '#83babb', '#c99693', '#a6b7ca', '#b7c18b']

def catalog():
    return [dict(id=i + 1, serviceId=schedule.SERVICE_ID, networkId=schedule.NETWORK_BASE + i, name=name, type=1, remoteControlKeyId=i + 1, hasLogoData=True, channel=dict(type='GR' if i < 6 else 'BS', channel=str(i + 1))) for i, name in enumerate(NAMES)]

class DemoServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, now):
        self.stopping = threading.Event()
        self.schedule = schedule.programs(now)
        super().__init__(('127.0.0.1', 0), Handler)

class Handler(BaseHTTPRequestHandler):

    def log_message(self, *args):
        pass

    def do_GET(self):
        try:
            if self.path == '/api/services':
                self.respond(json.dumps(catalog(), ensure_ascii=False).encode())
            elif self.path == '/api/programs':
                self.respond(json.dumps(self.server.schedule, ensure_ascii=False).encode())
            elif self.path.endswith('/logo'):
                index = int(self.path.split('/')[3]) - 1
                svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="128" height="72"><rect width="128" height="72" rx="14" fill="{COLORS[index]}"/><text x="64" y="51" text-anchor="middle" font-family="sans-serif" font-size="44" font-weight="700" fill="#18261c">N{index + 1}</text></svg>'
                self.respond(svg.encode(), 'image/svg+xml')
            elif self.path.endswith('/stream') and '/services/' in self.path:
                self.send_response(200)
                self.send_header('Content-Type', 'video/MP2T')
                self.end_headers()
                fixture.stream(BASE / 'Big Buck Bunny.ts', self.wfile, self.server.stopping)
                self.server.stopping.wait(180)
            elif self.path.startswith('/api/events/stream'):
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.end_headers()
                self.wfile.write(b'[')
                self.wfile.flush()
                while not self.server.stopping.wait(1):
                    self.wfile.write(b' ')
                    self.wfile.flush()
            else:
                self.send_error(404)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def respond(self, body, content_type='application/json'):
        self.send_response(200)
        self.send_header('Content-Type', content_type)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

def run(*args):
    return subprocess.run([str(a) for a in args], check=True, text=True, capture_output=True, timeout=20).stdout.strip()

def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()

def main():
    session = Path(os.environ['NAGAMETV_GUI_SESSION_DIR'])
    if os.environ['DISPLAY'] != (session / 'display').read_text().strip():
        raise RuntimeError('Use scripts/run-gui-tests.py for a validated private session')
    OUTPUT.mkdir(parents=True, exist_ok=True)
    config = Path(os.environ['XDG_CONFIG_HOME']) / 'nagametv'
    config.mkdir(parents=True, exist_ok=True)
    now = datetime.now(JST)
    server = DemoServer(now)
    fixture.author(BASE / 'demo.ts', BASE / 'Big Buck Bunny.ts', now,
                   schedule.on_air_pair(server.schedule, now))
    url = f'http://127.0.0.1:{server.server_port}'
    (config / 'settings.toml').write_text(f'language = "ja"\nserver = "{url}"\nservice_id = "1"\nautoplay = true\ntimeshift = "off"\ncomments_enabled = false\n')
    env = {key: value for key, value in os.environ.items() if not key.startswith('NAGAMETV_') or key in ('NAGAMETV_GUI_SESSION_DIR', 'NAGAMETV_AUDIO_SINK')}
    env.update(NAGAMETV_DIAGNOSTICS='0', NAGAMETV_REMOTE_ENABLED='0', TZ='Asia/Tokyo')
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with ExitStack() as stack:
            log = stack.enter_context((BASE / 'app.log').open('w'))
            app = subprocess.Popen([str(ROOT / 'build/nagametv')], env=env, stdout=log, stderr=log)
            stack.callback(stop, app)
            deadline = time.monotonic() + 20
            window = None
            while time.monotonic() < deadline:
                result = subprocess.run(['xdotool', 'search', '--onlyvisible', '--pid', str(app.pid)], capture_output=True, text=True)
                if result.returncode == 0:
                    window = result.stdout.splitlines()[0]
                    break
                if app.poll() is not None:
                    raise RuntimeError('Application exited; see app.log')
                time.sleep(0.1)
            if window is None:
                raise RuntimeError('Window did not appear')
            run('xdotool', 'windowsize', '--sync', window, WIDTH, HEIGHT)
            run('xdotool', 'windowmove', '--sync', window, 0, 0)
            run('xdotool', 'windowactivate', '--sync', window)
            time.sleep(4)
            run('xdotool', 'mousemove', '--window', window, WIDTH - 100, HEIGHT // 2)
            time.sleep(0.2)

            def key(name):
                run('xdotool', 'key', '--clearmodifiers', name)
                time.sleep(0.4)

            def reveal():
                run('xdotool', 'mousemove', '--window', window, WIDTH - 100, HEIGHT // 2)
                run('xdotool', 'mousemove', '--window', window, WIDTH - 101, HEIGHT // 2)

            def shot(name):
                run('import', '-window', window, OUTPUT / f'{name}.png')
                print(f'Captured {name}', flush=True)

            reveal()
            time.sleep(1)
            shot('01-live')
            key('s')
            time.sleep(0.5)
            shot('02-channels')
            key('Escape')
            key('g')
            time.sleep(0.8)
            shot('03-guide')
            key('Return')
            time.sleep(0.5)
            shot('03-guide-details')
            key('Escape')
            key('Escape')
            key('ctrl+o')
            key('ctrl+l')
            run('xdotool', 'type', '--clearmodifiers', '--delay', '1', BASE / 'Big Buck Bunny.ts')
            key('Return')
            time.sleep(4)
            key('space')
            key('Right')
            time.sleep(2)
            reveal()
            shot('04-recording')

    finally:
        server.stopping.set()
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
    published = ROOT / 'docs/media'
    published.mkdir(parents=True, exist_ok=True)
    for source, target in [('01-live', 'live'), ('02-channels', 'channels'),
                           ('03-guide', 'guide'), ('03-guide-details', 'guide-details'),
                           ('04-recording', 'recording')]:
        shutil.copyfile(OUTPUT / f'{source}.png', published / f'{target}.png')
if __name__ == '__main__':
    main()
