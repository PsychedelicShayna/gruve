"""Persistent loopback board service, version 2. Run through boardctl.py start."""
import argparse
import copy
import json
import math
import os
from pathlib import Path
import threading
import time
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse
from model import fresh, operation, patch, number, descendants

ROOT=Path(__file__).parent
parser=argparse.ArgumentParser()
parser.add_argument('--port',type=int,default=8770)
parser.add_argument('--data',type=Path,default=ROOT/'data')
parser.add_argument('--test',action='store_true',help='serve the local browser verification harness')
args=parser.parse_args()
args.data.mkdir(parents=True,exist_ok=True)
STATE=args.data/'scene.json'
lock=threading.Condition()
saved=json.loads(STATE.read_text()) if STATE.exists() else {}
scene=saved.get('scene',fresh());seq=saved.get('seq',0)
undo=saved.get('undo',[]);redo=saved.get('redo',[])
events=[];clients={};receipts={};request_ids={}

def persist():
    tmp=STATE.with_suffix('.tmp')
    tmp.write_text(json.dumps(dict(scene=scene,seq=seq,undo=undo[-30:],redo=redo[-30:])))
    os.replace(tmp,STATE)

def commit(payload):
    global scene,seq,undo,redo
    commands=payload.get('commands') if isinstance(payload,dict) and 'commands' in payload else payload
    commands=commands if isinstance(commands,list) else [commands]
    rid=payload.get('requestId') if isinstance(payload,dict) else None
    if rid and rid in request_ids:return request_ids[rid]
    if not 1<=len(commands)<=200:raise ValueError('send 1..200 commands per request')
    with lock:
        candidate=copy.deepcopy(scene);u=copy.deepcopy(undo);r=copy.deepcopy(redo);pending=[]
        for c in commands:
            before=copy.deepcopy(candidate)
            if not isinstance(c,dict):raise ValueError('command must be an object')
            op=c.get('op')
            if op in ('undo','redo'):
                src,dst=(u,r) if op=='undo' else (r,u)
                if not src:raise ValueError('nothing to '+op)
                dst.append(before);candidate=src.pop();selected=[]
            else:
                selected=operation(candidate,c)
                if candidate!=before:u.append(before);u=u[-30:];r=[]
            duration=c.get('duration',180 if op in ('create','remove') else 0)
            stagger=c.get('stagger',0)
            delta=copy.deepcopy(patch(before,candidate))
            # Semantic commands still apply when browser physics has changed positions
            # beyond the last server checkpoint, even if the server diff is empty.
            if op in ('move','layout','set','animate','impulse'):
                affected=descendants(candidate,selected) if op in ('move','layout') else selected
                known={o['id'] for o in delta['upsert']}
                for i in affected:
                    if i not in known:delta['upsert'].append(copy.deepcopy(candidate['objects'][i]))
                    fields=['x','y'] if op in ('move','layout') else ['vx','vy','body'] if op=='impulse' else list(c.get('props',{}))
                    delta['fields'][i]=list(set(delta['fields'].get(i,[]))|set(fields))
            pending.append(dict(op=op,patch=delta,selected=selected.copy(),duration=duration,stagger=stagger,by=c.get('by'),to=c.get('to'),velocity=c.get('velocity')))
        # Commit the complete validated sequence, then publish individual events.
        scene=candidate;undo=u;redo=r
        now=time.time()*1000
        for e in pending:
            seq+=1;e.update(seq=seq,accepted=now);events.append(e)
            receipts[seq]={'accepted':now,'op':e['op']}
        persist()
        with (args.data/'history.jsonl').open('a') as f:
            for c,e in zip(commands,pending):f.write(json.dumps(dict(seq=e['seq'],accepted=now,command=c))+'\n')
        lock.notify_all()
        result={'ok':True,'first':pending[0]['seq'],'last':seq,'counts':[len(e['selected']) for e in pending]}
        if rid:request_ids[rid]=result
        return result

class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def json(self,data,code=200):
        body=json.dumps(data).encode();self.send_response(code);self.send_header('Content-Type','application/json');self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def allowed(self):
        return self.headers.get('Host') in (f'127.0.0.1:{args.port}',f'localhost:{args.port}') and self.headers.get('Origin') in (None,f'http://127.0.0.1:{args.port}',f'http://localhost:{args.port}')
    def do_GET(self):
        if not self.allowed():return self.json({'error':'local origin required'},403)
        path=urlparse(self.path).path
        if path=='/state':
            with lock:return self.json(dict(seq=seq,**scene))
        if path=='/status':
            with lock:return self.json(dict(version=2,seq=seq,objects=len(scene['objects']),clients=clients,receipts={str(k):v for k,v in list(receipts.items())[-200:]}))
        if path=='/events':
            self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Cache-Control','no-cache');self.end_headers()
            with lock:initial=dict(scene=copy.deepcopy(scene),seq=seq);cursor=seq
            try:
                self.wfile.write(('event: snapshot\ndata: '+json.dumps(initial)+'\n\n').encode());self.wfile.flush()
                while True:
                    with lock:
                        batch=[e for e in events if e['seq']>cursor]
                        if not batch:lock.wait(10);batch=[e for e in events if e['seq']>cursor]
                    if not batch:self.wfile.write(b': heartbeat\n\n')
                    for e in batch:
                        self.wfile.write(('data: '+json.dumps(e)+'\n\n').encode());cursor=e['seq']
                    self.wfile.flush()
            except (BrokenPipeError,ConnectionResetError):pass
            return
        files={'/':'board.html','/board.js':'board.js','/engine.js':'engine.js','/board.css':'board.css'}
        if args.test:files['/test-browser.html']='test-browser.html'
        if path not in files:return self.json({'error':'not found'},404)
        file=ROOT/files[path];content=file.read_bytes();self.send_response(200)
        self.send_header('Content-Type','text/html' if path=='/' or path.endswith('.html') else 'text/css' if path.endswith('.css') else 'application/javascript')
        self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(content)));self.end_headers();self.wfile.write(content)
    def do_POST(self):
        global scene
        if not self.allowed():return self.json({'error':'local origin required'},403)
        try:
            length=int(self.headers.get('Content-Length','0'))
            if not 0<length<=2_000_000:raise ValueError('payload must be 1 byte..2MB')
            data=json.loads(self.rfile.read(length))
            if args.test and self.path=='/test-report':
                (args.data/'browser-report.json').write_text(json.dumps(data,indent=2))
                return self.json({'ok':True})
            if self.path=='/commands':return self.json(commit(data))
            if self.path=='/ack':
                with lock:
                    client=str(data['client'])[:120];stage=data['stage'];n=int(data['seq'])
                    clients[client]={'seen':time.time()*1000,'seq':n,'stage':stage,'visible':data.get('visible',False)}
                    if stage in ('firstFrame','done') and n in receipts:
                        timestamp=float(data['time'])
                        if not math.isfinite(timestamp):raise ValueError('invalid timestamp')
                        ms=timestamp-receipts[n]['accepted']
                        receipts[n].setdefault(client,{}).setdefault(stage+'Ms',round(ms,1))
                    if stage in ('done','checkpoint') and n==seq and data.get('visible') and isinstance(data.get('positions'),dict):
                        for i,p in data['positions'].items():
                            if i in scene['objects']:
                                for k in ('x','y','vx','vy'):
                                    if k in p:scene['objects'][i][k]=number(p[k])
                        persist()
                return self.json({'ok':True})
            return self.json({'error':'not found'},404)
        except (ValueError,KeyError,TypeError,IndexError) as e:return self.json({'error':str(e)},400)

if __name__=='__main__':ThreadingHTTPServer(('127.0.0.1',args.port),Handler).serve_forever()
