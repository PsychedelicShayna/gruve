import {bounds,translate,rootFor,stepPhysics,interpolate} from './engine.js';
const $=id=>document.getElementById(id),NS='http://www.w3.org/2000/svg',world=$('world'),canvas=$('canvas');
const objects=new Map(),elements=new Map(),selected=new Set(),locked=new Set(),queue=[];
let physics={},seq=0,running=false,dirty=true,epoch=0,drag=null,editorId=null,fitOnLoad=true,checkpointAt=0;
const client=crypto.randomUUID(),view=JSON.parse(sessionStorage.getItem('idea-board-view')||'null')||{x:innerWidth/2,y:innerHeight/2,z:1};
function svg(tag,attrs={}){const e=document.createElementNS(NS,tag);attrsTo(e,attrs);return e;}
function attrsTo(e,attrs){for(const [k,v]of Object.entries(attrs))if(v!==undefined)e.setAttribute(k,v);}
function transform(){world.setAttribute('transform',`translate(${view.x} ${view.y}) scale(${view.z})`);sessionStorage.setItem('idea-board-view',JSON.stringify(view));}transform();
function notify(message){$('toast').textContent=message;$('toast').style.display='block';setTimeout(()=>$('toast').style.display='none',5000);}
async function send(commands){const r=await fetch('/commands',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(commands)});const data=await r.json();if(!r.ok){notify(data.error);throw Error(data.error);}return data;}
function positionSnapshot(){return Object.fromEntries([...objects].map(([id,o])=>[id,{x:o.x||0,y:o.y||0,vx:o.vx||0,vy:o.vy||0}]));}
function ack(stage,n,positions=false){fetch('/ack',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({client,stage,seq:n,time:Date.now(),visible:!document.hidden,...(positions?{positions:positionSnapshot()}:{})})}).catch(()=>{});}
function center(id){const o=objects.get(id);if(!o)return {x:0,y:0};const b=bounds(o,objects);return {x:b.x+b.w/2,y:b.y+b.h/2};}
function draw(o){
 let g=elements.get(o.id);
 if(!g){g=svg('g',{'data-id':o.id});elements.set(o.id,g);world.append(g);}
 attrsTo(g,{class:'object'+(selected.has(o.id)?' selected':''),opacity:o.opacity??1,transform:`translate(${o.x||0} ${o.y||0})`});
 // Preserve the object group itself. Geometry children change only when its visual signature changes.
 const dependent=o.from||o.type==='group';const signature=JSON.stringify(o);
 if(!dependent&&g.dataset.signature===signature)return;g.dataset.signature=signature;g.replaceChildren();
 const b=bounds(o,objects),w=o.width||(o.type==='dot'?(o.radius||8)*2:260),h=o.height||(o.type==='dot'?(o.radius||8)*2:140),color=o.color||'#8eaede',fill=o.fill??(['dot','ellipse','polygon','diamond'].includes(o.type)?color:'#192435');
 const shape=(tag,a)=>g.append(svg(tag,{class:'shape',stroke:color,'stroke-width':o.strokeWidth||1.6,...a}));
 if(o.type==='group'){
   g.removeAttribute('transform');if(o.outline!==false){shape('rect',{x:b.x-20,y:b.y-28,width:b.w+40,height:b.h+48,rx:15,fill:'none','stroke-dasharray':'7 5'});const t=svg('text',{x:b.x-9,y:b.y-36,fill:color});t.textContent=o.title||o.text||'';g.append(t);}world.prepend(g);return;
 }
 if(o.from){g.removeAttribute('transform');const a=center(o.from),b=center(o.to);shape('line',{x1:a.x,y1:a.y,x2:b.x,y2:b.y,...(o.type==='arrow'?{'marker-end':'url(#arrow)'}:{})});if(o.text){const t=svg('text',{x:(a.x+b.x)/2,y:(a.y+b.y)/2-12,fill:color,'text-anchor':'middle'});t.textContent=o.text;g.append(t);}world.prepend(g);return;}
 if(o.type==='dot'||o.type==='ellipse')shape('ellipse',{cx:w/2,cy:h/2,rx:w/2,ry:h/2,fill});
 else if(o.type==='diamond')shape('polygon',{points:`${w/2},0 ${w},${h/2} ${w/2},${h} 0,${h/2}`,fill});
 else if(o.type==='polygon')shape('polygon',{points:(o.points||[]).map(p=>p.join(',')).join(' '),fill});
 else if(['line','arrow','path'].includes(o.type)){
   const points=o.points||[[0,0],[120,0]];shape('path',{d:points.map((p,i)=>(i?'L':'M')+p.join(' ')).join(' ')+(o.closed?'Z':''),fill:o.closed?fill:'none','stroke-linecap':'round','stroke-linejoin':'round',...(o.type==='arrow'?{'marker-end':'url(#arrow)'}:{})});
 }else{
   if(o.type!=='text')shape('rect',{width:w,height:h,rx:o.type==='rectangle'?3:13,fill});
   const fo=svg('foreignObject',{width:w,height:h});const div=document.createElement('div');div.className='card';div.style.fontSize=(o.fontSize||14)+'px';if(o.type==='text')div.style.color=color;
   if(o.title){const strong=document.createElement('strong');strong.textContent=o.title;div.append(strong);}
   const content=document.createElement(o.type==='code'?'pre':'div');content.textContent=o.text||'';div.append(content);fo.append(div);g.append(fo);
 }
}
function render(){for(const [id,g]of elements)if(!objects.has(id)){g.remove();elements.delete(id);selected.delete(id);}for(const o of objects.values())draw(o);$('counts').textContent=`${objects.size} objects${queue.length?' · '+queue.length+' queued':''}`;$('physics').textContent=physics.enabled?'Motion on':'Motion off';dirty=false;}
function fit(){if(!objects.size)return;render();const box=world.getBBox();if(!box.width||!box.height)return;view.z=Math.min(1.3,(innerWidth-140)/box.width,(innerHeight-180)/box.height);view.x=innerWidth/2-(box.x+box.width/2)*view.z;view.y=innerHeight/2-(box.y+box.height/2)*view.z;transform();}
const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve));
async function execute(e,generation){
 const moveBy=e.by||(e.op==='move'&&e.to&&objects.has(e.selected[0])?[e.to[0]-(objects.get(e.selected[0]).x||0),e.to[1]-(objects.get(e.selected[0]).y||0)]:null);
 const old=new Map(),targets=e.patch.upsert.map(o=>{
   const current=objects.get(o.id);if(!current)return o;
   if(e.op==='undo'||e.op==='redo')return o;
   const next={...current};for(const field of e.patch.fields[o.id]||[])next[field]=o[field];
   if(e.op==='move'&&moveBy){next.x=(current.x||0)+moveBy[0];next.y=(current.y||0)+moveBy[1];}
   if(e.op==='impulse'&&e.velocity){next.vx=(current.vx||0)+e.velocity[0];next.vy=(current.vy||0)+e.velocity[1];}
   return next;
 }),removes=e.patch.remove,ids=[...new Set([...targets.map(o=>o.id),...removes])];
 const duration=e.duration||0,stagger=Math.min(e.stagger||0,1000/Math.max(1,ids.length-1)),total=duration+stagger*Math.max(0,ids.length-1);
 const motionLocks=new Set();
 for(const id of ids){if(objects.has(id))old.set(id,structuredClone(objects.get(id)));if(['move','layout','create','spawn','remove','clear','undo','redo'].includes(e.op)){const root=rootFor(id,objects);locked.add(root);motionLocks.add(root);}}
 for(const o of targets)if(!objects.has(o.id))objects.set(o.id,{...o,opacity:duration?0:(o.opacity??1)});
 physics={...e.patch.physics};$('activity').textContent=`${e.op} · ${ids.length||e.selected.length} objects`;
 let start;let first=false;
 do {
   const now=await frame();if(generation!==epoch)return;if(start===undefined)start=now-16;
   const elapsed=now-start;
   for(let index=0;index<ids.length;index++){
     const id=ids[index],before=old.get(id),after=targets.find(o=>o.id===id),t=duration?Math.max(0,Math.min(1,(elapsed-index*stagger)/duration)):(elapsed>=index*stagger?1:0),ease=1-(1-t)**3;
     if(removes.includes(id)){
       if(t>=1)objects.delete(id);else if(before)objects.set(id,{...before,opacity:(before.opacity??1)*(1-ease)});
     }else if(after){
       if(!before)objects.set(id,{...after,opacity:(after.opacity??1)*ease});
       else{const next={...objects.get(id)};const fields=['undo','redo'].includes(e.op)?Object.keys(after):e.patch.fields[id]||[];for(const k of fields)next[k]=interpolate(before[k]??after[k],after[k],ease);objects.set(id,next);}
     }
   }
   dirty=true;render();
   if(first===false){first=true;requestAnimationFrame(()=>{if(generation===epoch)ack('firstFrame',e.seq);});}
   if(elapsed>=total)break;
 }while(true);
 for(const o of targets){if(!old.has(o.id)||['undo','redo'].includes(e.op))objects.set(o.id,structuredClone(o));else{const current=objects.get(o.id);for(const k of e.patch.fields[o.id]||[])current[k]=structuredClone(o[k]);}}for(const id of removes)objects.delete(id);
 for(const id of motionLocks)if(drag?.id!==id)locked.delete(id);dirty=true;render();if(e.op==='fit')fit();seq=e.seq;
 await frame();if(generation!==epoch)return;
 ack('done',seq,queue.length===0);$('activity').textContent='Ready';
}
async function drain(){if(running)return;running=true;const generation=epoch;try{while(queue.length&&generation===epoch)await execute(queue.shift(),generation);}catch(e){notify(e.message);}finally{if(generation===epoch)running=false;}}
const stream=new EventSource('/events');stream.onopen=()=>$('connection').textContent='connected';stream.onerror=()=>$('connection').textContent='reconnecting';
stream.addEventListener('snapshot',e=>{epoch++;running=false;queue.length=0;locked.clear();const data=JSON.parse(e.data);objects.clear();for(const [id,o]of Object.entries(data.scene.objects))objects.set(id,o);physics=data.scene.physics;seq=data.seq;dirty=true;render();if(fitOnLoad){fit();fitOnLoad=false;}ack('snapshot',seq);});
stream.onmessage=e=>{const data=JSON.parse(e.data);queue.push(data);drain();};
let previous=performance.now();function tick(now){const elapsed=(now-previous)/1000;previous=now;if(stepPhysics(objects,physics,elapsed,locked))dirty=true;if(dirty)render();if(!running&&!queue.length&&now-checkpointAt>1500){checkpointAt=now;ack('checkpoint',seq,true);}requestAnimationFrame(tick);}requestAnimationFrame(tick);
canvas.onpointerdown=e=>{
 if(e.button!==0)return;const raw=e.target.closest('[data-id]')?.dataset.id,id=raw?rootFor(raw,objects):null;
 if(id){if(!e.shiftKey&&!selected.has(id))selected.clear();if(e.shiftKey&&selected.has(id))selected.delete(id);else selected.add(id);dirty=true;}
 else if(!e.shiftKey){selected.clear();dirty=true;}
 const o=objects.get(id);drag={id,sx:e.clientX,sy:e.clientY,lastX:e.clientX,lastY:e.clientY,moved:false,x:o?o.x||0:view.x,y:o?o.y||0:view.y};if(id)locked.add(id);canvas.setPointerCapture(e.pointerId);
};
canvas.onpointermove=e=>{if(!drag)return;const dx=e.clientX-drag.lastX,dy=e.clientY-drag.lastY;drag.lastX=e.clientX;drag.lastY=e.clientY;if(Math.abs(e.clientX-drag.sx)+Math.abs(e.clientY-drag.sy)>3)drag.moved=true;if(drag.id){translate(drag.id,dx/view.z,dy/view.z,objects);dirty=true;}else{view.x+=dx;view.y+=dy;transform();}};
canvas.onpointerup=()=>{if(drag?.id){const o=objects.get(drag.id);locked.delete(drag.id);if(drag.moved)send({op:'move',select:drag.id,to:[o.x,o.y]}).catch(()=>{});}drag=null;};canvas.onpointercancel=()=>{if(drag?.id)locked.delete(drag.id);drag=null;};
canvas.onwheel=e=>{e.preventDefault();const z=Math.max(.08,Math.min(5,view.z*Math.exp(-e.deltaY*.001)));view.x=e.clientX-(e.clientX-view.x)*z/view.z;view.y=e.clientY-(e.clientY-view.y)*z/view.z;view.z=z;transform();};
canvas.ondblclick=e=>{const id=e.target.closest('[data-id]')?.dataset.id,o=objects.get(id);if(!o)return;editorId=id;$('guide').hidden=false;$('editor').hidden=false;$('title').value=o.title||'';$('text').value=o.text||'';$('selection').textContent=id;$('title').focus();};
$('editor').onsubmit=e=>{e.preventDefault();send({op:'set',select:editorId,props:{title:$('title').value,text:$('text').value}}).catch(()=>{});$('editor').hidden=true;};
$('fit').onclick=fit;$('undo').onclick=()=>send({op:'undo'}).catch(()=>{});$('redo').onclick=()=>send({op:'redo'}).catch(()=>{});$('physics').onclick=()=>send({op:'physics',props:{enabled:!physics.enabled}}).catch(()=>{});$('help').onclick=()=>$('guide').hidden=!$('guide').hidden;$('close-guide').onclick=()=>$('guide').hidden=true;
$('send').onclick=async()=>{try{$('result').textContent=JSON.stringify(await send(JSON.parse($('command').value)));}catch(e){$('result').textContent=e.message;}};
addEventListener('keydown',e=>{if(['INPUT','TEXTAREA'].includes(document.activeElement.tagName))return;if(e.key==='Delete'&&selected.size){send({op:'remove',select:[...selected],duration:180,stagger:20}).catch(()=>{});selected.clear();}if(e.code==='Space'){e.preventDefault();$('physics').click();}if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='z'){e.preventDefault();send({op:e.shiftKey?'redo':'undo'}).catch(()=>{});}});
// Read-only diagnostics for local browser verification.
window.boardDiagnostics={state:()=>({seq,queue:queue.length,running,objects:Object.fromEntries(objects),physics}),client};
