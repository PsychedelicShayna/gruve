"""Validated, deterministic scene operations. No browser or third-party dependency."""
import copy
import math
import random

TYPES = {'dot','ellipse','rectangle','diamond','polygon','line','arrow','text','card','code','path','group'}
NUMBERS = {'x','y','width','height','radius','fontSize','strokeWidth','opacity','mass','vx','vy','rest','strength','curve'}
HEADS = {'none','arrow','open','dot','diamond','bar'}
ROUTES = {'straight','curve','elbow'}
FIELDS = NUMBERS | {'id','type','title','text','color','fill','points','tags','members','from','to','pinned','body','closed','outline','head','tail','route','measured'}
DEFAULT_PHYSICS = dict(enabled=False, repulsion=1200, center=0.02, damping=0.9, collision=True, bounce=0.45)

def fresh():
    return dict(objects={}, templates={}, physics=DEFAULT_PHYSICS.copy())

def number(v, name='number'):
    if isinstance(v, bool) or not isinstance(v,(int,float)) or not math.isfinite(v) or abs(v)>1e7:
        raise ValueError(f'{name} must be a finite number with magnitude <= 10000000')
    return v

def validate_object(o):
    if set(o)-FIELDS:
        raise ValueError('unknown object fields: '+', '.join(sorted(set(o)-FIELDS)))
    if not isinstance(o.get('id'),str) or not o['id'] or len(o['id'])>120:
        raise ValueError('object id must be a nonempty string of at most 120 characters')
    if o.get('type') not in TYPES:
        raise ValueError('unknown shape type')
    for k in NUMBERS & o.keys():
        number(o[k], k)
    for k in ('width','height','radius','mass'):
        if k in o and o[k]<=0: raise ValueError(k+' must be positive')
    if not 0 <= o.get('opacity',1) <= 1: raise ValueError('opacity must be 0..1')
    for k in ('title','text','color','fill','from','to'):
        if k in o and (not isinstance(o[k],str) or len(o[k])>20000): raise ValueError(k+' must be text')
    for k in ('pinned','body','closed','outline'):
        if k in o and not isinstance(o[k],bool): raise ValueError(k+' must be boolean')
    for k in ('head','tail'):
        if k in o and o[k] not in HEADS: raise ValueError(k+' must be one of '+', '.join(sorted(HEADS)))
    if 'route' in o and o['route'] not in ROUTES: raise ValueError('route must be one of '+', '.join(sorted(ROUTES)))
    if 'measured' in o and not isinstance(o['measured'],dict): raise ValueError('measured is reported by the browser, not set')
    for k in ('tags','members'):
        if k in o and (not isinstance(o[k],list) or not all(isinstance(v,str) for v in o[k])): raise ValueError(k+' must be a string array')
    if 'points' in o:
        if not isinstance(o['points'],list) or len(o['points'])>2000: raise ValueError('points must be an array, at most 2000')
        for p in o['points']:
            if not isinstance(p,list) or len(p)!=2: raise ValueError('point must be [x,y]')
            for v in p: number(v,'point')

def select(scene, spec):
    objects=scene['objects']
    if isinstance(spec,str): spec=[spec]
    if isinstance(spec,list):
        if any(not isinstance(i,str) or i not in objects for i in spec): raise ValueError('selection contains missing object IDs')
        return list(dict.fromkeys(spec))
    if not isinstance(spec,dict): raise ValueError('select must be ID, ID list, or filter object')
    if set(spec)-{'type','tag','ids','fraction','slice','limit','roots'}: raise ValueError('unknown selection filter')
    ids=select(scene,spec['ids']) if 'ids' in spec else list(objects)
    if 'type' in spec: ids=[i for i in ids if objects[i]['type']==spec['type']]
    if 'tag' in spec: ids=[i for i in ids if spec['tag'] in objects[i].get('tags',[])]
    if spec.get('roots'):
        members={i for o in objects.values() for i in o.get('members',[])}
        ids=[i for i in ids if i not in members]
    if 'fraction' in spec:
        f=number(spec['fraction'],'fraction')
        if not 0<=f<=1: raise ValueError('fraction must be 0..1')
        ids=ids[:math.floor(len(ids)*f)]
    if 'slice' in spec:
        s=spec['slice']
        if not isinstance(s,list) or len(s)!=2 or any(v is not None and (type(v) is not int) for v in s): raise ValueError('slice must be [start,end]')
        ids=ids[s[0]:s[1]]
    if 'limit' in spec:
        if type(spec['limit']) is not int or spec['limit']<0: raise ValueError('limit must be a nonnegative integer')
        ids=ids[:spec['limit']]
    return ids

def descendants(scene, ids):
    result=set(ids)
    for i in ids:
        result.update(descendants(scene,scene['objects'][i].get('members',[])))
    return result

def check_graph(scene):
    objs=scene['objects'];owned=set()
    for o in objs.values():
        validate_object(o)
        if ('from' in o)!=('to' in o):raise ValueError('connections require both from and to')
        for field in ('from','to'):
            if field in o and o[field] not in objs: raise ValueError('missing connection endpoint '+o[field])
            if field in o and ('from' in objs[o[field]] or o[field]==o['id']):raise ValueError('connections attach to shapes or groups, not other connections')
        if 'members' in o:
            if o['type']!='group': raise ValueError('only a group may have members')
            for i in o['members']:
                if i not in objs or i==o['id'] or i in owned: raise ValueError('invalid or multiply owned group member '+i)
                owned.add(i)
    def visit(i,stack):
        if i in stack: raise ValueError('cyclic group')
        for m in objs[i].get('members',[]): visit(m,stack|{i})
    for i in objs: visit(i,set())

def operation(scene,c):
    """Mutates a private candidate; caller commits only after all validation passes."""
    if not isinstance(c,dict): raise ValueError('command must be an object')
    op=c.get('op'); objs=scene['objects']; selected=[]
    allowed={'create','set','remove','move','group','ungroup','link','animate','impulse','physics','layout','fit','view','wait','clear','define','spawn'}
    if op not in allowed: raise ValueError('unknown op '+str(op))
    for key in ('duration','stagger'):
        v=number(c.get(key,0),key)
        if not 0<=v<=10000: raise ValueError(key+' must be 0..10000 milliseconds')
    def add(o):
        o={'x':0,'y':0,**copy.deepcopy(o)}
        if 'measured' in o: raise ValueError('measured is reported by the browser, not set')
        if o.get('id') in objs: raise ValueError('duplicate id '+o['id'])
        validate_object(o);objs[o['id']]=o;selected.append(o['id'])
    if op=='create':
        if 'items' in c:
            if not isinstance(c['items'],list): raise ValueError('items must be an array')
            for o in c['items']: add(o)
        else:
            o=c.get('object',{}); count=c.get('count',1)
            if type(count) is not int or not 1<=count<=500: raise ValueError('count must be 1..500')
            rng=random.Random(c.get('seed',1));spacing=number(c.get('spacing',45));spread=number(c.get('spread',300))
            for i in range(count):
                n=copy.deepcopy(o)
                if count>1:
                    n['id']=o['id']+'-'+str(i)
                    if c.get('arrange','scatter')=='grid':
                        cols=math.ceil(math.sqrt(count));n['x']=o.get('x',0)+(i%cols)*spacing;n['y']=o.get('y',0)+(i//cols)*spacing
                    else:
                        a=rng.random()*math.tau;r=math.sqrt(rng.random())*spread;n['x']=o.get('x',0)+math.cos(a)*r;n['y']=o.get('y',0)+math.sin(a)*r
                add(n)
    elif op=='define':
        if not isinstance(c.get('name'),str) or not isinstance(c.get('items'),list): raise ValueError('define requires name and items')
        template=fresh()
        for o in c['items']:
            validate_object(o)
            if o['id'] in template['objects']: raise ValueError('duplicate template id')
            template['objects'][o['id']]=copy.deepcopy(o)
        check_graph(template);scene['templates'][c['name']]=copy.deepcopy(c['items'])
    elif op=='spawn':
        if c.get('template') not in scene['templates']: raise ValueError('unknown template')
        prefix=c.get('id');x=number(c.get('x',0));y=number(c.get('y',0))
        if not isinstance(prefix,str): raise ValueError('spawn needs id')
        items=scene['templates'][c['template']];owned={i for o in items for i in o.get('members',[])}
        for o in items:
            n=copy.deepcopy(o);n['id']=prefix+'/'+o['id'];n['x']=o.get('x',0)+x;n['y']=o.get('y',0)+y
            for field in ('from','to'):
                if field in n:n[field]=prefix+'/'+n[field]
            if 'members' in n:n['members']=[prefix+'/'+i for i in n['members']]
            add(n)
        add(dict(id=prefix,type='group',x=x,y=y,members=[prefix+'/'+o['id'] for o in items if o['id'] not in owned],outline=False,body=c.get('body',False)))
    elif op=='link':
        props=c.get('props',{})
        if not isinstance(props,dict): raise ValueError('link props must be an object')
        head={'head':'arrow'} if c.get('arrow',False) else {}
        add({**head,**props,'id':c.get('id'),'type':'arrow' if c.get('arrow',False) else 'line','from':c.get('from'),'to':c.get('to')})
    elif op=='view':
        if 'center' in c:
            if not isinstance(c['center'],list) or len(c['center'])!=2: raise ValueError('view center must be [x,y]')
            for v in c['center']: number(v,'center')
        if 'by' in c:
            if not isinstance(c['by'],list) or len(c['by'])!=2: raise ValueError('view by must be [dx,dy]')
            for v in c['by']: number(v,'by')
        if 'zoom' in c and not 0.05<=number(c['zoom'],'zoom')<=8: raise ValueError('zoom must be 0.05..8')
        if 'fit' in c and c['fit'] is not True: selected=select(scene,c['fit'])
        if not {'center','by','zoom','fit'}&set(c): raise ValueError('view needs center, by, zoom or fit')
    elif op=='physics':
        props=c.get('props',{})
        if set(props)-set(DEFAULT_PHYSICS): raise ValueError('unknown physics property')
        for k,v in props.items():
            if k in ('enabled','collision'):
                if type(v) is not bool: raise ValueError(k+' must be boolean')
            else:
                number(v,k)
                if v<0 or (k in ('damping','bounce') and v>1): raise ValueError('invalid physics coefficient')
        scene['physics'].update(props)
    elif op=='clear':
        selected=list(objs);objs.clear()
    elif op not in ('fit','wait'):
        selected=select(scene,c.get('select'))
        if op in ('set','animate'):
            props=c.get('props',{})
            if not isinstance(props,dict) or set(props)&{'id','type','members','x','y','measured'}: raise ValueError('set/animate cannot change identity, membership, position or measurements; use move/group')
            for i in selected:
                for k,v in props.items():
                    if v is None: objs[i].pop(k,None)
                    else: objs[i][k]=copy.deepcopy(v)
        elif op=='move':
            if 'to' in c and len(selected)!=1: raise ValueError('move to requires exactly one selected object; use by for many')
            delta=c.get('by')
            if 'to' in c:
                dest=c['to'];delta=[dest[0]-objs[selected[0]].get('x',0),dest[1]-objs[selected[0]].get('y',0)]
            if not isinstance(delta,list) or len(delta)!=2: raise ValueError('move needs by:[dx,dy] or to:[x,y]')
            dx,dy=[number(v) for v in delta]
            for i in descendants(scene,selected):objs[i]['x']=objs[i].get('x',0)+dx;objs[i]['y']=objs[i].get('y',0)+dy
        elif op=='remove':
            removed=descendants(scene,selected)
            removed.update(i for i,o in objs.items() if o.get('from') in removed or o.get('to') in removed)
            for i in removed:objs.pop(i,None)
            for o in objs.values():
                if 'members' in o:o['members']=[i for i in o['members'] if i not in removed]
        elif op=='group':
            if any(i in o.get('members',[]) for o in objs.values() for i in selected): raise ValueError('ungroup existing parent before regrouping')
            add({**c.get('props',{}),'id':c.get('id'),'type':'group','members':selected.copy(),'x':sum(objs[i].get('x',0) for i in selected)/max(1,len(selected)),'y':sum(objs[i].get('y',0) for i in selected)/max(1,len(selected))})
        elif op=='ungroup':
            for i in selected:
                if objs[i]['type']!='group': raise ValueError('ungroup selects groups only')
                if any(i in o.get('members',[]) for o in objs.values()): raise ValueError('ungroup parent first')
            for i in selected:objs.pop(i)
            for i,o in list(objs.items()):
                if o.get('from') in selected or o.get('to') in selected:objs.pop(i)
        elif op=='impulse':
            velocity=c.get('velocity')
            if not isinstance(velocity,list) or len(velocity)!=2:raise ValueError('impulse needs velocity:[vx,vy]')
            for i in selected:
                objs[i]['body']=True;objs[i]['vx']=objs[i].get('vx',0)+number(velocity[0]);objs[i]['vy']=objs[i].get('vy',0)+number(velocity[1])
            scene['physics']['enabled']=True
        elif op=='layout':
            rng=random.Random(c.get('seed',1));spread=number(c.get('spread',300));spacing=number(c.get('spacing',180));cols=math.ceil(math.sqrt(max(1,len(selected))))
            owned={i for o in objs.values() for i in o.get('members',[])}
            if any(i in owned for i in selected):raise ValueError('layout selects root objects only')
            for j,i in enumerate(selected):
                if c.get('mode','scatter')=='grid':x=(j%cols)*spacing;y=(j//cols)*spacing
                else:a=rng.random()*math.tau;r=math.sqrt(rng.random())*spread;x=math.cos(a)*r;y=math.sin(a)*r
                dx=x-objs[i].get('x',0);dy=y-objs[i].get('y',0)
                for m in descendants(scene,[i]):objs[m]['x']=objs[m].get('x',0)+dx;objs[m]['y']=objs[m].get('y',0)+dy
    if len(objs)>1000: raise ValueError('board limit is 1000 objects')
    check_graph(scene)
    return selected

def patch(before,after):
    upsert=[v for k,v in after['objects'].items() if before['objects'].get(k)!=v]
    fields={o['id']:[k for k,v in o.items() if before['objects'].get(o['id'],{}).get(k)!=v] for o in upsert}
    return dict(upsert=upsert,fields=fields,remove=[k for k in before['objects'] if k not in after['objects']],physics=after['physics'])
