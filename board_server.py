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
from urllib.parse import urlparse, parse_qs
from model import fresh, operation, patch, number, descendants, upgrade, presets_of, VERSION

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
marks=saved.get('marks',[])
if scene.get('version')!=VERSION:
    # One-time upgrade of a v1.1 scene; the original is kept beside it.
    backup=args.data/f'scene.v{scene.get("version",2)}.json'
    if STATE.exists() and not backup.exists():backup.write_bytes(STATE.read_bytes())
    scene=upgrade(scene);undo=[];redo=[]
    marks=[m for m in marks if not m.get('target') or m['target'] in scene['objects']]
events=[];clients={};receipts={};request_ids={};marks_rev=0
MARK_BATCHES=5

def preset_summary():
    return {name:dict(params=p['params'],doc=p.get('doc',''),builtin=name not in scene.get('presets',{})) for name,p in presets_of(scene).items()}

def visible_view():
    live=[c for c in clients.values() if c.get('visible') and c.get('view')]
    return max(live,key=lambda c:c['seen'])['view'] if live else None

def add_mark(data):
    """Marks are pointers from the human to the model: numbered within the unread batch, never part of the scene."""
    global marks_rev
    x=number(data['x'],'x');y=number(data['y'],'y')
    target=data.get('target');note=data.get('note','')
    if target is not None and (not isinstance(target,str) or target not in scene['objects']): target=None
    if not isinstance(note,str) or len(note)>2000: raise ValueError('note must be text')
    unread=[m for m in marks if not m['read']]
    batch=unread[0]['batch'] if unread else (marks[-1]['batch']+1 if marks else 1)
    offset=None
    if target:
        o=scene['objects'][target];offset=[x-o.get('x',0),y-o.get('y',0)]
    m=dict(n=len(unread)+1,batch=batch,x=x,y=y,target=target,offset=offset,note=note,at=time.time()*1000,read=False)
    marks.append(m);marks_rev+=1;prune_marks();persist();lock.notify_all()
    return m

def prune_marks():
    batches=sorted({m['batch'] for m in marks})
    keep=set(batches[-MARK_BATCHES:])
    marks[:]=[m for m in marks if m['batch'] in keep]

def take_marks(consume):
    global marks_rev
    unread=[m for m in marks if not m['read']]
    if consume and unread:
        for m in unread:m['read']=True
        marks_rev+=1;persist();lock.notify_all()
    return copy.deepcopy(unread)

def clear_marks():
    global marks_rev
    marks.clear();marks_rev+=1;persist();lock.notify_all()

def note_mark(ref,note):
    global marks_rev
    if not isinstance(note,str) or len(note)>2000: raise ValueError('note must be text')
    for m in marks:
        if m['batch']==ref.get('batch') and m['n']==ref.get('n'):
            m['note']=note;marks_rev+=1;persist();lock.notify_all();return copy.deepcopy(m)
    raise ValueError('unknown mark')

def persist():
    tmp=STATE.with_suffix('.tmp')
    tmp.write_text(json.dumps(dict(scene=scene,seq=seq,undo=undo[-30:],redo=redo[-30:],marks=marks)))
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
            if op in ('move','layout','set','impulse'):
                affected=selected
                known={o['id'] for o in delta['upsert']}
                for i in affected:
                    if i not in known:delta['upsert'].append(copy.deepcopy(candidate['objects'][i]))
                    fields=['x','y'] if op in ('move','layout') else ['vx','vy','body'] if op=='impulse' else list(c.get('props',{}))
                    delta['fields'][i]=list(set(delta['fields'].get(i,[]))|set(fields))
            pending.append(dict(op=op,patch=delta,selected=selected.copy(),duration=duration,stagger=stagger,command=c))
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
        url=urlparse(self.path);path=url.path;query=parse_qs(url.query)
        if path=='/state':
            with lock:return self.json(dict(seq=seq,view=visible_view(),marks=[m for m in marks if not m['read']],**{**scene,'presets':preset_summary()}))
        if path=='/status':
            with lock:return self.json(dict(version=3,seq=seq,objects=len(scene['objects']),view=visible_view(),clients=clients,receipts={str(k):v for k,v in list(receipts.items())[-200:]}))
        if path=='/presets':
            with lock:return self.json(dict(presets=presets_of(scene)))
        if path=='/marks':
            with lock:return self.json(dict(marks=take_marks(query.get('take',['1'])[0]!='0'),view=visible_view()))
        if path=='/events':
            self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Cache-Control','no-cache');self.end_headers()
            with lock:initial=dict(scene=copy.deepcopy(scene),seq=seq,marks=copy.deepcopy(marks),presets=preset_summary());cursor=seq;marks_seen=marks_rev
            try:
                self.wfile.write(('event: snapshot\ndata: '+json.dumps(initial)+'\n\n').encode());self.wfile.flush()
                while True:
                    with lock:
                        if not [e for e in events if e['seq']>cursor] and marks_seen==marks_rev:lock.wait(10)
                        batch=[e for e in events if e['seq']>cursor]
                        marks_now=copy.deepcopy(marks) if marks_seen!=marks_rev else None;marks_seen=marks_rev
                    if not batch and marks_now is None:self.wfile.write(b': heartbeat\n\n')
                    for e in batch:
                        self.wfile.write(('data: '+json.dumps(e)+'\n\n').encode());cursor=e['seq']
                    if marks_now is not None:self.wfile.write(('event: marks\ndata: '+json.dumps(marks_now)+'\n\n').encode())
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
            if self.path=='/marks':
                with lock:
                    if data.get('clear'):clear_marks();return self.json({'ok':True})
                    if isinstance(data.get('update'),dict):return self.json(dict(ok=True,mark=note_mark(data['update'],data.get('note',''))))
                    return self.json(dict(ok=True,mark=add_mark(data)))
            if self.path=='/ack':
                with lock:
                    client=str(data['client'])[:120];stage=data['stage'];n=int(data['seq'])
                    view=data.get('view')
                    if isinstance(view,dict):view={k:number(view[k],k) for k in ('cx','cy','zoom','w','h') if k in view}
                    clients[client]={'seen':time.time()*1000,'seq':n,'stage':stage,'visible':data.get('visible',False),'view':view if view else clients.get(client,{}).get('view')}
                    if stage in ('firstFrame','done') and n in receipts:
                        timestamp=float(data['time'])
                        if not math.isfinite(timestamp):raise ValueError('invalid timestamp')
                        ms=timestamp-receipts[n]['accepted']
                        receipts[n].setdefault(client,{}).setdefault(stage+'Ms',round(ms,1))
                    if stage in ('done','checkpoint') and n==seq and data.get('visible'):
                        changed=False
                        for i,p in (data.get('positions') or {}).items():
                            o=scene['objects'].get(i)
                            if o and o.get('parent') is None and isinstance(p,dict):
                                for k in ('x','y','vx','vy'):
                                    if k in p and o.get(k)!=p[k]:o[k]=number(p[k]);changed=True
                        for i,m in (data.get('measured') or {}).items():
                            if i in scene['objects'] and isinstance(m,dict):
                                clean={k:number(m[k],k) for k in ('w','h','contentW','contentH') if k in m};clean['overflow']=bool(m.get('overflow'))
                                if scene['objects'][i].get('measured')!=clean:scene['objects'][i]['measured']=clean;changed=True
                        for i,b in (data.get('boxes') or {}).items():
                            if i in scene['objects'] and isinstance(b,dict):
                                clean={k:number(b[k],k) for k in ('x','y','w','h') if k in b}
                                if scene['objects'][i].get('box')!=clean:scene['objects'][i]['box']=clean;changed=True
                        if changed:persist()
                return self.json({'ok':True})
            return self.json({'error':'not found'},404)
        except (ValueError,KeyError,TypeError,IndexError) as e:return self.json({'error':str(e)},400)

if __name__=='__main__':ThreadingHTTPServer(('127.0.0.1',args.port),Handler).serve_forever()
