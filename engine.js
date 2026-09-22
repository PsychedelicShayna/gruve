// Browser-independent geometry and approximate, intentionally gentle 2D physics.
export function bounds(o, objects) {
  if(o.type==='group') {
    const boxes=(o.members||[]).map(i=>objects.get(i)).filter(n=>n&&!n.from).map(n=>bounds(n,objects));
    if(boxes.length){const x=Math.min(...boxes.map(b=>b.x)),y=Math.min(...boxes.map(b=>b.y));return {x,y,w:Math.max(...boxes.map(b=>b.x+b.w))-x,h:Math.max(...boxes.map(b=>b.y+b.h))-y};}
  }
  const x=o.x||0,y=o.y||0;
  if(o.from&&objects.has(o.from)&&objects.has(o.to)){
    const a=bounds(objects.get(o.from),objects),b=bounds(objects.get(o.to),objects),ax=a.x+a.w/2,ay=a.y+a.h/2,bx=b.x+b.w/2,by=b.y+b.h/2;
    return {x:Math.min(ax,bx),y:Math.min(ay,by),w:Math.abs(bx-ax)||1,h:Math.abs(by-ay)||1};
  }
  if(['line','arrow','path'].includes(o.type)&&!o.points)return {x,y,w:120,h:1};
  if(o.points?.length){const xs=o.points.map(p=>p[0]),ys=o.points.map(p=>p[1]);return {x:x+Math.min(...xs),y:y+Math.min(...ys),w:Math.max(...xs)-Math.min(...xs)||1,h:Math.max(...ys)-Math.min(...ys)||1};}
  const size=o.radius?o.radius*2:16;
  return {x,y,w:o.width||(o.type==='dot'?size:260),h:o.height||(o.type==='dot'?size:140)};
}
export function translate(id,dx,dy,objects,seen=new Set()) {
  if(seen.has(id))return;seen.add(id);const o=objects.get(id);if(!o)return;
  o.x=(o.x||0)+dx;o.y=(o.y||0)+dy;
  for(const m of o.members||[])translate(m,dx,dy,objects,seen);
}
export function rootFor(id,objects){
  let next=id;
  for(let depth=0;depth<objects.size;depth++){
    const parent=[...objects.values()].find(o=>o.members?.includes(next));
    if(!parent)return next;next=parent.id;
  }
  return next;
}
export function stepPhysics(objects,settings,dt,locked=new Set()) {
  if(!settings.enabled)return false;
  const children=new Set([...objects.values()].flatMap(o=>o.members||[]));
  const bodies=[...objects.values()].filter(o=>o.body&&!children.has(o.id)&&!o.from);
  if(!bodies.length)return false;
  dt=Math.min(dt,.025);
  const fixed=o=>o.pinned||locked.has(o.id),forces=new Map(bodies.map(o=>[o.id,{x:0,y:0}]));
  const center=o=>{const b=bounds(o,objects);return {x:b.x+b.w/2,y:b.y+b.h/2}};
  for(let i=0;i<bodies.length;i++){
    const a=bodies[i],pa=center(a),fa=forces.get(a.id);
    fa.x-=pa.x*settings.center;fa.y-=pa.y*settings.center;
    for(let j=i+1;j<bodies.length;j++){
      const b=bodies[j],pb=center(b),fb=forces.get(b.id);let dx=pb.x-pa.x,dy=pb.y-pa.y;
      if(Math.abs(dx)+Math.abs(dy)<.001){dx=.1;dy=.07;}
      const d2=Math.max(400,dx*dx+dy*dy),d=Math.sqrt(d2),f=Math.min(1000,settings.repulsion*100/d2);
      fa.x-=f*dx/d;fa.y-=f*dy/d;fb.x+=f*dx/d;fb.y+=f*dy/d;
    }
  }
  for(const link of objects.values()){
    if(!link.from||!link.to||!link.strength)continue;
    const a=objects.get(rootFor(link.from,objects)),b=objects.get(rootFor(link.to,objects));
    if(!a||!b||a===b)continue;const pa=center(a),pb=center(b),dx=pb.x-pa.x,dy=pb.y-pa.y,d=Math.hypot(dx,dy)||1,f=(d-(link.rest||180))*link.strength;
    if(forces.has(a.id)){forces.get(a.id).x+=f*dx/d;forces.get(a.id).y+=f*dy/d;}
    if(forces.has(b.id)){forces.get(b.id).x-=f*dx/d;forces.get(b.id).y-=f*dy/d;}
  }
  let moved=false;
  for(const o of bodies){
    if(fixed(o)){if(o.pinned){o.vx=0;o.vy=0;}continue;}
    const f=forces.get(o.id),damping=Math.pow(settings.damping,dt*60),mass=o.mass||1;
    o.vx=Math.max(-3000,Math.min(3000,((o.vx||0)+f.x/mass*dt)*damping));
    o.vy=Math.max(-3000,Math.min(3000,((o.vy||0)+f.y/mass*dt)*damping));
    if(Math.abs(o.vx)+Math.abs(o.vy)>.025){translate(o.id,o.vx*dt,o.vy*dt,objects);moved=true;}
  }
  if(settings.collision)for(let i=0;i<bodies.length;i++)for(let j=i+1;j<bodies.length;j++){
    const a=bodies[i],b=bodies[j],aa=bounds(a,objects),bb=bounds(b,objects);
    const ox=Math.min(aa.x+aa.w,bb.x+bb.w)-Math.max(aa.x,bb.x),oy=Math.min(aa.y+aa.h,bb.y+bb.h)-Math.max(aa.y,bb.y);
    if(ox<=0||oy<=0)continue;
    const ia=fixed(a)?0:1/(a.mass||1),ib=fixed(b)?0:1/(b.mass||1),sum=ia+ib;if(!sum)continue;
    const horizontal=ox<oy,nx=horizontal?(aa.x+aa.w/2<bb.x+bb.w/2?1:-1):0,ny=horizontal?0:(aa.y+aa.h/2<bb.y+bb.h/2?1:-1),overlap=(horizontal?ox:oy)+.1;
    translate(a.id,-nx*overlap*ia/sum,-ny*overlap*ia/sum,objects);translate(b.id,nx*overlap*ib/sum,ny*overlap*ib/sum,objects);
    const relative=((b.vx||0)-(a.vx||0))*nx+((b.vy||0)-(a.vy||0))*ny;
    if(relative<0){const impulse=-(1+settings.bounce)*relative/sum;a.vx=(a.vx||0)-impulse*ia*nx;a.vy=(a.vy||0)-impulse*ia*ny;b.vx=(b.vx||0)+impulse*ib*nx;b.vy=(b.vy||0)+impulse*ib*ny;}
    moved=true;
  }
  return moved;
}
export function interpolate(a,b,t){
  if(typeof a==='number'&&typeof b==='number')return a+(b-a)*t;
  if(typeof a==='string'&&typeof b==='string'&&/^#[0-9a-f]{6}$/i.test(a)&&/^#[0-9a-f]{6}$/i.test(b)){
    return '#'+[1,3,5].map(i=>Math.round(parseInt(a.slice(i,i+2),16)*(1-t)+parseInt(b.slice(i,i+2),16)*t).toString(16).padStart(2,'0')).join('');
  }
  return t>=1?b:a;
}
