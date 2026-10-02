// Browser-independent layout, geometry and gentle 2D physics for the v1.2 board.
// `objects` is a Map id → object. Groups own `children` in local coordinates.

const DEFAULT_TEXT_W=260;
const isFill=v=>v==='fill';
const num=(v,d=0)=>typeof v==='number'?v:d;

// ---------------------------------------------------------------- layout
// resolve(objects, measure) → {boxes: Map id → {x,y,w,h} (world, logical), locals: Map id → {x,y,w,h} (parent frame),
//   measured: Map id → {...}}. `measure(o, width)` returns {w,h} content size for a text object.
export function resolve(objects,measure){
  const boxes=new Map(),locals=new Map(),measured=new Map();
  const cache=new Map();
  // Local size of a node in its own frame: {bx,by,bw,bh} bounds relative to the node origin, plus child placements.
  function sizeOf(o,avail){
    const key=o.id+'|'+(avail.w??'')+'|'+(avail.h??'');
    if(cache.has(key))return cache.get(key);
    const r=compute(o,avail);cache.set(key,r);return r;
  }
  function compute(o,avail){
    const t=o.type;
    if(t==='rect'||t==='ellipse'){
      let w=o.w,h=o.h;
      if(t==='ellipse'&&o.r!==undefined){w=h=o.r*2;}
      w=isFill(w)?(avail.w??0):num(w,0);h=isFill(h)?(avail.h??0):num(h,0);
      return {bx:0,by:0,bw:w,bh:h};
    }
    if(t==='polygon'||t==='polyline'){
      const pts=o.points||[];const xs=pts.map(p=>p[0]),ys=pts.map(p=>p[1]);
      const x=Math.min(...xs),y=Math.min(...ys);
      return {bx:x,by:y,bw:Math.max(...xs)-x,bh:Math.max(...ys)-y};
    }
    if(t==='text'){
      const w=isFill(o.w)?(avail.w??DEFAULT_TEXT_W):num(o.w,DEFAULT_TEXT_W);
      const content=measure?measure(o,w):{w,h:num(o.size,14)*1.5};
      let h;
      if(isFill(o.h))h=avail.h??content.h;else if(typeof o.h==='number')h=o.h;else h=content.h;
      measured.set(o.id,{w,h,contentW:content.w,contentH:content.h,overflow:content.h>h+1||content.w>w+1});
      return {bx:0,by:0,bw:w,bh:h};
    }
    if(t==='group')return computeGroup(o,avail);
    return {bx:0,by:0,bw:0,bh:0};
  }
  function computeGroup(g,avail){
    const p=num(g.padding,0),lay=g.layout||null,gap=num(lay?.gap,0);
    const kids=(g.children||[]).map(i=>objects.get(i)).filter(Boolean);
    const overlay=k=>isFill(k.w)&&isFill(k.h);
    const content=kids.filter(k=>!overlay(k));
    // Width first. A fill group with nothing to fill (no available width) hugs instead.
    let w;
    const hugW=()=>{
      const contributors=content.filter(k=>!isFill(k.w));
      const sizes=contributors.map(k=>sizeOf(k,{w:null,h:null}));
      if(lay?.type==='row')return sizes.reduce((s,b)=>s+b.bw,0)+gap*Math.max(0,sizes.length-1)+2*p;
      if(lay?.type==='grid'){
        const cols=Math.max(1,lay.cols|0);const colW=[];
        content.forEach((k,i)=>{const c=i%cols;if(colW[c]===undefined)colW[c]=0;if(!isFill(k.w))colW[c]=Math.max(colW[c],sizeOf(k,{w:null,h:null}).bw);});
        return colW.reduce((s,v)=>s+v,0)+gap*Math.max(0,colW.length-1)+2*p;
      }
      if(lay?.type==='stack')return Math.max(0,...sizes.map(b=>b.bw))+2*p;
      let minX=Infinity,maxX=-Infinity;
      contributors.forEach((k,i)=>{minX=Math.min(minX,num(k.x)+sizes[i].bx);maxX=Math.max(maxX,num(k.x)+sizes[i].bx+sizes[i].bw);});
      return contributors.length?maxX-minX+2*p:2*p;
    };
    if(typeof g.w==='number')w=g.w;
    else if(isFill(g.w)&&avail.w!==null&&avail.w!==undefined)w=avail.w;
    else w=hugW();
    const innerW=Math.max(0,w-2*p);
    // Children sized with the known inner width (text wraps here). Natural heights size the tracks.
    const sized=content.map(k=>({k,b:sizeOf(k,{w:innerW,h:null})}));
    let h,rowsH=null,colW=null;
    const hugKids=sized.filter(s=>!isFill(s.k.h));
    if(lay?.type==='grid'){
      const cols=Math.max(1,lay.cols|0);colW=[];rowsH=[];
      sized.forEach((s,i)=>{const c=i%cols,r=(i/cols)|0;if(colW[c]===undefined)colW[c]=0;if(rowsH[r]===undefined)rowsH[r]=0;if(!isFill(s.k.w))colW[c]=Math.max(colW[c],s.b.bw);if(!isFill(s.k.h))rowsH[r]=Math.max(rowsH[r],s.b.bh);else if(s.b.bh)rowsH[r]=Math.max(rowsH[r],s.b.bh);});
    }
    const hugH=()=>{
      if(lay?.type==='stack')return sized.reduce((s,x)=>s+x.b.bh,0)+gap*Math.max(0,sized.length-1)+2*p;
      if(lay?.type==='row')return Math.max(0,...sized.map(x=>x.b.bh))+2*p;
      if(lay?.type==='grid')return rowsH.reduce((s,v)=>s+v,0)+gap*Math.max(0,rowsH.length-1)+2*p;
      let minY=Infinity,maxY=-Infinity;
      hugKids.forEach(s=>{minY=Math.min(minY,num(s.k.y)+s.b.by);maxY=Math.max(maxY,num(s.k.y)+s.b.by+s.b.bh);});
      return hugKids.length?maxY-minY+2*p:2*p;
    };
    if(typeof g.h==='number')h=g.h;
    else if(isFill(g.h)&&avail.h!==null&&avail.h!==undefined)h=avail.h;
    else h=hugH();
    const innerH=Math.max(0,h-2*p);
    // Bounds in the group's frame. Layouts start at the origin; free groups hug their content, which may start negative.
    let bx=0,by=0;
    if(!lay&&typeof g.w!=='number'&&!isFill(g.w)){
      const contributors=sized.filter(s=>!isFill(s.k.w));
      if(contributors.length)bx=Math.min(...contributors.map(s=>num(s.k.x)+s.b.bx))-p;
    }
    if(!lay&&typeof g.h!=='number'&&!isFill(g.h)){
      if(hugKids.length)by=Math.min(...hugKids.map(s=>num(s.k.y)+s.b.by))-p;
    }
    // Placement of content children (positions relative to the group origin).
    const placements=[];let cursor=p,contentMinX=Infinity,contentMinY=Infinity,contentMaxX=-Infinity,contentMaxY=-Infinity;
    const align=lay?.align||'start';
    const cols=Math.max(1,(lay?.cols|0)||1);
    sized.forEach((s,i)=>{
      const b=s.b,c=i%cols,r=(i/cols)|0;
      // Fill on one axis: the track in a grid, the inner size in a free group, natural size along a stack/row's flow axis.
      let kw=b.bw,kh=b.bh;
      if(isFill(s.k.w))kw=lay?.type==='grid'?colW[c]:lay?.type==='row'?b.bw:innerW;
      if(isFill(s.k.h))kh=lay?.type==='grid'?rowsH[r]:lay?.type==='stack'?b.bh:innerH;
      let x,y;
      if(lay?.type==='stack'){
        x=p+(align==='center'?(innerW-kw)/2:align==='end'?innerW-kw:0)-b.bx;y=cursor-b.by;cursor+=kh+gap;
      }else if(lay?.type==='row'){
        x=cursor-b.bx;y=p+(align==='center'?(innerH-kh)/2:align==='end'?innerH-kh:0)-b.by;cursor+=kw+gap;
      }else if(lay?.type==='grid'){
        x=p+colW.slice(0,c).reduce((a,v)=>a+v,0)+gap*c+(align==='center'?(colW[c]-kw)/2:align==='end'?colW[c]-kw:0)-b.bx;
        y=p+rowsH.slice(0,r).reduce((a,v)=>a+v,0)+gap*r+(align==='center'?(rowsH[r]-kh)/2:align==='end'?rowsH[r]-kh:0)-b.by;
      }else{
        x=isFill(s.k.w)?bx+p-b.bx:num(s.k.x);y=isFill(s.k.h)?by+p-b.by:num(s.k.y);
      }
      placements.push({id:s.k.id,x,y,w:kw,h:kh,b});
      contentMinX=Math.min(contentMinX,x+b.bx);contentMinY=Math.min(contentMinY,y+b.by);
      contentMaxX=Math.max(contentMaxX,x+b.bx+kw);contentMaxY=Math.max(contentMaxY,y+b.by+kh);
    });
    for(const k of kids.filter(overlay)){
      placements.push({id:k.id,x:bx,y:by,w,h,b:{bx:0,by:0,bw:w,bh:h}});
    }
    if(typeof g.w==='number'||typeof g.h==='number'){
      // Content may spill past any edge: free children can sit at negative x/y.
      const over=contentMinX<bx-1||contentMinY<by-1||contentMaxX>bx+w+1||contentMaxY>by+h+1;
      const left=Math.min(bx,contentMinX),top=Math.min(by,contentMinY);
      measured.set(g.id,{w,h,contentW:Math.max(0,contentMaxX-left),contentH:Math.max(0,contentMaxY-top),overflow:over});
    }
    return {bx,by,bw:w,bh:h,placements};
  }
  function place(o,originX,originY,local){
    // local: {x,y,w,h} in the parent frame; origin: world position of o's origin
    const s=local.s;
    boxes.set(o.id,{x:originX+s.bx,y:originY+s.by,w:s.bw,h:s.bh,ox:originX,oy:originY});
    locals.set(o.id,{x:local.x,y:local.y,w:s.bw,h:s.bh,bx:s.bx,by:s.by});
    if(o.type==='group'){
      for(const pl of s.placements||[]){
        const child=objects.get(pl.id);if(!child)continue;
        // Re-derive the child's own size in the final available box so fill children get their real dimensions.
        const cs=(isFill(child.w)||isFill(child.h))?sizeOf(child,{w:pl.w,h:pl.h}):pl.b;
        place(child,originX+pl.x,originY+pl.y,{x:pl.x,y:pl.y,s:cs});
      }
    }
  }
  for(const o of objects.values()){
    if(o.type==='edge'||o.parent!==undefined&&o.parent!==null)continue;
    const s=sizeOf(o,{w:null,h:null});
    place(o,num(o.x),num(o.y),{x:num(o.x),y:num(o.y),s});
  }
  return {boxes,locals,measured};
}

// ---------------------------------------------------------------- geometry
export function centreOf(box){return {x:box.x+box.w/2,y:box.y+box.h/2};}
const unit=(p,q)=>{const dx=q.x-p.x,dy=q.y-p.y,d=Math.hypot(dx,dy)||1;return {x:dx/d,y:dy/d};};

// World-space outline points for polygons/polylines.
export function worldPoints(o,boxes){
  const b=boxes.get(o.id);if(!b)return [];
  if(b.pts)return b.pts;   // pre-projected (3D view)
  return (o.points||[]).map(p=>({x:b.ox+p[0],y:b.oy+p[1]}));
}
// Point on the outline of `o` along the ray from its box centre towards `target`.
export function edgePoint(o,boxes,target){
  const b=boxes.get(o.id);if(!b)return target;
  const c=centreOf(b),dx=target.x-c.x,dy=target.y-c.y;
  if(Math.abs(dx)<1e-6&&Math.abs(dy)<1e-6)return c;
  if(o.type==='ellipse'){
    const rx=b.w/2||1,ry=b.h/2||1,t=1/Math.sqrt((dx*dx)/(rx*rx)+(dy*dy)/(ry*ry));
    return {x:c.x+dx*t,y:c.y+dy*t};
  }
  if(o.type==='polygon'){
    const pts=worldPoints(o,boxes);let best=null;
    for(let i=0;i<pts.length;i++){
      const a=pts[i],q=pts[(i+1)%pts.length];
      const t=raySegment(c,{x:dx,y:dy},a,q);
      if(t!==null&&(best===null||t>best))best=t;
    }
    if(best!==null)return {x:c.x+dx*best,y:c.y+dy*best};
  }
  const tx=dx?Math.abs((b.w/2)/dx):Infinity,ty=dy?Math.abs((b.h/2)/dy):Infinity,t=Math.min(tx,ty);
  return {x:c.x+dx*t,y:c.y+dy*t};
}
function raySegment(o,d,a,b){
  const ex=b.x-a.x,ey=b.y-a.y,den=d.x*ey-d.y*ex;
  if(Math.abs(den)<1e-9)return null;
  const t=((a.x-o.x)*ey-(a.y-o.y)*ex)/den,u=((a.x-o.x)*d.y-(a.y-o.y)*d.x)/den;
  return t>=0&&u>=0&&u<=1?t:null;
}
function anchorTarget(a){return typeof a==='string'?a:Array.isArray(a)?null:a?.id??null;}
// Resolve an anchor to {point, object|null, box|null, free:boolean}.
function anchorInfo(a,objects,boxes){
  if(Array.isArray(a))return {point:{x:a[0],y:a[1]},object:null,box:null,fixed:true};
  const id=anchorTarget(a),o=objects.get(id),box=boxes.get(id);
  if(!o||!box)return null;
  if(typeof a==='string')return {object:o,box,point:null,fixed:false};
  if(a.vertex!==undefined){const p=worldPoints(o,boxes)[a.vertex];return {object:o,box,point:p||centreOf(box),fixed:true};}
  if(a.at)return {object:o,box,point:{x:box.x+box.w*a.at[0],y:box.y+box.h*a.at[1]},fixed:true};
  if(a.side){
    const f=a.offset??.5;
    const point=a.side==='left'?{x:box.x,y:box.y+box.h*f}:a.side==='right'?{x:box.x+box.w,y:box.y+box.h*f}:a.side==='top'?{x:box.x+box.w*f,y:box.y}:{x:box.x+box.w*f,y:box.y+box.h};
    return {object:o,box,point,fixed:true,side:a.side};
  }
  return {object:o,box,point:null,fixed:false};
}
// Route for an edge → {points, control?, start, end, tangentStart, tangentEnd} or null.
export function route(edge,objects,boxes){
  const A=anchorInfo(edge.from,objects,boxes),B=anchorInfo(edge.to,objects,boxes);
  if(!A||!B)return null;
  const refA=A.point||centreOf(A.box),refB=B.point||centreOf(B.box);
  const mode=edge.route||'straight';
  if(mode==='elbow'){
    const dx=refB.x-refA.x,dy=refB.y-refA.y;
    const horizontal=A.side?(A.side==='left'||A.side==='right'):B.side?(B.side==='left'||B.side==='right'):Math.abs(dx)>=Math.abs(dy);
    let pts;
    if(horizontal){
      const sx=A.point?A.point.x:(dx>=0?A.box.x+A.box.w:A.box.x),sy=A.point?A.point.y:refA.y;
      const ex=B.point?B.point.x:(dx>=0?B.box.x:B.box.x+B.box.w),ey=B.point?B.point.y:refB.y;
      const mx=(sx+ex)/2;pts=[{x:sx,y:sy},{x:mx,y:sy},{x:mx,y:ey},{x:ex,y:ey}];
    }else{
      const sy=A.point?A.point.y:(dy>=0?A.box.y+A.box.h:A.box.y),sx=A.point?A.point.x:refA.x;
      const ey=B.point?B.point.y:(dy>=0?B.box.y:B.box.y+B.box.h),ex=B.point?B.point.x:refB.x;
      const my=(sy+ey)/2;pts=[{x:sx,y:sy},{x:sx,y:my},{x:ex,y:my},{x:ex,y:ey}];
    }
    return {points:pts,start:pts[0],end:pts[3],tangentStart:unit(pts[0],pts[1]),tangentEnd:unit(pts[2],pts[3])};
  }
  if(mode==='curve'){
    const d=Math.hypot(refB.x-refA.x,refB.y-refA.y)||1,bend=edge.curve??Math.min(80,d*.25);
    const nx=-(refB.y-refA.y)/d,ny=(refB.x-refA.x)/d;
    const control={x:(refA.x+refB.x)/2+nx*bend,y:(refA.y+refB.y)/2+ny*bend};
    const start=A.point||edgePoint(A.object,boxes,control),end=B.point||edgePoint(B.object,boxes,control);
    return {points:[start,end],control,start,end,tangentStart:unit(start,control),tangentEnd:unit(control,end)};
  }
  const start=A.point||edgePoint(A.object,boxes,refB),end=B.point||edgePoint(B.object,boxes,start);
  return {points:[start,end],start,end,tangentStart:unit(start,end),tangentEnd:unit(start,end)};
}
// Geometry for one head whose tip is at `tip`, pointing along unit `dir`.
export function headGeometry(kind,tip,dir,size){
  const nx=-dir.y,ny=dir.x,back={x:tip.x-dir.x*size,y:tip.y-dir.y*size},pt=(p)=>`${p.x.toFixed(2)},${p.y.toFixed(2)}`;
  const wing=(k)=>({x:back.x+nx*size*k,y:back.y+ny*size*k});
  switch(kind){
    case 'arrow':return {trim:size*.85,shapes:[{tag:'polygon',filled:true,attrs:{points:[tip,wing(.42),wing(-.42)].map(pt).join(' ')}}]};
    case 'open':return {trim:0,shapes:[{tag:'polyline',filled:false,attrs:{points:[wing(.45),tip,wing(-.45)].map(pt).join(' ')}}]};
    case 'dot':{const r=size*.42,c={x:tip.x-dir.x*r,y:tip.y-dir.y*r};return {trim:r*2,shapes:[{tag:'circle',filled:true,attrs:{cx:c.x,cy:c.y,r}}]};}
    case 'diamond':{const mid={x:tip.x-dir.x*size*.55,y:tip.y-dir.y*size*.55},far={x:tip.x-dir.x*size*1.1,y:tip.y-dir.y*size*1.1};
      return {trim:size*1.05,shapes:[{tag:'polygon',filled:true,attrs:{points:[tip,{x:mid.x+nx*size*.4,y:mid.y+ny*size*.4},far,{x:mid.x-nx*size*.4,y:mid.y-ny*size*.4}].map(pt).join(' ')}}]};}
    case 'bar':return {trim:0,shapes:[{tag:'line',filled:false,attrs:{x1:tip.x+nx*size*.5,y1:tip.y+ny*size*.5,x2:tip.x-nx*size*.5,y2:tip.y-ny*size*.5}}]};
    default:return {trim:0,shapes:[]};
  }
}
export function edgeEndpoints(edge){return [anchorTarget(edge.from),anchorTarget(edge.to)].filter(Boolean);}
export function rootFor(id,objects){
  let next=id;
  for(let depth=0;depth<objects.size;depth++){
    const o=objects.get(next);if(!o||o.parent===undefined||o.parent===null)return next;next=o.parent;
  }
  return next;
}
export function interpolate(a,b,t){
  if(typeof a==='number'&&typeof b==='number')return a+(b-a)*t;
  if(typeof a==='string'&&typeof b==='string'&&/^#[0-9a-f]{6}$/i.test(a)&&/^#[0-9a-f]{6}$/i.test(b)){
    return '#'+[1,3,5].map(i=>Math.round(parseInt(a.slice(i,i+2),16)*(1-t)+parseInt(b.slice(i,i+2),16)*t).toString(16).padStart(2,'0')).join('');
  }
  return t>=1?b:a;
}

// ---------------------------------------------------------------- physics (roots only)
export function translate(id,dx,dy,objects,boxes){
  const o=objects.get(id);if(!o)return;
  o.x=(o.x||0)+dx;o.y=(o.y||0)+dy;
  if(boxes){const shift=(i)=>{const b=boxes.get(i);if(b){b.x+=dx;b.y+=dy;b.ox+=dx;b.oy+=dy;}for(const c of objects.get(i)?.children||[])shift(c);};shift(id);}
}
export function stepPhysics(objects,settings,dt,locked=new Set(),boxes=new Map()){
  if(!settings.enabled)return false;
  const bodies=[...objects.values()].filter(o=>o.body&&(o.parent===undefined||o.parent===null)&&o.type!=='edge'&&boxes.has(o.id));
  if(!bodies.length)return false;
  dt=Math.min(dt,.025);
  const fixed=o=>o.pinned||locked.has(o.id),forces=new Map(bodies.map(o=>[o.id,{x:0,y:0}]));
  const center=o=>centreOf(boxes.get(o.id));
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
    if(link.type!=='edge'||!link.strength)continue;
    const [fa,fb]=edgeEndpoints(link);if(!fa||!fb)continue;
    const a=objects.get(rootFor(fa,objects)),b=objects.get(rootFor(fb,objects));
    if(!a||!b||a===b||!boxes.has(a.id)||!boxes.has(b.id))continue;
    const pa=center(a),pb=center(b),dx=pb.x-pa.x,dy=pb.y-pa.y,d=Math.hypot(dx,dy)||1,f=(d-(link.rest||180))*link.strength;
    if(forces.has(a.id)){forces.get(a.id).x+=f*dx/d;forces.get(a.id).y+=f*dy/d;}
    if(forces.has(b.id)){forces.get(b.id).x-=f*dx/d;forces.get(b.id).y-=f*dy/d;}
  }
  let moved=false;
  for(const o of bodies){
    if(fixed(o)){if(o.pinned){o.vx=0;o.vy=0;}continue;}
    const f=forces.get(o.id),damping=Math.pow(settings.damping,dt*60),mass=o.mass||1;
    o.vx=Math.max(-3000,Math.min(3000,((o.vx||0)+f.x/mass*dt)*damping));
    o.vy=Math.max(-3000,Math.min(3000,((o.vy||0)+f.y/mass*dt)*damping));
    if(Math.abs(o.vx)+Math.abs(o.vy)>.025){translate(o.id,o.vx*dt,o.vy*dt,objects,boxes);moved=true;}
  }
  if(settings.collision)for(let i=0;i<bodies.length;i++)for(let j=i+1;j<bodies.length;j++){
    const a=bodies[i],b=bodies[j],aa=boxes.get(a.id),bb=boxes.get(b.id);
    const ox=Math.min(aa.x+aa.w,bb.x+bb.w)-Math.max(aa.x,bb.x),oy=Math.min(aa.y+aa.h,bb.y+bb.h)-Math.max(aa.y,bb.y);
    if(ox<=0||oy<=0)continue;
    const ia=fixed(a)?0:1/(a.mass||1),ib=fixed(b)?0:1/(b.mass||1),sum=ia+ib;if(!sum)continue;
    const horizontal=ox<oy,nx=horizontal?(aa.x+aa.w/2<bb.x+bb.w/2?1:-1):0,ny=horizontal?0:(aa.y+aa.h/2<bb.y+bb.h/2?1:-1),overlap=(horizontal?ox:oy)+.1;
    translate(a.id,-nx*overlap*ia/sum,-ny*overlap*ia/sum,objects,boxes);translate(b.id,nx*overlap*ib/sum,ny*overlap*ib/sum,objects,boxes);
    const relative=((b.vx||0)-(a.vx||0))*nx+((b.vy||0)-(a.vy||0))*ny;
    if(relative<0){const impulse=-(1+settings.bounce)*relative/sum;a.vx=(a.vx||0)-impulse*ia*nx;a.vy=(a.vy||0)-impulse*ia*ny;b.vx=(b.vx||0)+impulse*ib*nx;b.vy=(b.vy||0)+impulse*ib*ny;}
    moved=true;
  }
  return moved;
}
