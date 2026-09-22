import {resolve,route,headGeometry,rootFor,stepPhysics,interpolate,translate,centreOf,edgeEndpoints} from './engine.js';

const $=id=>document.getElementById(id);
const NS='http://www.w3.org/2000/svg';
const world=$('world'),canvas=$('canvas');
const objects=new Map(),elements=new Map(),selected=new Set(),locked=new Set(),queue=[];
let physics={},seq=0,running=false,dirty=true,epoch=0,drag=null,editorId=null,fitOnLoad=true,checkpointAt=0;
let marks=[],pointMode=false,noteTarget=null,layout={boxes:new Map(),locals:new Map(),measured:new Map()},presets={};
const client=crypto.randomUUID();
const view=JSON.parse(sessionStorage.getItem('idea-board-view')||'null')||{x:innerWidth/2,y:innerHeight/2,z:1};
const edgeLayer=svg('g',{id:'edges'}),shapeLayer=svg('g',{id:'shapes'}),marksLayer=svg('g',{id:'marks',class:'marks'});
world.append(edgeLayer,shapeLayer,marksLayer);
const BATCH_COLORS=['#6ee7a0','#ff7a7a','#7ab8ff','#c98bff','#ffb35c'];

function svg(tag,attrs={}){const e=document.createElementNS(NS,tag);attrsTo(e,attrs);return e;}
function attrsTo(e,attrs){for(const [k,v] of Object.entries(attrs))if(v!==undefined)e.setAttribute(k,v);}
function notify(message){$('toast').textContent=message;$('toast').style.display='block';setTimeout(()=>$('toast').style.display='none',6000);}
const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve));
const isRoot=o=>o.parent===undefined||o.parent===null;

// ---------- text measurement (the only browser-dependent size) ----------
const measureCache=new Map();
function textStyle(div,o){
  div.style.fontSize=(o.size||14)+'px';
  div.style.fontFamily=o.font==='mono'?'ui-monospace,monospace':'system-ui,sans-serif';
  div.style.fontWeight=o.weight||400;
  div.style.textAlign=o.align||'left';
  div.style.lineHeight='1.5';
}
function measureText(o,w){
  const key=JSON.stringify([o.text||'',w,o.size||14,o.font||'sans',o.weight||400]);
  if(measureCache.has(key))return measureCache.get(key);
  const probe=$('measure');probe.style.width=w+'px';textStyle(probe,o);probe.textContent=o.text||'';
  const r={w:Math.ceil(probe.scrollWidth),h:Math.ceil(probe.offsetHeight)};
  if(measureCache.size>5000)measureCache.clear();
  measureCache.set(key,r);return r;
}

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
  const boxes=(ids||[...objects.keys()]).map(i=>layout.boxes.get(i)).filter(Boolean);
  if(!boxes.length)return null;
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
  const current=viewSummary(),z=c.zoom??view.z;
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
function ack(stage,n,full=false){
  const body={client,stage,seq:n,time:Date.now(),visible:!document.hidden,view:viewSummary()};
  if(full){
    body.positions=Object.fromEntries([...objects.values()].filter(isRoot).map(o=>[o.id,{x:o.x||0,y:o.y||0,vx:o.vx||0,vy:o.vy||0}]));
    body.boxes=Object.fromEntries([...layout.boxes].map(([id,b])=>[id,{x:Math.round(b.x*10)/10,y:Math.round(b.y*10)/10,w:Math.round(b.w*10)/10,h:Math.round(b.h*10)/10}]));
    body.measured=Object.fromEntries(layout.measured);
  }
  fetch('/ack',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)}).catch(()=>{});
}
async function postMark(body){
  const r=await fetch('/marks',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  const data=await r.json();if(!r.ok){notify(data.error);throw Error(data.error);}return data;
}

// ---------- drawing ----------
const signatureOf=(o,local)=>JSON.stringify([o,local.w,local.h],(k,v)=>k==='box'||k==='measured'?undefined:v);
function styleAttrs(o,defaultFill){
  const a={stroke:o.color||'#8eaede','stroke-width':o.strokeWidth??1.6,fill:o.fill??defaultFill};
  if(o.dash)a['stroke-dasharray']=o.dash.join(' ');
  return a;
}
function drawHeads(g,o,color,strokeWidth,start,end,tangentStart,tangentEnd){
  const size=Math.max(8,strokeWidth*4+5);
  const heads=[[o.head??'none',end,tangentEnd,1],[o.tail??'none',start,{x:-tangentStart.x,y:-tangentStart.y},-1]];
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
function pathFor(points,control,closed,smooth){
  if(control)return `M${points[0].x} ${points[0].y}Q${control.x} ${control.y} ${points[1].x} ${points[1].y}`;
  if(smooth&&points.length>2){
    // Catmull-Rom → cubic Bézier
    const pts=closed?[points[points.length-1],...points,points[0],points[1]]:[points[0],...points,points[points.length-1]];
    let d=`M${pts[1].x} ${pts[1].y}`;
    for(let i=1;i<pts.length-2;i++){
      const p0=pts[i-1],p1=pts[i],p2=pts[i+1],p3=pts[i+2];
      d+=`C${p1.x+(p2.x-p0.x)/6} ${p1.y+(p2.y-p0.y)/6} ${p2.x-(p3.x-p1.x)/6} ${p2.y-(p3.y-p1.y)/6} ${p2.x} ${p2.y}`;
    }
    return d+(closed?'Z':'');
  }
  return points.map((p,i)=>(i?'L':'M')+p.x+' '+p.y).join(' ')+(closed?'Z':'');
}
function labelAt(g,text,x,y,color){const t=svg('text',{x,y:y-10,fill:color,'text-anchor':'middle'});t.textContent=text;g.append(t);}
function drawEdge(g,o){
  const r=route(o,objects,layout.boxes);if(!r)return;
  const color=o.color||'#8eaede',strokeWidth=o.strokeWidth??1.6;
  const trimmed=drawHeads(g,o,color,strokeWidth,r.start,r.end,r.tangentStart,r.tangentEnd);
  const points=[...r.points];points[0]=trimmed.start;points[points.length-1]=trimmed.end;
  g.append(svg('path',{class:'shape',d:pathFor(points,r.control),fill:'none',...styleAttrs(o,'none'),'stroke-linecap':'round','stroke-linejoin':'round'}));
  if(o.label){
    let mid;
    if(r.control)mid={x:.25*r.start.x+.5*r.control.x+.25*r.end.x,y:.25*r.start.y+.5*r.control.y+.25*r.end.y};
    else if(points.length===4)mid={x:(points[1].x+points[2].x)/2,y:(points[1].y+points[2].y)/2};
    else mid={x:(r.start.x+r.end.x)/2,y:(r.start.y+r.end.y)/2};
    labelAt(g,o.label,mid.x,mid.y,color);
  }
}
function drawPolyline(g,o){
  const raw=(o.points||[[0,0],[120,0]]).map(p=>({x:p[0],y:p[1]})),n=raw.length;
  const color=o.color||'#8eaede',strokeWidth=o.strokeWidth??1.6;
  const unit=(p,q)=>{const dx=q.x-p.x,dy=q.y-p.y,d=Math.hypot(dx,dy)||1;return {x:dx/d,y:dy/d};};
  const trimmed=drawHeads(g,o,color,strokeWidth,raw[0],raw[n-1],unit(raw[0],raw[1]||raw[0]),unit(raw[n-2]||raw[0],raw[n-1]));
  const points=[...raw];points[0]=trimmed.start;points[n-1]=trimmed.end;
  g.append(svg('path',{class:'shape',d:pathFor(points,null,false,o.smooth),...styleAttrs(o,'none'),fill:'none','stroke-linecap':'round','stroke-linejoin':'round'}));
}
function drawText(g,o,local){
  const fo=svg('foreignObject',{width:local.w,height:local.h});
  const div=document.createElement('div');div.className='text';textStyle(div,o);div.style.color=o.color||'#dde5f1';div.style.width=local.w+'px';div.style.height=local.h+'px';
  div.textContent=o.text||'';fo.append(div);g.append(fo);
}
function draw(o){
  let g=elements.get(o.id);
  if(!g){g=svg('g',{'data-id':o.id});elements.set(o.id,g);}
  const parentEl=o.type==='edge'?edgeLayer:isRoot(o)?shapeLayer:elements.get(o.parent)||shapeLayer;
  if(g.parentNode!==parentEl)parentEl.append(g);
  const local=layout.locals.get(o.id)||{x:o.x||0,y:o.y||0,w:0,h:0,bx:0,by:0};
  attrsTo(g,{class:'object '+o.type+(selected.has(o.id)?' selected':''),opacity:o.opacity??1});
  if(o.type==='edge'){g.removeAttribute('transform');g.replaceChildren();drawEdge(g,o);return;}
  g.setAttribute('transform',`translate(${local.x} ${local.y})`);
  const signature=signatureOf(o,local);
  if(g.dataset.signature===signature)return;
  g.dataset.signature=signature;
  // Keep child groups; rebuild only this object's own geometry (children are separate <g data-id> elements).
  for(const child of [...g.children])if(!child.dataset?.id)child.remove();
  const own=[];
  const w=local.w,h=local.h,bx=local.bx,by=local.by;
  if(o.type==='rect')own.push(svg('rect',{class:'shape',width:w,height:h,rx:o.rx??0,...styleAttrs(o,'#192435')}));
  else if(o.type==='ellipse')own.push(svg('ellipse',{class:'shape',cx:w/2,cy:h/2,rx:w/2,ry:h/2,...styleAttrs(o,o.color||'#8eaede')}));
  else if(o.type==='polygon')own.push(svg('path',{class:'shape',d:pathFor((o.points||[]).map(p=>({x:p[0],y:p[1]})),null,true,o.smooth),...styleAttrs(o,o.color||'#8eaede'),'stroke-linejoin':'round'}));
  else if(o.type==='polyline'){const tmp=svg('g');drawPolyline(tmp,o);own.push(...tmp.children);}
  else if(o.type==='text'){const tmp=svg('g');drawText(tmp,o,local);own.push(...tmp.children);}
  else if(o.type==='group'){
    if(o.outline||o.title){
      own.push(svg('rect',{class:'shape outline',x:bx-20,y:by-28,width:w+40,height:h+48,rx:15,fill:'none',stroke:o.color||'#8eaede','stroke-width':o.strokeWidth??1.6,'stroke-dasharray':'7 5'}));
      if(o.title){const t=svg('text',{x:bx-9,y:by-36,fill:o.color||'#8eaede'});t.textContent=o.title;own.push(t);}
    }else own.push(svg('rect',{class:'hit',x:bx,y:by,width:w,height:h,fill:'transparent',stroke:'none'}));
    const m=layout.measured.get(o.id);
    if(m?.overflow)own.push(svg('rect',{class:'overflow',x:bx,y:by,width:w,height:h,fill:'none'}));
  }
  const m=layout.measured.get(o.id);
  if(o.type==='text'&&m?.overflow)own.push(svg('rect',{class:'overflow',x:0,y:0,width:w,height:h,fill:'none'}));
  g.prepend(...own);
}
function drawHandle(){
  for(const h of world.querySelectorAll('.handle'))h.remove();
  if(selected.size!==1)return;
  const id=[...selected][0],o=objects.get(id),g=elements.get(id),local=layout.locals.get(id);
  if(!o||!g||!local||o.type==='edge'||o.type==='polygon'||o.type==='polyline'||o.type==='ellipse'&&o.preset)return;
  const s=12/view.z;
  g.append(svg('rect',{class:'handle',x:local.bx+local.w-s/2,y:local.by+local.h-s/2,width:s,height:s,rx:s/4,'data-handle':id}));
}
function drawMarks(){
  marksLayer.replaceChildren();
  const batches=[...new Set(marks.map(m=>m.batch))].sort((a,b)=>a-b),z=view.z;
  for(const m of marks){
    const rank=batches.length-1-batches.indexOf(m.batch);
    const color=BATCH_COLORS[(m.batch-1)%BATCH_COLORS.length];
    const opacity=m.read?Math.max(.12,.55-rank*.12):.95;
    let x=m.x,y=m.y;
    const target=m.target&&objects.get(m.target),box=target&&layout.boxes.get(m.target);
    if(box&&m.offset){x=box.ox+m.offset[0];y=box.oy+m.offset[1];}
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
function orderedObjects(){
  // Parents before children so DOM parents exist; roots in registry order, children in `children` order.
  const out=[];const visit=id=>{const o=objects.get(id);if(!o)return;out.push(o);for(const c of o.children||[])visit(c);};
  for(const o of objects.values())if(isRoot(o)&&o.type!=='edge')visit(o.id);
  for(const o of objects.values())if(o.type==='edge')out.push(o);
  return out;
}
function render(){
  for(const [id,g] of elements)if(!objects.has(id)){g.remove();elements.delete(id);selected.delete(id);}
  layout=resolve(objects,measureText);
  for(const o of orderedObjects())draw(o);
  // DOM order follows children order for z-ordering.
  for(const o of objects.values())if(o.type==='group')for(const c of o.children||[]){const el=elements.get(c);if(el&&el.parentNode===elements.get(o.id))elements.get(o.id).append(el);}
  drawMarks();drawHandle();
  $('counts').textContent=`${objects.size} objects${queue.length?' · '+queue.length+' queued':''}`;
  $('physics').textContent=physics.enabled?'Motion on':'Motion off';
  dirty=false;
}

// ---------- command execution ----------
async function execute(e,generation){
  const c=e.command||{};
  if(e.op==='view'){
    $('activity').textContent='view';
    await animateCamera(viewTargetFor({...c,selected:e.selected}),c.duration??300,generation);
    if(generation!==epoch)return;
    seq=e.seq;ack('firstFrame',seq);ack('done',seq,queue.length===0);$('activity').textContent='Ready';dirty=true;return;
  }
  const old=new Map();
  const targets=e.patch.upsert.map(o=>{
    const current=objects.get(o.id);if(!current)return o;
    if(e.op==='undo'||e.op==='redo')return o;
    const next={...current};for(const field of e.patch.fields[o.id]||[])next[field]=o[field];
    if(e.op==='move'){next.x=o.x;next.y=o.y;}
    if(e.op==='impulse'&&c.velocity){next.vx=(current.vx||0)+c.velocity[0];next.vy=(current.vy||0)+c.velocity[1];}
    return next;
  });
  const removes=e.patch.remove,ids=[...new Set([...targets.map(o=>o.id),...removes])];
  const duration=e.duration||0,stagger=Math.min(e.stagger||0,1000/Math.max(1,ids.length-1)),total=duration+stagger*Math.max(0,ids.length-1);
  const motionLocks=new Set();
  for(const id of ids){
    if(objects.has(id))old.set(id,structuredClone(objects.get(id)));
    if(['move','layout','create','remove','clear','undo','redo','reparent','group','ungroup'].includes(e.op)){const root=rootFor(id,objects);locked.add(root);motionLocks.add(root);}
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
  catch(e){notify(e.message);console.error(e);}
  finally{if(generation===epoch)running=false;}
}

// ---------- stream ----------
const stream=new EventSource('/events');
stream.onopen=()=>$('connection').textContent='connected';
stream.onerror=()=>$('connection').textContent='reconnecting';
stream.addEventListener('snapshot',async e=>{
  epoch++;running=false;queue.length=0;locked.clear();
  const data=JSON.parse(e.data);
  await document.fonts.ready;measureCache.clear();
  objects.clear();for(const [id,o] of Object.entries(data.scene.objects))objects.set(id,o);
  physics=data.scene.physics;seq=data.seq;marks=data.marks||[];presets=data.presets||{};
  dirty=true;render();
  if(fitOnLoad){fit();fitOnLoad=false;}
  ack('snapshot',seq);
});
stream.addEventListener('marks',e=>{marks=JSON.parse(e.data);drawMarks();});
stream.onmessage=e=>{queue.push(JSON.parse(e.data));drain();};

let previous=performance.now();
function tick(now){
  const elapsed=(now-previous)/1000;previous=now;
  if(stepPhysics(objects,physics,elapsed,locked,layout.boxes))dirty=true;
  if(dirty)render();
  if(!running&&!queue.length&&now-checkpointAt>1500){checkpointAt=now;ack('checkpoint',seq,true);}
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);

// ---------- marks (human → model pointers) ----------
function setPointMode(on){pointMode=on;$('point').classList.toggle('active',on);canvas.classList.toggle('pointing',on);if(!on)hideNoteInput();}
function positionNoteInput(){
  const input=$('mark-note');if(input.hidden||!noteTarget)return;
  const p=toScreen(noteTarget.x,noteTarget.y);input.style.left=(p.x+18)+'px';input.style.top=(p.y-14)+'px';
}
function showNoteInput(mark){noteTarget=mark;const input=$('mark-note');input.value='';input.hidden=false;positionNoteInput();input.focus();}
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
function pickId(e){
  const raw=e.target.closest('[data-id]')?.dataset.id;if(!raw)return null;
  return e.ctrlKey||e.metaKey?raw:rootFor(raw,objects);
}
canvas.onpointerdown=e=>{
  if(e.button!==0)return;
  if(noteTarget)commitNote();
  if(pointMode||e.altKey){placeMark(e).catch(()=>{});return;}
  const handleId=e.target.dataset?.handle;
  if(handleId){
    const local=layout.locals.get(handleId);
    drag={resize:handleId,sx:e.clientX,sy:e.clientY,w:local.w,h:local.h,moved:false};
    locked.add(rootFor(handleId,objects));canvas.setPointerCapture(e.pointerId);return;
  }
  const id=pickId(e);
  if(id){if(!e.shiftKey&&!selected.has(id))selected.clear();if(e.shiftKey&&selected.has(id))selected.delete(id);else selected.add(id);dirty=true;}
  else if(!e.shiftKey){selected.clear();dirty=true;}
  const o=objects.get(id);
  drag={id,sx:e.clientX,sy:e.clientY,lastX:e.clientX,lastY:e.clientY,moved:false};
  if(id&&o){const p=objects.get(o.parent);if(p?.layout){drag.id=null;drag.pan=false;}else locked.add(rootFor(id,objects));}
  canvas.setPointerCapture(e.pointerId);
};
canvas.onpointermove=e=>{
  if(!drag)return;
  if(drag.resize){
    const o=objects.get(drag.resize);if(!o)return;
    o.w=Math.max(20,Math.round(drag.w+(e.clientX-drag.sx)/view.z));
    if(typeof o.h==='number'||o.type!=='group')o.h=Math.max(16,Math.round(drag.h+(e.clientY-drag.sy)/view.z));
    if(o.type==='ellipse'&&o.r!==undefined){delete o.r;}
    drag.moved=true;dirty=true;return;
  }
  const dx=e.clientX-drag.lastX,dy=e.clientY-drag.lastY;drag.lastX=e.clientX;drag.lastY=e.clientY;
  if(Math.abs(e.clientX-drag.sx)+Math.abs(e.clientY-drag.sy)>3)drag.moved=true;
  if(drag.id){translate(drag.id,dx/view.z,dy/view.z,objects,layout.boxes);dirty=true;}
  else if(drag.pan!==false){view.x+=dx;view.y+=dy;transform();}
};
canvas.onpointerup=()=>{
  if(drag?.resize){
    const o=objects.get(drag.resize);locked.delete(rootFor(drag.resize,objects));
    if(drag.moved&&o){
      const props={w:o.w};if(typeof o.h==='number')props.h=o.h;
      send({op:'set',select:drag.resize,props}).catch(()=>{});
    }
  }else if(drag?.id){
    const o=objects.get(drag.id);locked.delete(rootFor(drag.id,objects));
    if(drag.moved&&o)send({op:'move',select:drag.id,to:[o.x,o.y]}).catch(()=>{});
  }
  drag=null;
};
canvas.onpointercancel=()=>{if(drag?.id)locked.delete(rootFor(drag.id,objects));if(drag?.resize)locked.delete(rootFor(drag.resize,objects));drag=null;};
canvas.onwheel=e=>{
  e.preventDefault();
  const z=Math.max(.08,Math.min(5,view.z*Math.exp(-e.deltaY*.001)));
  view.x=e.clientX-(e.clientX-view.x)*z/view.z;view.y=e.clientY-(e.clientY-view.y)*z/view.z;view.z=z;transform();
};
function editableTextOf(id){
  // Returns {target, key} for the text a double-click should edit.
  const o=objects.get(id);if(!o)return null;
  if(o.preset&&o.params!==undefined&&presets[o.preset]&&'text' in presets[o.preset].params)return {target:id,title:'title' in presets[o.preset].params,params:true};
  if(o.type==='text')return {target:id,title:false,params:false};
  for(const c of o.children||[]){const child=objects.get(c);if(child?.type==='text')return {target:c,title:false,params:false};}
  return null;
}
canvas.ondblclick=e=>{
  const raw=e.target.closest('[data-id]')?.dataset.id;if(!raw)return;
  const id=rootFor(raw,objects),edit=editableTextOf(id)||editableTextOf(raw);if(!edit)return;
  const o=objects.get(edit.target);editorId=edit;
  $('guide').hidden=false;$('editor').hidden=false;$('title').parentElement.hidden=!edit.title;
  $('title').value=edit.params?(o.params?.title||''):'';$('text').value=edit.params?(o.params?.text||''):(o.text||'');$('selection').textContent=edit.target;
  (edit.title?$('title'):$('text')).focus();
};
$('editor').onsubmit=e=>{
  e.preventDefault();if(!editorId)return;
  const props=editorId.params?{text:$('text').value,...(editorId.title?{title:$('title').value}:{})}:{text:$('text').value};
  send({op:'set',select:editorId.target,props,resetOverrides:true}).catch(()=>{});$('editor').hidden=true;
};
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
window.boardDiagnostics={state:()=>({seq,queue:queue.length,running,objects:Object.fromEntries(objects),physics,marks,view:viewSummary(),selected:[...selected],pointMode,boxes:Object.fromEntries(layout.boxes),measured:Object.fromEntries(layout.measured)}),client};
