"""Validated, deterministic scene operations for the v1.2 primitive/composite board.

No browser or third-party dependency. The scene is a flat registry of objects; groups own
`children` (ids) positioned in the group's local coordinates. Presets expand into primitives.
"""
import copy
import json
import math
import random
import re
from pathlib import Path

VERSION = 3
PRIMITIVES = {'rect', 'ellipse', 'polygon', 'polyline', 'text', 'edge', 'group'}
HEADS = {'none', 'arrow', 'open', 'dot', 'diamond', 'bar'}
ROUTES = {'straight', 'curve', 'elbow'}
SIDES = {'left', 'right', 'top', 'bottom'}
FONTS = {'sans', 'mono'}
ALIGNS = {'left', 'center', 'right'}
# Paint values reach SVG fill/stroke attributes: colours only, never url(...) references.
COLOR = re.compile(r'#[0-9a-fA-F]{3,8}|[a-zA-Z]{3,30}|(?:rgb|rgba|hsl|hsla)\([0-9.,%\s/+-]{1,80}\)')
LAYOUTS = {'stack', 'row', 'grid'}

COMMON = {'id', 'type', 'x', 'y', 'z', 'tags', 'opacity', 'pinned', 'body', 'mass', 'vx', 'vy', 'parent', 'preset', 'params', 'overridden', 'box', 'measured'}
STYLE = {'color', 'fill', 'strokeWidth', 'dash'}
FIELDS = {
    'rect': COMMON | STYLE | {'w', 'h', 'rx'},
    'ellipse': COMMON | STYLE | {'w', 'h', 'r'},
    'polygon': COMMON | STYLE | {'points', 'smooth'},
    'polyline': COMMON | STYLE | {'points', 'smooth', 'head', 'tail'},
    'text': COMMON | STYLE | {'w', 'h', 'text', 'size', 'font', 'weight', 'align'},
    'edge': COMMON | STYLE | {'from', 'to', 'route', 'curve', 'head', 'tail', 'label', 'rest', 'strength'},
    'group': COMMON | STYLE | {'w', 'h', 'children', 'layout', 'padding', 'outline', 'title'},
}
NUMBERS = {'x', 'y', 'z', 'opacity', 'mass', 'vx', 'vy', 'rx', 'r', 'strokeWidth', 'size', 'weight', 'curve', 'rest', 'strength', 'padding'}
INSTANCE_FIELDS = {'id', 'x', 'y', 'z', 'tags', 'opacity', 'pinned', 'body', 'mass', 'vx', 'vy'}
READ_ONLY = {'box', 'measured', 'parent', 'preset', 'params', 'overridden'}
DEFAULT_PHYSICS = dict(enabled=False, repulsion=1200, center=0.02, damping=0.9, collision=True, bounce=0.45)
BUILTIN_PRESETS = json.loads((Path(__file__).parent / 'presets.json').read_text())


def fresh():
    return dict(version=VERSION, objects={}, presets={}, physics=DEFAULT_PHYSICS.copy())


def presets_of(scene):
    return {**BUILTIN_PRESETS, **scene.get('presets', {})}


# ---------------------------------------------------------------- validation

def number(v, name='number'):
    if isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) or abs(v) > 1e7:
        raise ValueError(f'{name} must be a finite number with magnitude <= 10000000')
    return v


def size(v, name, allowed):
    if isinstance(v, str):
        if v not in allowed:
            raise ValueError(f'{name} must be a positive number' + ''.join(f' or "{a}"' for a in sorted(allowed)))
        return v
    number(v, name)
    if v <= 0:
        raise ValueError(f'{name} must be positive')
    return v


def points(v, name='points', minimum=2):
    if not isinstance(v, list) or not minimum <= len(v) <= 2000:
        raise ValueError(f'{name} must be an array of at least {minimum} and at most 2000 points')
    for p in v:
        if not isinstance(p, list) or len(p) not in (2, 3):
            raise ValueError('point must be [x,y]')
        for c in p:
            number(c, 'point')


def anchor(v, name):
    if isinstance(v, str):
        return
    if isinstance(v, list):
        if len(v) not in (2, 3):
            raise ValueError(f'{name} point must be [x,y] or [x,y,z]')
        for c in v:
            number(c, name)
        return
    if not isinstance(v, dict) or not isinstance(v.get('id'), str):
        raise ValueError(f'{name} must be an id, {{"id",...}}, [x,y] or [x,y,z]')
    extra = set(v) - {'id', 'side', 'offset', 'at', 'vertex'}
    if extra:
        raise ValueError(f'unknown {name} anchor fields: ' + ', '.join(sorted(extra)))
    if 'side' in v and v['side'] not in SIDES:
        raise ValueError(f'{name}.side must be left, right, top or bottom')
    if 'offset' in v and not 0 <= number(v['offset'], name + '.offset') <= 1:
        raise ValueError(f'{name}.offset must be 0..1')
    if 'at' in v:
        if not isinstance(v['at'], list) or len(v['at']) != 2 or any(not 0 <= number(c, name + '.at') <= 1 for c in v['at']):
            raise ValueError(f'{name}.at must be [fx,fy] with fractions 0..1')
    if 'vertex' in v and (type(v['vertex']) is not int or v['vertex'] < 0):
        raise ValueError(f'{name}.vertex must be a nonnegative integer')


def anchor_id(v):
    return v if isinstance(v, str) else v.get('id') if isinstance(v, dict) else None


def validate_object(o):
    t = o.get('type')
    if t not in PRIMITIVES:
        raise ValueError(f'unknown type {t!r}; primitives are ' + ', '.join(sorted(PRIMITIVES)))
    if not isinstance(o.get('id'), str) or not o['id'] or len(o['id']) > 160:
        raise ValueError('object id must be a nonempty string of at most 160 characters')
    extra = set(o) - FIELDS[t]
    if extra:
        raise ValueError(f'unknown {t} fields: ' + ', '.join(sorted(extra)))
    for k in NUMBERS & o.keys():
        number(o[k], k)
    for k in ('mass', 'r', 'strokeWidth', 'size'):
        if k in o and o[k] <= 0:
            raise ValueError(k + ' must be positive')
    if not 0 <= o.get('opacity', 1) <= 1:
        raise ValueError('opacity must be 0..1')
    for k in ('text', 'color', 'fill', 'label', 'title'):
        if k in o and (not isinstance(o[k], str) or len(o[k]) > 20000):
            raise ValueError(k + ' must be text')
    for k in ('color', 'fill'):
        if k in o and not COLOR.fullmatch(o[k]):
            raise ValueError(f'{k} must be a colour: #rgb, #rrggbb, a colour name, rgb()/hsl(), or none')
    for k in ('pinned', 'body', 'smooth', 'outline', 'overridden'):
        if k in o and not isinstance(o[k], bool):
            raise ValueError(k + ' must be boolean')
    for k in ('tags', 'children'):
        if k in o and (not isinstance(o[k], list) or not all(isinstance(v, str) for v in o[k])):
            raise ValueError(k + ' must be a string array')
    if 'dash' in o and (not isinstance(o['dash'], list) or len(o['dash']) != 2 or any(number(v, 'dash') < 0 for v in o['dash'])):
        raise ValueError('dash must be [on,off]')
    for k in ('head', 'tail'):
        if k in o and o[k] not in HEADS:
            raise ValueError(k + ' must be one of ' + ', '.join(sorted(HEADS)))
    if t in ('rect', 'ellipse'):
        for k in ('w', 'h'):
            if k in o:
                size(o[k], k, {'fill'})
        if t == 'ellipse' and 'r' not in o and not ('w' in o and 'h' in o):
            raise ValueError('ellipse needs r or w and h')
        if t == 'rect' and not ('w' in o and 'h' in o):
            raise ValueError('rect needs w and h (numbers or "fill")')
    if t in ('polygon', 'polyline'):
        points(o.get('points'), 'points', 3 if t == 'polygon' else 2)
    if t == 'text':
        if 'w' in o:
            size(o['w'], 'w', {'fill'})
        if 'h' in o:
            size(o['h'], 'h', {'fill', 'hug'})
        if 'font' in o and o['font'] not in FONTS:
            raise ValueError('font must be sans or mono')
        if 'align' in o and o['align'] not in ALIGNS:
            raise ValueError('align must be left, center or right')
    if t == 'edge':
        if 'from' not in o or 'to' not in o:
            raise ValueError('edge needs from and to')
        anchor(o['from'], 'from')
        anchor(o['to'], 'to')
        if 'route' in o and o['route'] not in ROUTES:
            raise ValueError('route must be one of ' + ', '.join(sorted(ROUTES)))
        if 'parent' in o:
            raise ValueError('edges are root objects and cannot be grouped')
    if t == 'group':
        for k in ('w', 'h'):
            if k in o:
                size(o[k], k, {'fill', 'hug'})
        if 'padding' in o and o['padding'] < 0:
            raise ValueError('padding must be nonnegative')
        if 'layout' in o:
            lay = o['layout']
            if not isinstance(lay, dict) or lay.get('type') not in LAYOUTS:
                raise ValueError('layout must be {"type":"stack"|"row"|"grid",...}')
            if set(lay) - {'type', 'gap', 'cols', 'align'}:
                raise ValueError('unknown layout fields')
            if 'gap' in lay and number(lay['gap'], 'gap') < 0:
                raise ValueError('gap must be nonnegative')
            if lay['type'] == 'grid' and (type(lay.get('cols')) is not int or lay['cols'] < 1):
                raise ValueError('grid layout needs cols >= 1')
            if 'align' in lay and lay['align'] not in ('start', 'center', 'end'):
                raise ValueError('align must be start, center or end')
    for k in ('box', 'measured'):
        if k in o and not isinstance(o[k], dict):
            raise ValueError(k + ' is reported by the browser, not set')
    if 'params' in o and not isinstance(o['params'], dict):
        raise ValueError('params must be an object')


def check_graph(scene):
    objs = scene['objects']
    owned = {}
    for o in objs.values():
        validate_object(o)
        if o.get('parent') is not None:
            p = objs.get(o['parent'])
            if not p or p['type'] != 'group' or o['id'] not in p.get('children', []):
                raise ValueError(f'{o["id"]} names parent {o["parent"]} that does not own it')
        for c in o.get('children', []):
            if c not in objs or c == o['id'] or c in owned:
                raise ValueError('invalid or multiply owned child ' + c)
            if objs[c].get('parent') != o['id']:
                raise ValueError(f'child {c} does not name {o["id"]} as parent')
            owned[c] = o['id']
        if o['type'] == 'edge':
            for field in ('from', 'to'):
                a = o[field]
                i = anchor_id(a)
                if i is None:
                    continue
                target = objs.get(i)
                if not target:
                    raise ValueError(f'missing edge endpoint {i}')
                if target['type'] == 'edge':
                    raise ValueError('edges attach to shapes, text or groups, not other edges')
                if isinstance(a, dict) and 'vertex' in a:
                    if target['type'] not in ('polygon', 'polyline'):
                        raise ValueError('vertex anchors need a polygon or polyline')
                    if a['vertex'] >= len(target['points']):
                        raise ValueError(f'{i} has no vertex {a["vertex"]}')

    def visit(i, stack):
        if i in stack:
            raise ValueError('cyclic group')
        for c in objs[i].get('children', []):
            visit(c, stack | {i})
    for i in objs:
        visit(i, set())


# ---------------------------------------------------------------- presets

_TOKEN = re.compile(r'\s*(?:(\d+\.?\d*|\.\d+)|([A-Za-z_][A-Za-z0-9_]*)|(.))')


def evaluate(expr, params):
    """Tiny arithmetic: numbers, parameters, + - * / and parentheses."""
    if not isinstance(expr, str) or len(expr) > 200:
        raise ValueError('expression must be a short string')
    tokens = []
    for num, name, op in _TOKEN.findall(expr):
        if num:
            tokens.append(('n', float(num)))
        elif name:
            if name not in params:
                raise ValueError(f'unknown parameter {name} in expression {expr!r}')
            v = params[name]
            if isinstance(v, bool) or not isinstance(v, (int, float)):
                raise ValueError(f'parameter {name} is not a number')
            tokens.append(('n', float(v)))
        elif op.strip():
            if op not in '+-*/()':
                raise ValueError(f'bad character {op!r} in expression {expr!r}')
            tokens.append(('o', op))
    pos = 0

    def peek():
        return tokens[pos] if pos < len(tokens) else (None, None)

    def take():
        nonlocal pos
        pos += 1
        return tokens[pos - 1]

    def atom():
        kind, v = take() if pos < len(tokens) else (None, None)
        if kind == 'n':
            return v
        if kind == 'o' and v == '(':
            r = additive()
            if take() != ('o', ')'):
                raise ValueError('missing ) in ' + expr)
            return r
        if kind == 'o' and v == '-':
            return -atom()
        raise ValueError('bad expression ' + expr)

    def term():
        r = atom()
        while peek() in (('o', '*'), ('o', '/')):
            op = take()[1]
            rhs = atom()
            if op == '/':
                if rhs == 0:
                    raise ValueError('division by zero in ' + expr)
                r /= rhs
            else:
                r *= rhs
        return r

    def additive():
        r = term()
        while peek() in (('o', '+'), ('o', '-')):
            op = take()[1]
            r = r + term() if op == '+' else r - term()
        return r
    result = additive()
    if pos != len(tokens):
        raise ValueError('bad expression ' + expr)
    if not math.isfinite(result):
        raise ValueError('expression is not finite: ' + expr)
    return int(result) if result == int(result) else result


_INTERP = re.compile(r'\$\{([A-Za-z_][A-Za-z0-9_]*)\}')


def substitute(value, params):
    if isinstance(value, dict):
        if set(value) == {'$'}:
            return evaluate(value['$'], params)
        return {k: substitute(v, params) for k, v in value.items()}
    if isinstance(value, list):
        return [substitute(v, params) for v in value]
    if isinstance(value, str):
        m = _INTERP.fullmatch(value)
        if m:
            if m.group(1) not in params:
                raise ValueError(f'unknown parameter {m.group(1)}')
            return copy.deepcopy(params[m.group(1)])  # whole-value reference keeps the type

        def rep(m):
            if m.group(1) not in params:
                raise ValueError(f'unknown parameter {m.group(1)}')
            v = params[m.group(1)]
            return v if isinstance(v, str) else json.dumps(v)
        parts = [rep(m) for m in _INTERP.finditer(value)]
        if sum(map(len, parts)) + len(value) > 100000:
            raise ValueError('substituted text is longer than 100000 characters')
        it = iter(parts)
        return _INTERP.sub(lambda m: next(it), value)
    return value


def expand_items(items, params, out, depth=0):
    if depth > 4:
        raise ValueError('repeat nesting too deep')
    for item in items:
        if not isinstance(item, dict):
            raise ValueError('preset items must be objects')
        if 'when' in item:
            if not isinstance(item['when'], str) or item['when'] not in params:
                raise ValueError('when must name a parameter')
            if not params[item['when']]:
                continue
        if 'repeat' in item:
            extra = set(item) - {'repeat', 'as', 'index', 'items', 'when'}
            if extra:
                raise ValueError('repeat accepts repeat, as, index, items')
            seq = params.get(item['repeat']) if isinstance(item.get('repeat'), str) else None
            if not isinstance(seq, list):
                raise ValueError(f'repeat needs an array parameter, got {item.get("repeat")!r}')
            if len(seq) > 500:
                raise ValueError('repeat arrays are limited to 500 elements')
            alias = item.get('as', 'item')
            index = item.get('index', 'i')
            for n, element in enumerate(seq):
                expand_items(item.get('items', []), {**params, alias: element, index: n}, out, depth + 1)
            continue
        if len(out) >= 2000:
            raise ValueError('preset expands to more than 2000 objects')
        out.append(substitute({k: v for k, v in item.items() if k != 'when'}, params))
    return out


def validate_preset(definition):
    if not isinstance(definition, dict) or not isinstance(definition.get('params'), dict) or not isinstance(definition.get('items'), list):
        raise ValueError('a preset needs params (object of defaults) and items (array)')
    if set(definition) - {'name', 'params', 'items', 'group', 'doc'}:
        raise ValueError('preset accepts name, params, items, group, doc')
    for k in definition['params']:
        if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', k):
            raise ValueError(f'bad parameter name {k!r}')
        if k in INSTANCE_FIELDS:
            raise ValueError(f'parameter {k!r} collides with an instance field')
    if 'group' in definition and not isinstance(definition['group'], dict):
        raise ValueError('preset group must be an object')
    objs = expand(definition, {}, 'probe', 0, 0, {})
    scene = fresh()
    for o in objs:
        scene['objects'][o['id']] = o
    check_graph(scene)


def expand(definition, given, instance_id, x, y, instance_fields):
    """Materialize a preset: returns the list of objects (instance first)."""
    params = dict(definition['params'])
    unknown = set(given) - set(params)
    if unknown:
        raise ValueError('unknown parameters ' + ', '.join(sorted(unknown)) + '; this preset accepts ' + ', '.join(sorted(params)))
    for k, v in given.items():
        d = params[k]
        if isinstance(d, bool) or isinstance(v, bool):
            if type(v) is not type(d):
                raise ValueError(f'parameter {k} must be boolean')
        elif isinstance(d, (int, float)):
            number(v, k)
        elif isinstance(d, str) and not isinstance(v, str):
            raise ValueError(f'parameter {k} must be text')
        elif isinstance(d, list) and not isinstance(v, list):
            raise ValueError(f'parameter {k} must be an array')
        params[k] = copy.deepcopy(v)
    items = expand_items(definition['items'], params, [])
    if not items:
        raise ValueError('preset expanded to nothing')
    seen = set()
    for o in items:
        if not isinstance(o.get('id'), str):
            raise ValueError('every preset item needs an id')
        if o['id'] in seen:
            raise ValueError('preset child ids must be unique after expansion: ' + o['id'])
        seen.add(o['id'])
    meta = dict(preset=definition['name'], params={k: params[k] for k in given}, **instance_fields)
    if len(items) == 1 and 'group' not in definition:
        # A single-item preset is an alias for that primitive: no wrapper group.
        o = {**items[0], **meta, 'id': instance_id, 'x': x, 'y': y}
        o.pop('parent', None)
        validate_object(o)
        return [o]
    shell = substitute(definition.get('group', {}), params)
    reserved = (READ_ONLY | {'id', 'type', 'children'}) & set(shell)
    if reserved:
        raise ValueError('preset group cannot set ' + ', '.join(sorted(reserved)))
    group = {**shell, 'type': 'group', 'children': [], **meta, 'id': instance_id, 'x': x, 'y': y}
    out = [group]
    by_id = {instance_id: group}
    for o in items:
        local_parent = o.get('parent')
        parent_id = instance_id if local_parent is None else f'{instance_id}/{local_parent}'
        if parent_id not in by_id or by_id[parent_id]['type'] != 'group':
            raise ValueError(f'preset item {o["id"]} names parent {local_parent!r}, which is not an earlier group item')
        child = {'x': 0, 'y': 0, **o, 'id': f'{instance_id}/{o["id"]}', 'parent': parent_id}
        if child['type'] == 'edge':
            raise ValueError('presets cannot contain edges yet')
        if child['type'] == 'group':
            if 'children' in child:
                raise ValueError('preset group items list no children; later items name them as parent')
            child['children'] = []
        validate_object(child)
        by_id[parent_id]['children'].append(child['id'])
        by_id[child['id']] = child
        out.append(child)
    validate_object(group)
    return out


# ---------------------------------------------------------------- selection and tree helpers

def select(scene, spec):
    objects = scene['objects']
    if isinstance(spec, str):
        spec = [spec]
    if isinstance(spec, list):
        missing = [i for i in spec if not isinstance(i, str) or i not in objects]
        if missing:
            raise ValueError('selection names missing objects: ' + ', '.join(map(str, missing)))
        return list(dict.fromkeys(spec))
    if not isinstance(spec, dict):
        raise ValueError('select must be an id, an id list, or a filter object')
    if set(spec) - {'type', 'tag', 'preset', 'parent', 'ids', 'fraction', 'slice', 'limit', 'roots'}:
        raise ValueError('unknown selection filter; use type, tag, preset, parent, ids, roots, fraction, slice, limit')
    ids = select(scene, spec['ids']) if 'ids' in spec else list(objects)
    if 'type' in spec:
        ids = [i for i in ids if objects[i]['type'] == spec['type'] or objects[i].get('preset') == spec['type']]
    if 'preset' in spec:
        ids = [i for i in ids if objects[i].get('preset') == spec['preset']]
    if 'tag' in spec:
        ids = [i for i in ids if spec['tag'] in objects[i].get('tags', [])]
    if 'parent' in spec:
        ids = [i for i in ids if objects[i].get('parent') == spec['parent']]
    if spec.get('roots'):
        ids = [i for i in ids if objects[i].get('parent') is None]
    if 'fraction' in spec:
        f = number(spec['fraction'], 'fraction')
        if not 0 <= f <= 1:
            raise ValueError('fraction must be 0..1')
        ids = ids[:math.floor(len(ids) * f)]
    if 'slice' in spec:
        s = spec['slice']
        if not isinstance(s, list) or len(s) != 2 or any(v is not None and type(v) is not int for v in s):
            raise ValueError('slice must be [start,end]')
        ids = ids[s[0]:s[1]]
    if 'limit' in spec:
        if type(spec['limit']) is not int or spec['limit'] < 0:
            raise ValueError('limit must be a nonnegative integer')
        ids = ids[:spec['limit']]
    return ids


def descendants(scene, ids):
    result = set(ids)
    for i in ids:
        result.update(descendants(scene, scene['objects'][i].get('children', [])))
    return result


def root_of(scene, i):
    objs = scene['objects']
    while objs[i].get('parent') is not None:
        i = objs[i]['parent']
    return i


def world_position(scene, i):
    """World origin of i. A child of a laid-out group sits where the layout put it, which the
    browser reports as box.ox/oy; its stored x,y are ignored by the renderer. The reported offset
    from the parent's reported origin is used, so a parent moved since the report still counts."""
    objs = scene['objects']
    x = y = z = 0
    while i is not None:
        o = objs[i]
        parent = o.get('parent')
        box = o.get('box') or {}
        if parent is not None and objs[parent].get('layout') and 'ox' in box and 'oy' in box:
            pbox = objs[parent].get('box') or {}
            if 'ox' not in pbox or 'oy' not in pbox:
                return x + box['ox'], y + box['oy'], z + o.get('z', 0) + world_position(scene, parent)[2]
            x += box['ox'] - pbox['ox']
            y += box['oy'] - pbox['oy']
        else:
            x += o.get('x', 0)
            y += o.get('y', 0)
        z += o.get('z', 0)
        i = parent
    return x, y, z


def instance_of(scene, i):
    """The preset instance that owns object i (nearest preset ancestor), if i is inside one."""
    objs = scene['objects']
    p = objs[i].get('parent')
    while p is not None:
        if objs[p].get('preset'):
            return p
        p = objs[p].get('parent')
    return None


# ---------------------------------------------------------------- operations

def timing(c):
    """Validated (duration, stagger) of a command, in milliseconds. Create/remove fade in 180 ms and
    view glides in 300 ms unless told otherwise; everything else is immediate."""
    default = 180 if c.get('op') in ('create', 'remove') else 300 if c.get('op') == 'view' else 0
    out = []
    for key, fallback in (('duration', default), ('stagger', 0)):
        v = number(c.get(key, fallback), key)
        if not 0 <= v <= 10000:
            raise ValueError(key + ' must be 0..10000 milliseconds')
        out.append(v)
    return tuple(out)


def operation(scene, c):
    """Mutates a private candidate; the caller commits only after all validation passes."""
    if not isinstance(c, dict):
        raise ValueError('command must be an object')
    op = c.get('op')
    objs = scene['objects']
    selected = []
    allowed = {'create', 'set', 'remove', 'move', 'reparent', 'group', 'ungroup', 'link', 'define', 'undefine', 'physics', 'impulse', 'layout', 'view', 'wait', 'clear'}
    if op not in allowed:
        raise ValueError(f'unknown op {op!r}; ops are ' + ', '.join(sorted(allowed | {'undo', 'redo'})))
    timing(c)
    presets = presets_of(scene)

    def put(o):
        if o['id'] in objs:
            raise ValueError('duplicate id ' + o['id'])
        objs[o['id']] = o

    def add(spec):
        """Create a primitive or expand a preset from a create-style object."""
        spec = copy.deepcopy(spec)
        if not isinstance(spec, dict):
            raise ValueError('object must be an object')
        if READ_ONLY & set(spec):
            raise ValueError(', '.join(sorted(READ_ONLY & set(spec))) + ' are reported or managed by the board, not set')
        t = spec.get('type')
        if t in presets:
            instance_id = spec.get('id')
            if not isinstance(instance_id, str):
                raise ValueError('object needs an id')
            fields = {k: v for k, v in spec.items() if k in INSTANCE_FIELDS and k not in ('id', 'x', 'y')}
            params = {k: v for k, v in spec.items() if k not in INSTANCE_FIELDS and k != 'type'}
            for o in expand(presets[t], params, instance_id, number(spec.get('x', 0), 'x'), number(spec.get('y', 0), 'y'), fields):
                put(o)
            selected.append(instance_id)
            return instance_id
        if t not in PRIMITIVES:
            raise ValueError(f'unknown type {t!r}; primitives are ' + ', '.join(sorted(PRIMITIVES)) + '; presets are ' + ', '.join(sorted(presets)))
        o = {'x': 0, 'y': 0, **spec}
        if t == 'group':
            o.setdefault('children', [])
        validate_object(o)
        put(o)
        selected.append(o['id'])
        return o['id']

    def remove_ids(ids):
        removed = descendants(scene, ids)
        removed.update(i for i, o in objs.items() if o['type'] == 'edge' and (anchor_id(o['from']) in removed or anchor_id(o['to']) in removed))
        for i in removed:
            objs.pop(i, None)
        for o in objs.values():
            if 'children' in o:
                o['children'] = [i for i in o['children'] if i not in removed]
        return removed

    def reexpand(i, params, reset):
        inst = objs[i]
        if inst.get('overridden') and not reset:
            raise ValueError(f'{i} has directly edited children; pass "resetOverrides":true to re-expand it from its parameters')
        merged = {**inst.get('params', {}), **params}
        fields = {k: v for k, v in inst.items() if k in INSTANCE_FIELDS and k not in ('id', 'x', 'y')}
        fresh_objs = expand(presets[inst['preset']], merged, i, inst.get('x', 0), inst.get('y', 0), fields)
        new_ids = {o['id'] for o in fresh_objs}
        old_children = descendants(scene, [i]) - {i}
        for o in fresh_objs:
            if o['id'] in objs and o['id'] != i and o['id'] not in old_children:
                raise ValueError(f're-expanding {i} would create {o["id"]}, which is an unrelated existing object; remove or rename it first')
        for child in old_children - new_ids:
            objs.pop(child, None)
        for o in fresh_objs:
            # Browser reports survive until the next ack, so a laid-out child keeps its origin.
            reported = {k: objs[o['id']][k] for k in ('box', 'measured') if o['id'] in objs and k in objs[o['id']]}
            if o['id'] in objs and o['id'] != i:
                objs[o['id']].clear()
                objs[o['id']].update(o, **reported)
            elif o['id'] == i:
                keep_parent = inst.get('parent')
                inst.clear()
                inst.update(o, **reported)
                if keep_parent is not None:
                    inst['parent'] = keep_parent
            else:
                objs[o['id']] = o
        # Edges that pointed at vanished children go with them.
        gone = old_children - new_ids
        for eid, e in list(objs.items()):
            if e['type'] == 'edge' and (anchor_id(e['from']) in gone or anchor_id(e['to']) in gone):
                objs.pop(eid)

    if op == 'create':
        if 'items' in c:
            if not isinstance(c['items'], list) or not c['items']:
                raise ValueError('items must be a nonempty array')
            for spec in c['items']:
                add(spec)
        else:
            spec = c.get('object', {})
            if not isinstance(spec, dict):
                raise ValueError('create needs object:{...} or items:[...]')
            if c.get('arrange', 'scatter') not in ('scatter', 'grid'):
                raise ValueError('arrange must be scatter or grid')
            count = c.get('count', 1)
            if type(count) is not int or not 1 <= count <= 500:
                raise ValueError('count must be 1..500')
            rng = random.Random(c.get('seed', 1))
            spacing = number(c.get('spacing', 45))
            spread = number(c.get('spread', 300))
            for i in range(count):
                n = copy.deepcopy(spec)
                if count > 1:
                    n['id'] = str(spec.get('id', 'obj')) + '-' + str(i)
                    if c.get('arrange', 'scatter') == 'grid':
                        cols = math.ceil(math.sqrt(count))
                        n['x'] = spec.get('x', 0) + (i % cols) * spacing
                        n['y'] = spec.get('y', 0) + (i // cols) * spacing
                    else:
                        a = rng.random() * math.tau
                        r = math.sqrt(rng.random()) * spread
                        n['x'] = spec.get('x', 0) + math.cos(a) * r
                        n['y'] = spec.get('y', 0) + math.sin(a) * r
                add(n)
    elif op == 'define':
        d = c.get('preset')
        if not isinstance(d, dict) or not isinstance(d.get('name'), str) or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_-]*', d['name']):
            raise ValueError('define needs preset:{name, params, items}')
        if d['name'] in PRIMITIVES or d['name'] in BUILTIN_PRESETS:
            raise ValueError(f'preset name {d["name"]} is a built-in; pick another name')
        validate_preset(d)
        scene.setdefault('presets', {})[d['name']] = copy.deepcopy(d)
    elif op == 'undefine':
        name = c.get('name')
        if name not in scene.get('presets', {}):
            raise ValueError('undefine names a user preset; built-ins cannot be removed')
        if any(o.get('preset') == name for o in objs.values()):
            raise ValueError(f'preset {name} is still in use; remove its instances first')
        del scene['presets'][name]
    elif op == 'link':
        props = c.get('props', {})
        if not isinstance(props, dict):
            raise ValueError('link props must be an object')
        head = {'head': 'arrow'} if c.get('arrow', False) else {}
        add({**head, **props, 'id': c.get('id'), 'type': 'edge', 'from': c.get('from'), 'to': c.get('to')})
    elif op == 'view':
        if 'center' in c:
            if not isinstance(c['center'], list) or len(c['center']) not in (2, 3):
                raise ValueError('view center must be [x,y] or [x,y,z]')
            for v in c['center']:
                number(v, 'center')
        if 'by' in c:
            if not isinstance(c['by'], list) or len(c['by']) != 2:
                raise ValueError('view by must be [dx,dy]')
            for v in c['by']:
                number(v, 'by')
        if 'zoom' in c and not 0.05 <= number(c['zoom'], 'zoom') <= 8:
            raise ValueError('zoom must be 0.05..8')
        if 'fit' in c and c['fit'] is not True:
            selected = select(scene, c['fit'])
        if 'mode' in c and c['mode'] not in ('2d', '3d'):
            raise ValueError('view mode must be "2d" or "3d"')
        if 'yaw' in c:
            number(c['yaw'], 'yaw')
        if 'pitch' in c and not -85 <= number(c['pitch'], 'pitch') <= 85:
            raise ValueError('pitch must be -85..85 degrees')
        if not {'center', 'by', 'zoom', 'fit', 'mode', 'yaw', 'pitch'} & set(c):
            raise ValueError('view needs center, by, zoom, fit, mode, yaw or pitch')
    elif op == 'physics':
        props = c.get('props', {})
        if not isinstance(props, dict):
            raise ValueError('physics props must be an object')
        if set(props) - set(DEFAULT_PHYSICS):
            raise ValueError('unknown physics property; use ' + ', '.join(sorted(DEFAULT_PHYSICS)))
        for k, v in props.items():
            if k in ('enabled', 'collision'):
                if type(v) is not bool:
                    raise ValueError(k + ' must be boolean')
            else:
                number(v, k)
                if v < 0 or (k in ('damping', 'bounce') and v > 1):
                    raise ValueError('invalid physics coefficient')
        scene['physics'].update(props)
    elif op == 'clear':
        selected = list(objs)
        objs.clear()
    elif op != 'wait':
        selected = select(scene, c.get('select'))
        if op == 'set':
            props = c.get('props', {})
            if not isinstance(props, dict):
                raise ValueError('set needs props')
            if READ_ONLY & set(props) or {'id', 'type'} & set(props):
                raise ValueError('set cannot change id, type or board-managed fields')
            for i in selected:
                o = objs[i]
                if o.get('preset'):
                    preset_params = set(presets[o['preset']]['params'])
                    params = {k: v for k, v in props.items() if k in preset_params}
                    rest = {k: v for k, v in props.items() if k not in preset_params}
                    if params:
                        reexpand(i, params, bool(c.get('resetOverrides')))
                        o = objs[i]
                else:
                    rest = props
                    owner = instance_of(scene, i)
                    if owner:
                        objs[owner]['overridden'] = True
                for k, v in rest.items():
                    if v is None:
                        o.pop(k, None)
                    else:
                        o[k] = copy.deepcopy(v)
                validate_object(o)
        elif op == 'move':
            if 'to' in c and len(selected) != 1:
                raise ValueError('move to requires exactly one selected object; use by for many')
            for i in selected:
                p = objs[i].get('parent')
                if p is not None and objs[p].get('layout'):
                    raise ValueError(f'{i} is positioned by the layout of {p}; reorder its parent\'s children with set, or reparent it')
            if 'to' in c:
                dest = c['to']
                if not isinstance(dest, list) or len(dest) not in (2, 3):
                    raise ValueError('move to needs [x,y] or [x,y,z]')
                o = objs[selected[0]]
                delta = [number(dest[0]) - o.get('x', 0), number(dest[1]) - o.get('y', 0)] + ([number(dest[2]) - o.get('z', 0)] if len(dest) == 3 else [])
            else:
                delta = c.get('by')
            if not isinstance(delta, list) or len(delta) not in (2, 3):
                raise ValueError('move needs by:[dx,dy] or to:[x,y]; a third value moves in z')
            dx, dy = number(delta[0]), number(delta[1])
            dz = number(delta[2]) if len(delta) == 3 else 0
            for i in selected:
                owner = instance_of(scene, i)
                if owner and owner not in selected:
                    objs[owner]['overridden'] = True
            for i in selected:
                objs[i]['x'] = objs[i].get('x', 0) + dx
                objs[i]['y'] = objs[i].get('y', 0) + dy
                if dz:
                    objs[i]['z'] = objs[i].get('z', 0) + dz
        elif op == 'remove':
            for i in selected:
                owner = instance_of(scene, i)
                if owner and owner not in selected:
                    objs[owner]['overridden'] = True
            remove_ids(selected)
        elif op == 'reparent':
            into = c.get('into')
            if into is not None and (into not in objs or objs[into]['type'] != 'group'):
                raise ValueError('reparent into must be a group id or null for the root')
            if into is not None and objs[into].get('preset'):
                raise ValueError('cannot reparent into a preset instance; ungroup it or set resetOverrides on a define')
            if into is not None and instance_of(scene, into):
                objs[instance_of(scene, into)]['overridden'] = True
            for i in selected:
                if objs[i]['type'] == 'edge':
                    raise ValueError('edges are root objects')
                if into is not None and into in descendants(scene, [i]):
                    raise ValueError('cannot reparent into a descendant')
                wx, wy, wz = world_position(scene, i)
                owner = instance_of(scene, i)
                if owner:
                    objs[owner]['overridden'] = True
                old = objs[i].get('parent')
                if old is not None:
                    objs[old]['children'] = [k for k in objs[old]['children'] if k != i]
                if into is None:
                    objs[i].pop('parent', None)
                    objs[i]['x'], objs[i]['y'] = wx, wy
                else:
                    px, py, pz = world_position(scene, into)
                    objs[i]['parent'] = into
                    objs[i]['x'], objs[i]['y'] = wx - px, wy - py
                    objs[into]['children'].append(i)
                    wz -= pz
                if wz:
                    objs[i]['z'] = wz
                else:
                    objs[i].pop('z', None)
        elif op == 'group':
            gid = c.get('id')
            if not isinstance(gid, str):
                raise ValueError('group needs an id')
            props = c.get('props', {})
            if not isinstance(props, dict):
                raise ValueError('group props must be an object')
            reserved = (READ_ONLY | {'id', 'type', 'children'}) & set(props)
            if reserved:
                raise ValueError('group props cannot set ' + ', '.join(sorted(reserved)))
            if not selected:
                raise ValueError('group needs at least one selected object')
            parents = {objs[i].get('parent') for i in selected}
            if len(parents) != 1:
                raise ValueError('grouped objects must share a parent')
            if any(objs[i]['type'] == 'edge' for i in selected):
                raise ValueError('edges cannot be grouped')
            parent = parents.pop()
            if parent is not None and objs[parent].get('layout'):
                raise ValueError('cannot group children of a laid-out group; reparent them first')
            # The origin defaults to the members' top-left; x/y/z in props place it instead.
            # Either way the members keep their positions.
            gx = number(props['x'], 'x') if 'x' in props else min(objs[i].get('x', 0) for i in selected)
            gy = number(props['y'], 'y') if 'y' in props else min(objs[i].get('y', 0) for i in selected)
            gz = number(props.get('z', 0), 'z')
            g = {**props, 'x': gx, 'y': gy, 'id': gid, 'type': 'group', 'children': list(selected)}
            if parent is not None:
                g['parent'] = parent
                objs[parent]['children'] = [k for k in objs[parent]['children'] if k not in selected] + [gid]
            put(g)
            for i in selected:
                objs[i]['parent'] = gid
                objs[i]['x'] = objs[i].get('x', 0) - gx
                objs[i]['y'] = objs[i].get('y', 0) - gy
                if gz:
                    objs[i]['z'] = objs[i].get('z', 0) - gz
            owner = instance_of(scene, gid)
            if owner:
                objs[owner]['overridden'] = True
            validate_object(g)
        elif op == 'ungroup':
            for i in selected:
                if objs[i]['type'] != 'group':
                    raise ValueError('ungroup selects groups only')
                parent = objs[i].get('parent')
                if parent is not None and objs[parent].get('layout'):
                    raise ValueError(f'{i} sits in the laid-out group {parent}; reparent it out before ungrouping')
            for i in selected:
                parent = objs[i].get('parent')
                owner = instance_of(scene, i)
                if owner:
                    objs[owner]['overridden'] = True
                # Members keep the world position they are drawn at (a laid-out group ignores stored x,y).
                base = world_position(scene, parent) if parent is not None else (0, 0, 0)
                placed = {k: world_position(scene, k) for k in objs[i].get('children', [])}
                g = objs.pop(i)
                for k, (wx, wy, wz) in placed.items():
                    objs[k]['x'], objs[k]['y'] = wx - base[0], wy - base[1]
                    if wz - base[2]:
                        objs[k]['z'] = wz - base[2]
                    else:
                        objs[k].pop('z', None)
                    if parent is None:
                        objs[k].pop('parent', None)
                    else:
                        objs[k]['parent'] = parent
                if parent is not None:
                    objs[parent]['children'] = [k for k in objs[parent]['children'] if k != i] + list(g.get('children', []))
                for eid, e in list(objs.items()):
                    if e['type'] == 'edge' and (anchor_id(e['from']) == i or anchor_id(e['to']) == i):
                        objs.pop(eid)
        elif op == 'impulse':
            velocity = c.get('velocity')
            if not isinstance(velocity, list) or len(velocity) != 2:
                raise ValueError('impulse needs velocity:[vx,vy]')
            for i in selected:
                r = root_of(scene, i)
                objs[r]['body'] = True
                objs[r]['vx'] = objs[r].get('vx', 0) + number(velocity[0])
                objs[r]['vy'] = objs[r].get('vy', 0) + number(velocity[1])
            scene['physics']['enabled'] = True
        elif op == 'layout':
            if c.get('mode', 'scatter') not in ('scatter', 'grid'):
                raise ValueError('layout mode must be scatter or grid')
            rng = random.Random(c.get('seed', 1))
            spread = number(c.get('spread', 300))
            spacing = number(c.get('spacing', 180))
            cols = math.ceil(math.sqrt(max(1, len(selected))))
            if any(objs[i].get('parent') is not None for i in selected):
                raise ValueError('layout arranges root objects only')
            for j, i in enumerate(selected):
                if c.get('mode', 'scatter') == 'grid':
                    x, y = (j % cols) * spacing, (j // cols) * spacing
                else:
                    a = rng.random() * math.tau
                    r = math.sqrt(rng.random()) * spread
                    x, y = math.cos(a) * r, math.sin(a) * r
                objs[i]['x'], objs[i]['y'] = x, y
    if len(objs) > 2000:
        raise ValueError('board limit is 2000 objects')
    check_graph(scene)
    return selected


def patch(before, after):
    upsert = [v for k, v in after['objects'].items() if before['objects'].get(k) != v]
    fields = {o['id']: [k for k in set(o) | set(before['objects'].get(o['id'], {})) if before['objects'].get(o['id'], {}).get(k) != o.get(k)] for o in upsert}
    return dict(upsert=upsert, fields=fields, remove=[k for k in before['objects'] if k not in after['objects']], physics=after['physics'])


# ---------------------------------------------------------------- v1.1 upgrade

def upgrade(old):
    """Convert a version-2 scene (absolute members, card/code/text types, from/to links) to version 3."""
    if old.get('version') == VERSION:
        return old
    objs = old.get('objects', {})
    new = fresh()
    new['physics'] = {**DEFAULT_PHYSICS, **old.get('physics', {})}
    out = new['objects']
    common = lambda o: {k: v for k, v in o.items() if k in ('id', 'x', 'y', 'tags', 'opacity', 'pinned', 'body', 'mass', 'vx', 'vy')}
    for i, o in objs.items():
        t = o['type']
        base = common(o)
        style = {k: o[k] for k in ('color', 'fill', 'strokeWidth') if k in o}
        if 'from' in o:
            e = {**base, 'type': 'edge', 'from': o['from'], 'to': o['to'], **style}
            for k in ('route', 'curve', 'head', 'tail', 'rest', 'strength'):
                if k in o:
                    e[k] = o[k]
            if t == 'arrow' and 'head' not in e:
                e['head'] = 'arrow'
            if o.get('text'):
                e['label'] = o['text']
            e.pop('x', None); e.pop('y', None)
            out[i] = e
        elif t in ('card', 'code', 'text'):
            params = {}
            if o.get('title'):
                params['title'] = o['title']
            if o.get('text'):
                params['text'] = o['text']
            if 'width' in o:
                params['w'] = o['width']
            if 'color' in o:
                params['color'] = o['color']
            if 'fill' in o and t != 'text':
                params['fill'] = o['fill']
            if 'fontSize' in o:
                params['size'] = o['fontSize']
            name = 'label' if t == 'text' else t
            if t == 'text':
                params = {'text': '\n'.join(s for s in (o.get('title'), o.get('text')) if s), **{k: v for k, v in params.items() if k in ('w', 'color', 'size')}}
            for e in expand(BUILTIN_PRESETS[name], params, i, o.get('x', 0), o.get('y', 0), {k: v for k, v in base.items() if k not in ('id', 'x', 'y')}):
                if e['id'] in out:
                    raise ValueError('upgrade id collision ' + e['id'])
                out[e['id']] = e
        elif t == 'dot':
            params = {k: v for k, v in (('r', o.get('radius')), ('color', o.get('color'))) if v is not None}
            out[i] = expand(BUILTIN_PRESETS['dot'], params, i, o.get('x', 0), o.get('y', 0), {k: v for k, v in base.items() if k not in ('id', 'x', 'y')})[0]
        elif t == 'ellipse':
            out[i] = {**base, 'type': 'ellipse', 'w': o.get('width', 260), 'h': o.get('height', 140), **style}
            out[i].setdefault('fill', out[i].get('color', '#8eaede'))
        elif t == 'rectangle':
            out[i] = {**base, 'type': 'rect', 'w': o.get('width', 260), 'h': o.get('height', 140), 'rx': 3, **style}
        elif t == 'diamond':
            params = {k: v for k, v in (('w', o.get('width')), ('h', o.get('height')), ('color', o.get('color'))) if v is not None}
            out[i] = expand(BUILTIN_PRESETS['diamond'], params, i, o.get('x', 0), o.get('y', 0), {k: v for k, v in base.items() if k not in ('id', 'x', 'y')})[0]
        elif t == 'polygon':
            out[i] = {**base, 'type': 'polygon', 'points': o.get('points', [[0, 0], [10, 0], [0, 10]]), **style}
            out[i].setdefault('fill', out[i].get('color', '#8eaede'))
        elif t in ('line', 'arrow', 'path'):
            pts = o.get('points') or [[0, 0], [120, 0]]
            if o.get('closed'):
                out[i] = {**base, 'type': 'polygon', 'points': pts, **style}
            else:
                out[i] = {**base, 'type': 'polyline', 'points': pts, **style}
                if t == 'arrow' or o.get('head'):
                    out[i]['head'] = o.get('head', 'arrow')
                if o.get('tail'):
                    out[i]['tail'] = o['tail']
        elif t == 'group':
            g = {**base, 'type': 'group', 'children': list(o.get('members', [])), **style}
            if 'title' in o or 'text' in o:
                g['title'] = o.get('title') or o.get('text', '')
            if 'outline' in o:
                g['outline'] = o['outline']
            out[i] = g
    # Members become children in local coordinates. Absolute positions are read before any are
    # rewritten, so nested groups convert correctly whatever their key order. Edges stay roots,
    # and a member claimed by two groups belongs to the first.
    absolute = {i: (o.get('x', 0), o.get('y', 0)) for i, o in out.items()}
    owner = {}
    for gid, g in out.items():
        if g['type'] != 'group' or g.get('preset'):
            continue
        g['children'] = [cid for cid in dict.fromkeys(g['children'])
                         if cid in out and cid != gid and cid not in owner and out[cid]['type'] != 'edge']
        for cid in g['children']:
            owner[cid] = gid
    def reaches(start, target):
        while start is not None:
            if start == target:
                return True
            start = owner.get(start)
        return False
    for cid, gid in list(owner.items()):
        if reaches(gid, cid):  # membership cycle in the old scene: break it here
            out[gid]['children'].remove(cid)
            del owner[cid]
    for cid, gid in owner.items():
        child = out[cid]
        child['parent'] = gid
        child['x'] = absolute[cid][0] - absolute[gid][0]
        child['y'] = absolute[cid][1] - absolute[gid][1]
    for o in out.values():
        if o['type'] == 'edge':
            for f in ('from', 'to'):
                if anchor_id(o[f]) not in out:
                    o['_drop'] = True
    for k in [k for k, o in out.items() if o.pop('_drop', False)]:
        out.pop(k)
    check_graph(new)
    return new
