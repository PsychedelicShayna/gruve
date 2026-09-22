import {bounds,translate,rootFor,stepPhysics,interpolate,route,headGeometry} from './engine.js';

const $=id=>document.getElementById(id);
const NS='http://www.w3.org/2000/svg';
const world=$('world'),canvas=$('canvas');
const objects=new Map(),elements=new Map(),measures=new Map(),selected=new Set(),locked=new Set(),queue=[];
let physics={},seq=0,running=false,dirty=true,epoch=0,drag=null,editorId=null,fitOnLoad=true,checkpointAt=0;
let marks=[],pointMode=false,noteTarget=null,cameraAnimation=null;
const client=crypto.randomUUID();
const view=JSON.parse(sessionStorage.getItem('idea-board-view')||'null')||{x:innerWidth/2,y:innerHeight/2,z:1};
const marksLayer=svg('g',{id:'marks',class:'marks'});
const BATCH_COLORS=['#6ee7a0','#ff7a7a','#7ab8ff','#c98bff','#ffb35c'];

function svg(tag,attrs={}){const e=document.createElementNS(NS,tag);attrsTo(e,attrs);return e;}
function attrsTo(e,attrs){for(const [k,v] of Object.entries(attrs))if(v!==undefined)e.setAttribute(k,v);}
function notify(message){$('toast').textContent=message;$('toast').style.display='block';setTimeout(()=>$('toast').style.display='none',5000);}
const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve));

// ---------- camera ----------
function transform(){
  world.setAttribute('transform',`translate(${view.x} ${view.y}) scale(${view.z})`);
  sessionStorage.setItem('idea-board-view',JSON.stringify(view));
  drawMarks();drawHandle();positionNoteInput();
}
function toWorld(sx,sy){return {x:(sx-view.x)/view.z,y:(sy-view.y)/view.z};}
function toScreen(wx,wy){return {x:wx*view.z+view.x,y:wy*view.z+view.y};}
function viewSummary(){const tl=toWorld(0,0),br=toWorld(innerWidth,innerHeight);return {cx:(tl.x+br.x)/2,cy:(tl.y+br.y)/2,zoom:view.z,w:br.x-tl.x,h:br.y-tl.y};}
function sceneBox(ids){
  const list=(ids||[...objects.keys()]).map(i=>objects.get(i)).filter(Boolean);
  if(!list.length)return null;
  const boxes=list.map(o=>bounds(o,objects));
  const x=Math.min(...boxes.map(b=>b.x)),y=Math.min(...boxes.map(b=>b.y));
  return {x,y,w:Math.max(...boxes.map(b=>b.x+b.w))-x,h:Math.max(...boxes.map(b=>b.y+b.h))-y};
}
function fitTarget(ids){
  const box=sceneBox(ids);if(!box||!box.w||!box.h)return null;
  const z=Math.min(1.3,(innerWidth-140)/box.w,(innerHeight-180)/box.h);
  return {z,x:innerWidth/2-(box.x+box.w/2)*z,y:innerHeight/2-(box.y+box.h/2)*z};
}
function viewTargetFor(c){
  if(c.fit!==undefined)return fitTarget(c.fit===true?null:c.selected);
  const current=viewSummary();
  const z=c.zoom??view.z;
  let cx=current.cx,cy=current.cy;
  if(c.center){cx=c.center[0];cy=c.center[1];}
  if(c.by){cx+=c.by[0];cy+=c.by[1];}
  return {z,x:innerWidth/2-cx*z,y:innerHeight/2-cy*z};
}
async function animateCamera(target,duration,generation){
  if(!target)return;
  const from={...view};let start;
  do{
    const now=await frame();if(generation!==undefined&&generation!==epoch)return;
    if(start===undefined)start=now-16;
    const t=duration?Math.min(1,(now-start)/duration):1,ease=1-(1-t)**3;
    view.x=from.x+(target.x-from.x)*ease;view.y=from.y+(target.y-from.y)*ease;view.z=from.z+(target.z-from.z)*ease;
    transform();
    if(t>=1)break;
  }while(true);
}
function fit(){animateCamera(fitTarget(null),250);}

// ---------- server ----------
async function send(commands){
  const r=await fetch('/commands',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(commands)});
  const data=await r.json();if(!r.ok){notify(data.error);throw Error(data.error);}return data;
}
function positionSnapshot(){return Object.fromEntries([...objects].map(([id,o])=>[id,{x:o.x||0,y:o.y||0,vx:o.vx||0,vy:o.vy||0}]));}
function ack(stage,n,full=false){
  const body={client,stage,seq:n,time:Date.now(),visible:!document.hidden,view:viewSummary()};
  if(full){body.positions=positionSnapshot();body.measured=Object.fromEntries(measures);}
  fetch('/ack',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}).catch(()=>{});
}
async function postMark(body){
  const r=await fetch('/marks',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  const data=await r.json();if(!r.ok){notify(data.error);throw Error(data.error);}return data;
}

// ---------- drawing ----------
function center(id){const o=objects.get(id);if(!o)return {x:0,y:0};const b=bounds(o,objects);return {x:b.x+b.w/2,y:b.y+b.h/2};}
const signatureOf=o=>JSON.stringify(o,(k,v)=>k==='measured'?undefined:v);

function drawHeads(g,o,color,strokeWidth,start,end,tangentStart,tangentEnd){
  // Returns trimmed start/end so the stroke ends under a filled head.
  const size=Math.max(8,strokeWidth*4+5);
  const heads=[[o.head??(o.type==='arrow'?'arrow':'none'),end,tangentEnd,1],[o.tail??'none',start,{x:-tangentStart.x,y:-tangentStart.y},-1]];
  const trimmed={start:{...start},end:{...end}};
  for(const [kind,tip,dir,which] of heads){
    if(!kind||kind==='none')continue;
    const geometry=headGeometry(kind,tip,dir,size);
    for(const s of geometry.shapes)g.append(svg(s.tag,{class:'head',stroke:color,'stroke-width':strokeWidth,fill:s.filled?color:'none','stroke-linejoin':'round',...s.attrs}));
    const target=which===1?trimmed.end:trimmed.start;
    target.x-=dir.x*geometry.trim;target.y-=dir.y*geometry.trim;
  }
  return trimmed;
}
function pathFor(points,control,closed){
  if(control)return `M${points[0].x} ${points[0].y}Q${control.x} ${control.y} ${points[1].x} ${points[1].y}`;
  return points.map((p,i)=>(i?'L':'M')+p.x+' '+p.y).join(' ')+(closed?'Z':'');
}
function labelAt(g,text,x,y,color){const t=svg('text',{x,y:y-10,fill:color,'text-anchor':'middle'});t.textContent=text;g.append(t);}

function drawConnection(g,o,color,strokeWidth){
  const r=route(o,objects);if(!r)return;
  const trimmed=drawHeads(g,o,color,strokeWidth,r.start,r.end,r.tangentStart,r.tangentEnd);
  const points=[...r.points];points[0]=trimmed.start;points[points.length-1]=trimmed.end;
  g.append(svg('path',{class:'shape',d:pathFor(points,r.control),fill:'none',stroke:color,'stroke-width':strokeWidth,'stroke-linecap':'round','stroke-linejoin':'round'}));
  if(o.text){
    let mid;
    if(r.control)mid={x:.25*r.start.x+.5*r.control.x+.25*r.end.x,y:.25*r.start.y+.5*r.control.y+.25*r.end.y};
    else if(points.length===4)mid={x:(points[1].x+points[2].x)/2,y:(points[1].y+points[2].y)/2};
    else mid={x:(r.start.x+r.end.x)/2,y:(r.start.y+r.end.y)/2};
    labelAt(g,o.text,mid.x,mid.y,color);
  }
}
function drawPolyline(g,o,color,fill,strokeWidth){
  const raw=(o.points||[[0,0],[120,0]]).map(p=>({x:p[0],y:p[1]}));
  const n=raw.length;
  const unit=(p,q)=>{const dx=q.x-p.x,dy=q.y-p.y,d=Math.hypot(dx,dy)||1;return {x:dx/d,y:dy/d};};
  const tStart=unit(raw[0],raw[1]||raw[0]),tEnd=unit(raw[n-2]||raw[0],raw[n-1]);
  const trimmed=o.closed?{start:raw[0],end:raw[n-1]}:drawHeads(g,o,color,strokeWidth,raw[0],raw[n-1],tStart,tEnd);
  const points=[...raw];points[0]=trimmed.start;points[n-1]=trimmed.end;
  g.append(svg('path',{class:'shape',d:pathFor(points,null,o.closed),fill:o.closed?fill:'none',stroke:color,'stroke-width':strokeWidth,'stroke-linecap':'round','stroke-linejoin':'round'}));
}
function drawCard(g,o,w,color,fill){
  // Cards size themselves to their text unless a height is given; either way the measurement is reported.
  const fontSize=(o.fontSize||14)+'px';
  const div=document.createElement('div');div.className='card';div.style.fontSize=fontSize;div.style.height='auto';
  if(o.type==='text')div.style.color=color;
  if(o.title){const strong=document.createElement('strong');strong.textContent=o.title;div.append(strong);}
  const content=document.createElement(o.type==='code'?'pre':'div');content.textContent=o.text||'';div.append(content);
  const fo=svg('foreignObject',{width:w,height:1});fo.append(div);
  const rect=o.type==='text'?null:svg('rect',{class:'shape',stroke:color,'stroke-width':o.strokeWidth||1.6,width:w,height:1,rx:o.type==='rectangle'?3:13,fill});
  if(rect)g.append(rect);
  g.append(fo);
  const contentH=Math.ceil(div.offsetHeight),contentW=Math.ceil(div.scrollWidth);
  const h=o.height||Math.max(40,contentH);
  div.style.height=h+'px';
  fo.setAttribute('height',h);if(rect)rect.setAttribute('height',h);
  const measured={w,h,contentW,contentH,overflow:contentH>h+1||contentW>w+1};
  o.measured=measured;measures.set(o.id,measured);
  if(measured.overflow)g.append(svg('rect',{class:'overflow',x:0,y:0,width:w,height:h,rx:o.type==='rectangle'?3:13,fill:'none'}));
}
function draw(o){
  let g=elements.get(o.id);
  if(!g){g=svg('g',{'data-id':o.id});elements.set(o.id,g);world.append(g);}
  attrsTo(g,{class:'object'+(selected.has(o.id)?' selected':''),opacity:o.opacity??1,transform:`translate(${o.x||0} ${o.y||0})`});
  // Dependent objects (links, groups) are re-drawn every frame; others only when their signature changes.
  const dependent=o.from||o.type==='group';const signature=signatureOf(o);
  if(!dependent&&g.dataset.signature===signature)return;
  g.dataset.signature=signature;g.replaceChildren();
  const b=bounds(o,objects);
  const w=o.width||(o.type==='dot'?(o.radius||8)*2:260),h=o.height||(o.type==='dot'?(o.radius||8)*2:140);
  const color=o.color||'#8eaede',fill=o.fill??(['dot','ellipse','polygon','diamond'].includes(o.type)?color:'#192435'),strokeWidth=o.strokeWidth||1.6;
  const shape=(tag,a)=>g.append(svg(tag,{class:'shape',stroke:color,'stroke-width':strokeWidth,...a}));
  if(o.type==='group'){
    g.removeAttribute('transform');
    if(o.outline!==false){
      shape('rect',{x:b.x-20,y:b.y-28,width:b.w+40,height:b.h+48,rx:15,fill:'none','stroke-dasharray':'7 5'});
      const t=svg('text',{x:b.x-9,y:b.y-36,fill:color});t.textContent=o.title||o.text||'';g.append(t);
    }
    world.prepend(g);return;
  }
  if(o.from){g.removeAttribute('transform');drawConnection(g,o,color,strokeWidth);world.prepend(g);return;}
  if(o.type==='dot'||o.type==='ellipse')shape('ellipse',{cx:w/2,cy:h/2,rx:w/2,ry:h/2,fill});
  else if(o.type==='diamond')shape('polygon',{points:`${w/2},0 ${w},${h/2} ${w/2},${h} 0,${h/2}`,fill});
  else if(o.type==='polygon')shape('polygon',{points:(o.points||[]).map(p=>p.join(',')).join(' '),fill});
  else if(['line','arrow','path'].includes(o.type))drawPolyline(g,o,color,fill,strokeWidth);
  else drawCard(g,o,w,color,fill);
}
function drawHandle(){
  for(const h of world.querySelectorAll('.handle'))h.remove();
  if(selected.size!==1)return;
  const id=[...selected][0],o=objects.get(id),g=elements.get(id);
  if(!o||!g||o.from||['group','dot','line','arrow','path','polygon'].includes(o.type))return;
  const b=bounds(o,objects),s=12/view.z;
  g.append(svg('rect',{class:'handle',x:b.w-s/2,y:b.h-s/2,width:s,height:s,rx:s/4,'data-handle':id}));
}
function drawMarks(){
  marksLayer.replaceChildren();
  const batches=[...new Set(marks.map(m=>m.batch))].sort((a,b)=>a-b);
  const z=view.z;
  for(const m of marks){
    const rank=batches.length-1-batches.indexOf(m.batch);
    const color=BATCH_COLORS[(m.batch-1)%BATCH_COLORS.length];
    const opacity=m.read?Math.max(.12,.55-rank*.12):.95;
    let x=m.x,y=m.y;
    const target=m.target&&objects.get(m.target);
    if(target&&m.offset){x=(target.x||0)+m.offset[0];y=(target.y||0)+m.offset[1];}
    const g=svg('g',{class:'mark',opacity,transform:`translate(${x} ${y}) scale(${1/z})`});
    g.append(svg('circle',{r:11,fill:color,stroke:'#0a0e16','stroke-width':2}));
    const t=svg('text',{y:4,'text-anchor':'middle',fill:'#0a0e16'});t.textContent=m.n;g.append(t);
    if(m.note){const n=svg('text',{x:16,y:4,fill:color,class:'note'});n.textContent=m.note;g.append(n);}
    marksLayer.append(g);
  }
  world.append(marksLayer);
  $('marks-chip').hidden=!marks.length;
  $('marks-count').textContent=marks.filter(m=>!m.read).length+' unread · '+marks.length+' marks';
}
function render(){
  for(const [id,g] of elements)if(!objects.has(id)){g.remove();elements.delete(id);selected.delete(id);measures.delete(id);}
  for(const o of objects.values())draw(o);
  drawMarks();drawHandle();
  $('counts').textContent=`${objects.size} objects${queue.length?' · '+queue.length+' queued':''}`;
  $('physics').textContent=physics.enabled?'Motion on':'Motion off';
  dirty=false;
}

// ---------- command execution ----------
async function execute(e,generation){
  const c=e.command||{};
  if(e.op==='view'||e.op==='fit'){
    $('activity').textContent=e.op;
    await animateCamera(viewTargetFor(e.op==='fit'?{fit:true}:{...c,selected:e.selected}),c.duration??300,generation);
    if(generation!==epoch)return;
    seq=e.seq;ack('firstFrame',seq);ack('done',seq,queue.length===0);$('activity').textContent='Ready';dirty=true;return;
  }
  const moveBy=c.by||(e.op==='move'&&c.to&&objects.has(e.selected[0])?[c.to[0]-(objects.get(e.selected[0]).x||0),c.to[1]-(objects.get(e.selected[0]).y||0)]:null);
  const old=new Map();
  const targets=e.patch.upsert.map(o=>{
    const current=objects.get(o.id);if(!current)return o;
    if(e.op==='undo'||e.op==='redo')return o;
    const next={...current};for(const field of e.patch.fields[o.id]||[])next[field]=o[field];
    if(e.op==='move'&&moveBy){next.x=(current.x||0)+moveBy[0];next.y=(current.y||0)+moveBy[1];}
    if(e.op==='impulse'&&c.velocity){next.vx=(current.vx||0)+c.velocity[0];next.vy=(current.vy||0)+c.velocity[1];}
    return next;
  });
  const removes=e.patch.remove,ids=[...new Set([...targets.map(o=>o.id),...removes])];
  const duration=e.duration||0,stagger=Math.min(e.stagger||0,1000/Math.max(1,ids.length-1)),total=duration+stagger*Math.max(0,ids.length-1);
  const motionLocks=new Set();
  for(const id of ids){
    if(objects.has(id))old.set(id,structuredClone(objects.get(id)));
    if(['move','layout','create','spawn','remove','clear','undo','redo'].includes(e.op)){const root=rootFor(id,objects);locked.add(root);motionLocks.add(root);}
  }
  for(const o of targets)if(!objects.has(o.id))objects.set(o.id,{...o,opacity:duration?0:(o.opacity??1)});
  physics={...e.patch.physics};
  $('activity').textContent=`${e.op} · ${ids.length||e.selected.length} objects`;
  let start,first=false;
  do{
    const now=await frame();if(generation!==epoch)return;if(start===undefined)start=now-16;
    const elapsed=now-start;
    for(let index=0;index<ids.length;index++){
      const id=ids[index],before=old.get(id),after=targets.find(o=>o.id===id);
      const t=duration?Math.max(0,Math.min(1,(elapsed-index*stagger)/duration)):(elapsed>=index*stagger?1:0),ease=1-(1-t)**3;
      if(removes.includes(id)){
        if(t>=1)objects.delete(id);else if(before)objects.set(id,{...before,opacity:(before.opacity??1)*(1-ease)});
      }else if(after){
        if(!before)objects.set(id,{...after,opacity:(after.opacity??1)*ease});
        else{
          const next={...objects.get(id)};const fields=['undo','redo'].includes(e.op)?Object.keys(after):e.patch.fields[id]||[];
          for(const k of fields)next[k]=interpolate(before[k]??after[k],after[k],ease);
          objects.set(id,next);
        }
      }
    }
    dirty=true;render();
    if(first===false){first=true;requestAnimationFrame(()=>{if(generation===epoch)ack('firstFrame',e.seq);});}
    if(elapsed>=total)break;
  }while(true);
  for(const o of targets){
    if(!old.has(o.id)||['undo','redo'].includes(e.op))objects.set(o.id,structuredClone(o));
    else{const current=objects.get(o.id);for(const k of e.patch.fields[o.id]||[]){if(o[k]===undefined)delete current[k];else current[k]=structuredClone(o[k]);}}
  }
  for(const id of removes)objects.delete(id);
  for(const id of motionLocks)if(drag?.id!==id)locked.delete(id);
  dirty=true;render();seq=e.seq;
  await frame();if(generation!==epoch)return;
  ack('done',seq,queue.length===0);$('activity').textContent='Ready';
}
async function drain(){
  if(running)return;running=true;const generation=epoch;
  try{while(queue.length&&generation===epoch)await execute(queue.shift(),generation);}
  catch(e){notify(e.message);}
  finally{if(generation===epoch)running=false;}
}

// ---------- stream ----------
const stream=new EventSource('/events');
stream.onopen=()=>$('connection').textContent='connected';
stream.onerror=()=>$('connection').textContent='reconnecting';
stream.addEventListener('snapshot',e=>{
  epoch++;running=false;queue.length=0;locked.clear();
  const data=JSON.parse(e.data);
  objects.clear();for(const [id,o] of Object.entries(data.scene.objects))objects.set(id,o);
  physics=data.scene.physics;seq=data.seq;marks=data.marks||[];
  dirty=true;render();
  if(fitOnLoad){fit();fitOnLoad=false;}
  ack('snapshot',seq);
});
stream.addEventListener('marks',e=>{marks=JSON.parse(e.data);drawMarks();});
stream.onmessage=e=>{queue.push(JSON.parse(e.data));drain();};

let previous=performance.now();
function tick(now){
  const elapsed=(now-previous)/1000;previous=now;
  if(stepPhysics(objects,physics,elapsed,locked))dirty=true;
  if(dirty)render();
  if(!running&&!queue.length&&now-checkpointAt>1500){checkpointAt=now;ack('checkpoint',seq,true);}
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);

// ---------- marks (human → model pointers) ----------
function setPointMode(on){
  pointMode=on;$('point').classList.toggle('active',on);canvas.classList.toggle('pointing',on);
  if(!on)hideNoteInput();
}
function positionNoteInput(){
  const input=$('mark-note');if(input.hidden||!noteTarget)return;
  const p=toScreen(noteTarget.x,noteTarget.y);input.style.left=(p.x+18)+'px';input.style.top=(p.y-14)+'px';
}
function showNoteInput(mark){
  noteTarget=mark;const input=$('mark-note');input.value='';input.hidden=false;positionNoteInput();input.focus();
}
function hideNoteInput(){$('mark-note').hidden=true;noteTarget=null;}
async function commitNote(){
  const input=$('mark-note'),mark=noteTarget;if(!mark)return;
  const note=input.value.trim();hideNoteInput();
  if(note)await postMark({update:{batch:mark.batch,n:mark.n},note}).catch(()=>{});
}
async function placeMark(e){
  const raw=e.target.closest('[data-id]')?.dataset.id,target=raw?rootFor(raw,objects):null;
  const p=toWorld(e.clientX,e.clientY);
  const data=await postMark({x:p.x,y:p.y,target});
  showNoteInput(data.mark);
}
$('mark-note').onkeydown=e=>{e.stopPropagation();if(e.key==='Enter'){e.preventDefault();commitNote();}if(e.key==='Escape'){e.preventDefault();hideNoteInput();}};
$('mark-note').onblur=()=>{if(noteTarget)commitNote();};
$('point').onclick=()=>setPointMode(!pointMode);
$('clear-marks').onclick=()=>postMark({clear:true}).catch(()=>{});

// ---------- pointer interaction ----------
canvas.onpointerdown=e=>{
  if(e.button!==0)return;
  if(noteTarget){commitNote();}
  if(pointMode||e.altKey){placeMark(e).catch(()=>{});return;}
  const handleId=e.target.dataset?.handle;
  if(handleId){
    const o=objects.get(handleId),b=bounds(o,objects);
    drag={resize:handleId,sx:e.clientX,sy:e.clientY,w:b.w,h:b.h,moved:false};
    locked.add(handleId);canvas.setPointerCapture(e.pointerId);return;
  }
  const raw=e.target.closest('[data-id]')?.dataset.id,id=raw?rootFor(raw,objects):null;
  if(id){if(!e.shiftKey&&!selected.has(id))selected.clear();if(e.shiftKey&&selected.has(id))selected.delete(id);else selected.add(id);dirty=true;}
  else if(!e.shiftKey){selected.clear();dirty=true;}
  const o=objects.get(id);
  drag={id,sx:e.clientX,sy:e.clientY,lastX:e.clientX,lastY:e.clientY,moved:false,x:o?o.x||0:view.x,y:o?o.y||0:view.y};
  if(id)locked.add(id);canvas.setPointerCapture(e.pointerId);
};
canvas.onpointermove=e=>{
  if(!drag)return;
  if(drag.resize){
    const o=objects.get(drag.resize);if(!o)return;
    o.width=Math.max(40,Math.round(drag.w+(e.clientX-drag.sx)/view.z));
    o.height=Math.max(30,Math.round(drag.h+(e.clientY-drag.sy)/view.z));
    drag.moved=true;dirty=true;return;
  }
  const dx=e.clientX-drag.lastX,dy=e.clientY-drag.lastY;drag.lastX=e.clientX;drag.lastY=e.clientY;
  if(Math.abs(e.clientX-drag.sx)+Math.abs(e.clientY-drag.sy)>3)drag.moved=true;
  if(drag.id){translate(drag.id,dx/view.z,dy/view.z,objects);dirty=true;}
  else{view.x+=dx;view.y+=dy;transform();}
};
canvas.onpointerup=()=>{
  if(drag?.resize){
    const o=objects.get(drag.resize);locked.delete(drag.resize);
    if(drag.moved&&o)send({op:'set',select:drag.resize,props:{width:o.width,height:o.height}}).catch(()=>{});
  }else if(drag?.id){
    const o=objects.get(drag.id);locked.delete(drag.id);
    if(drag.moved)send({op:'move',select:drag.id,to:[o.x,o.y]}).catch(()=>{});
  }
  drag=null;
};
canvas.onpointercancel=()=>{if(drag?.id)locked.delete(drag.id);if(drag?.resize)locked.delete(drag.resize);drag=null;};
canvas.onwheel=e=>{
  e.preventDefault();
  const z=Math.max(.08,Math.min(5,view.z*Math.exp(-e.deltaY*.001)));
  view.x=e.clientX-(e.clientX-view.x)*z/view.z;view.y=e.clientY-(e.clientY-view.y)*z/view.z;view.z=z;transform();
};
canvas.ondblclick=e=>{
  const id=e.target.closest('[data-id]')?.dataset.id,o=objects.get(id);if(!o)return;
  editorId=id;$('guide').hidden=false;$('editor').hidden=false;
  $('title').value=o.title||'';$('text').value=o.text||'';$('selection').textContent=id;$('title').focus();
};
$('editor').onsubmit=e=>{e.preventDefault();send({op:'set',select:editorId,props:{title:$('title').value,text:$('text').value}}).catch(()=>{});$('editor').hidden=true;};
$('fit').onclick=fit;
$('undo').onclick=()=>send({op:'undo'}).catch(()=>{});
$('redo').onclick=()=>send({op:'redo'}).catch(()=>{});
$('physics').onclick=()=>send({op:'physics',props:{enabled:!physics.enabled}}).catch(()=>{});
$('help').onclick=()=>$('guide').hidden=!$('guide').hidden;
$('close-guide').onclick=()=>$('guide').hidden=true;
$('send').onclick=async()=>{try{$('result').textContent=JSON.stringify(await send(JSON.parse($('command').value)));}catch(e){$('result').textContent=e.message;}};
addEventListener('keydown',e=>{
  if(['INPUT','TEXTAREA'].includes(document.activeElement.tagName))return;
  if(e.key==='Delete'&&selected.size){send({op:'remove',select:[...selected],duration:180,stagger:20}).catch(()=>{});selected.clear();}
  if(e.code==='Space'){e.preventDefault();$('physics').click();}
  if(e.key==='m'||e.key==='M')setPointMode(!pointMode);
  if(e.key==='Escape'&&pointMode)setPointMode(false);
  if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='z'){e.preventDefault();send({op:e.shiftKey?'redo':'undo'}).catch(()=>{});}
});
addEventListener('resize',()=>transform());
transform();
// Read-only diagnostics for local browser verification.
window.boardDiagnostics={state:()=>({seq,queue:queue.length,running,objects:Object.fromEntries(objects),physics,marks,view:viewSummary(),selected:[...selected],pointMode}),client};
