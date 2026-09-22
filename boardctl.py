#!/usr/bin/env python3
"""Small command driver. No dependencies. JSON can be supplied as an argument or stdin."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time
import urllib.request
import urllib.error

ROOT=Path(__file__).parent
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--port',type=int,default=8770)
p.add_argument('action',choices=['start','send','state','status','open'])
p.add_argument('json',nargs='?')
p.add_argument('--wait',action='store_true',help='wait for a browser completion acknowledgment, up to 10 seconds')
a=p.parse_args();url=f'http://127.0.0.1:{a.port}'
def get(path):return json.load(urllib.request.urlopen(url+path,timeout=2))
try:
    if a.action=='start':
        try:status=get('/status')
        except (OSError,urllib.error.URLError):
            data=ROOT/'data';data.mkdir(exist_ok=True)
            with (data/'server.log').open('ab') as out:
                subprocess.Popen([sys.executable,str(ROOT/'board_server.py'),'--port',str(a.port)],stdin=subprocess.DEVNULL,stdout=out,stderr=out,start_new_session=True)
            for _ in range(30):
                try:status=get('/status');break
                except (OSError,urllib.error.URLError):time.sleep(.1)
            else:raise RuntimeError('server did not start; read data/server.log')
        if status.get('version')!=2:raise RuntimeError('port is occupied by another service')
        print(json.dumps({'url':url,'version':status['version'],'objects':status['objects']}))
    elif a.action=='open':subprocess.run(['xdg-open',url],check=True)
    elif a.action in ('state','status'):print(json.dumps(get('/'+a.action)))
    else:
        payload=json.loads(a.json if a.json else sys.stdin.read())
        body=json.dumps(payload).encode()
        result=json.load(urllib.request.urlopen(urllib.request.Request(url+'/commands',data=body,headers={'Content-Type':'application/json'}),timeout=3))
        if a.wait:
            end=time.monotonic()+10
            while time.monotonic()<end:
                receipt=get('/status')['receipts'].get(str(result['last']),{})
                if any(isinstance(v,dict) and 'doneMs' in v for v in receipt.values()):result['receipt']=receipt;break
                time.sleep(.025)
            else:result['warning']='No completion acknowledgment within 10 seconds'
        print(json.dumps(result))
except urllib.error.HTTPError as e:
    print(e.read().decode(),file=sys.stderr);sys.exit(1)
except Exception as e:
    print(str(e),file=sys.stderr);sys.exit(1)
