// Orbit camera for the 3D view. World axes: x right, y down, z toward the viewer.
// With yaw = pitch = 0 the projection of the z = 0 plane is the identity (times zoom), so 2D and 3D agree.

const rad=d=>d*Math.PI/180;

// Orthonormal basis for a camera orbiting the target: right, up, forward (forward points from the camera into the scene).
export function basis(yawDeg,pitchDeg){
  const y=rad(yawDeg),p=rad(pitchDeg);
  const cy=Math.cos(y),sy=Math.sin(y),cp=Math.cos(p),sp=Math.sin(p);
  // Start from forward = (0,0,-1), right = (1,0,0), up = (0,1,0); yaw rotates about the world y axis, pitch about the camera's right axis.
  const right={x:cy,y:0,z:-sy};
  const forward0={x:-sy,y:0,z:-cy};
  const up0={x:0,y:1,z:0};
  // pitch: rotate forward0 and up0 about `right`
  const forward={x:forward0.x*cp+up0.x*sp,y:forward0.y*cp+up0.y*sp,z:forward0.z*cp+up0.z*sp};
  const up={x:up0.x*cp-forward0.x*sp,y:up0.y*cp-forward0.y*sp,z:up0.z*cp-forward0.z*sp};
  return {right,up,forward};
}
const dot=(a,b)=>a.x*b.x+a.y*b.y+a.z*b.z;

// cam: {tx,ty,tz, yaw, pitch, dist, zoom, cx, cy} — target, angles, camera distance, zoom and screen centre.
export function project(p,cam){
  const b=basis(cam.yaw,cam.pitch);
  const d={x:(p.x||0)-cam.tx,y:(p.y||0)-cam.ty,z:(p.z||0)-cam.tz};
  const x=dot(d,b.right),y=dot(d,b.up),z=dot(d,b.forward);
  const depth=cam.dist+z;                       // distance from the camera along forward
  const scale=cam.dist/Math.max(1,depth);       // perspective: 1 at the target plane
  return {x:cam.cx+x*scale*cam.zoom,y:cam.cy+y*scale*cam.zoom,scale:scale*cam.zoom,depth};
}
// World point on the plane through `anchor` with normal `normal` under screen point (sx,sy).
export function unproject(sx,sy,cam,anchor,normal={x:0,y:0,z:1}){
  const b=basis(cam.yaw,cam.pitch);
  // camera position and the ray through the screen point (in world coordinates)
  const eye={x:cam.tx-b.forward.x*cam.dist,y:cam.ty-b.forward.y*cam.dist,z:cam.tz-b.forward.z*cam.dist};
  const px=(sx-cam.cx)/cam.zoom,py=(sy-cam.cy)/cam.zoom; // camera-plane offsets at the target distance
  const through={x:cam.tx+b.right.x*px+b.up.x*py,y:cam.ty+b.right.y*px+b.up.y*py,z:cam.tz+b.right.z*px+b.up.z*py};
  const dir={x:through.x-eye.x,y:through.y-eye.y,z:through.z-eye.z};
  const denom=dot(dir,normal);
  if(Math.abs(denom)<1e-9)return null;
  const t=dot({x:anchor.x-eye.x,y:anchor.y-eye.y,z:anchor.z-eye.z},normal)/denom;
  if(t<=0)return null;
  return {x:eye.x+dir.x*t,y:eye.y+dir.y*t,z:eye.z+dir.z*t};
}
// Unit normal of a polygon from its first three non-collinear points (world space).
export function faceNormal(points){
  for(let i=2;i<points.length;i++){
    const a=points[0],b=points[1],c=points[i];
    const u={x:b.x-a.x,y:b.y-a.y,z:(b.z||0)-(a.z||0)},v={x:c.x-a.x,y:c.y-a.y,z:(c.z||0)-(a.z||0)};
    const n={x:u.y*v.z-u.z*v.y,y:u.z*v.x-u.x*v.z,z:u.x*v.y-u.y*v.x};
    const l=Math.hypot(n.x,n.y,n.z);
    if(l>1e-9)return {x:n.x/l,y:n.y/l,z:n.z/l};
  }
  return {x:0,y:0,z:1};
}
// Brightness 0.45..1 for a face given its normal and the camera; light comes from over the viewer's left shoulder.
export function shade(normal,cam){
  const b=basis(cam.yaw,cam.pitch);
  const light={x:-0.35*b.right.x-0.5*b.up.x-0.8*b.forward.x,y:-0.35*b.right.y-0.5*b.up.y-0.8*b.forward.y,z:-0.35*b.right.z-0.5*b.up.z-0.8*b.forward.z};
  const l=Math.hypot(light.x,light.y,light.z);
  const k=Math.abs(dot(normal,light))/l;   // two-sided
  return 0.45+0.55*k;
}
export function mix(hex,brightness){
  if(!/^#[0-9a-f]{6}$/i.test(hex))return hex;
  return '#'+[1,3,5].map(i=>Math.round(Math.min(255,parseInt(hex.slice(i,i+2),16)*brightness)).toString(16).padStart(2,'0')).join('');
}
