import test from 'node:test';import assert from 'node:assert/strict';
import {project,unproject,basis,faceNormal,shade,mix} from './camera.js';

const cam={tx:0,ty:0,tz:0,yaw:0,pitch:0,dist:1000,zoom:1,cx:500,cy:400};
const near=(a,b,eps=1e-6)=>assert.ok(Math.abs(a-b)<eps,`${a} != ${b}`);

test('at rest the z=0 plane projects 1:1 around the screen centre, so 2D and 3D agree',()=>{
  const p=project({x:100,y:50,z:0},cam);
  assert.deepEqual([p.x,p.y,p.scale],[600,450,1]);
  const q=project({x:100,y:50,z:0},{...cam,zoom:2});
  assert.deepEqual([q.x,q.y,q.scale],[700,500,2]);
});
test('points toward the viewer grow, points away shrink and sort behind',()=>{
  const nearP=project({x:0,y:0,z:500},cam),farP=project({x:0,y:0,z:-500},cam);
  near(nearP.scale,2);near(farP.scale,2/3);
  assert.ok(farP.depth>nearP.depth);
});
test('yaw swings depth into the horizontal axis; pitch into the vertical',()=>{
  const p=project({x:0,y:0,z:-100},{...cam,yaw:90});
  near(p.x,600);near(p.y,400);
  const q=project({x:0,y:0,z:-100},{...cam,pitch:90});
  near(q.x,500);near(q.y,300);
});
test('unproject inverts project on the chosen plane at any orientation',()=>{
  for(const c of [cam,{...cam,yaw:30,pitch:-25,zoom:1.5,tx:40,ty:-20}]){
    const w={x:120,y:-80,z:40},s=project(w,c),back=unproject(s.x,s.y,c,{x:0,y:0,z:40});
    near(back.x,w.x,1e-6);near(back.y,w.y,1e-6);near(back.z,w.z,1e-6);
  }
});
test('camera basis stays orthonormal',()=>{
  const b=basis(37,-52);
  for(const v of [b.right,b.up,b.forward])near(Math.hypot(v.x,v.y,v.z),1,1e-9);
  near(b.right.x*b.up.x+b.right.y*b.up.y+b.right.z*b.up.z,0,1e-9);
  near(b.right.x*b.forward.x+b.right.y*b.forward.y+b.right.z*b.forward.z,0,1e-9);
});
test('faces facing the light are brighter than faces edge-on',()=>{
  const facing=shade({x:0,y:0,z:1},cam),edge=shade({x:1,y:0,z:0},cam);
  assert.ok(facing>edge);assert.ok(edge>=.45&&facing<=1);
  assert.equal(mix('#ff0000',.5),'#800000');assert.equal(mix('none',.5),'none');
  assert.deepEqual(faceNormal([{x:0,y:0,z:0},{x:1,y:0,z:0},{x:0,y:1,z:0}]),{x:0,y:0,z:1});
});
