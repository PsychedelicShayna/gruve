"""Local whiteboard command service. JSON commands in, SSE events out."""
import json
import threading
import time
from pathlib import Path
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler

ROOT = Path(__file__).parent
LOG = ROOT / 'events.jsonl'
events = [json.loads(line) for line in LOG.read_text().splitlines()] if LOG.exists() else []
condition = threading.Condition()
clients = 0
last_ack = 0

class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT), **kwargs)

    def log_message(self, *args):
        pass

    def reply(self, data, status=200):
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        global clients
        if self.path == '/status':
            return self.reply({'events': len(events), 'clients': clients, 'last_ack': last_ack})
        if self.path == '/events':
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Cache-Control', 'no-cache')
            self.end_headers()
            cursor = int(self.headers.get('Last-Event-ID', '0'))
            clients += 1
            try:
                while True:
                    with condition:
                        if cursor >= len(events):
                            condition.wait(timeout=15)
                        batch = events[cursor:]
                    if not batch:
                        self.wfile.write(b': heartbeat\n\n')
                    for event in batch:
                        self.wfile.write(('id: '+str(event['seq'])+'\ndata: '+json.dumps(event)+'\n\n').encode())
                        cursor = event['seq']
                    self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass
            finally:
                clients -= 1
            return
        if self.path == '/':
            self.path = '/live.html'
        super().do_GET()

    def do_POST(self):
        global last_ack
        if self.headers.get('Origin') not in (None, 'http://127.0.0.1:8769', 'http://localhost:8769'):
            return self.reply({'error': 'origin denied'}, 403)
        try:
            data = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
            if self.path == '/ack':
                last_ack = max(last_ack, int(data['seq']))
                return self.reply({'ok': True})
            if self.path != '/commands':
                return self.reply({'error': 'not found'}, 404)
            commands = data if isinstance(data, list) else [data]
            for c in commands:
                if c.get('op') not in ('create', 'update', 'remove', 'clear', 'fit'):
                    raise ValueError('op must be create, update, remove, clear, or fit')
                if c['op'] in ('create', 'update', 'remove') and not isinstance(c.get('id'), str):
                    raise ValueError('object id required')
            with condition:
                for c in commands:
                    event = {**c, 'seq': len(events)+1}
                    with LOG.open('a') as f:
                        f.write(json.dumps(event)+'\n')
                    events.append(event)
                condition.notify_all()
            self.reply({'ok': True, 'seq': len(events), 'commands': len(commands)})
        except (ValueError, TypeError, KeyError) as e:
            self.reply({'error': str(e)}, 400)

ThreadingHTTPServer(('127.0.0.1', 8769), Handler).serve_forever()
