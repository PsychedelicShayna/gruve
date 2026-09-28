import test from 'node:test';import assert from 'node:assert/strict';
import {resolve,route,edgePoint,headGeometry,stepPhysics,translate,interpolate} from './engine.js';

// Deterministic stand-in for browser text measurement: 7px per character, wrapping at the given width.
const measure=(o,w)=>{const line=(o.size||14)*1.5,chars=Math.max(1,Math.floor(w/7));const lines=(o.text||'').split('\n').reduce((n,l)=>n+Math.max(1,Math.ceil(l.length/chars)),0);return {w:Math.min(w,(o.text||'').length*7),h:lines*line};};
const scene=(...objs)=>{const m=new Map();for(const o of objs)m.set(o.id,{x:0,y:0,...o});return m;};
const card=(id,text,extra={})=>[
  {id,type:'group',w:260,padding:15,layout:{type:'stack',gap:6},children:[id+'/bg',id+'/t'],...extra},
  {id:id+'/bg',type:'rect',parent:id,w:'fill',h:'fill'},
  {id:id+'/t',type:'text',parent:id,w:'fill',text},
];
const settings={enabled:true,repulsion:0,center:0,damping:1,collision:false,bounce:.5};

test('a card hugs its wrapped text and its background fills the card',()=>{
  const m=scene(...card('c','x'.repeat(100),{x:100,y:50}));
  const {boxes,measured}=resolve(m,measure);
  const c=boxes.get('c'),t=boxes.get('c/t'),bg=boxes.get('c/bg');
  assert.equal(t.w,230);                      // 260 - 2*15
  assert.equal(t.h,measure({text:'x'.repeat(100)},230).h);
  assert.equal(c.h,t.h+30);                   // padding on both sides
  assert.deepEqual([bg.x,bg.y,bg.w,bg.h],[c.x,c.y,c.w,c.h]);
  assert.equal(measured.get('c/t').overflow,false);
});
test('explicit text height reports overflow without changing the box',()=>{
  const m=scene({id:'t',type:'text',w:100,h:20,text:'x'.repeat(200)});
  const {boxes,measured}=resolve(m,measure);
  assert.equal(boxes.get('t').h,20);
  assert.equal(measured.get('t').overflow,true);
  assert.ok(measured.get('t').contentH>20);
});
test('explicitly sized group reports overflow when children spill out',()=>{
  const m=scene({id:'g',type:'group',w:50,h:50,children:['r']},{id:'r',type:'rect',parent:'g',w:200,h:10});
  const {measured}=resolve(m,measure);
  assert.equal(measured.get('g').overflow,true);
  assert.equal(measured.get('g').contentW,200);
});
test('fitting explicitly sized group reports no overflow',()=>{
  const m=scene({id:'g',type:'group',w:50,h:50,children:['r']},{id:'r',type:'rect',parent:'g',w:20,h:10});
  const {measured}=resolve(m,measure);
  assert.equal(measured.get('g')?.overflow,false);
});
test('explicitly sized group reports children spilling past its left and top edges',()=>{
  const m=scene({id:'g',type:'group',w:100,h:100,children:['r']},{id:'r',type:'rect',parent:'g',x:-50,y:-20,w:10,h:10});
  const {measured}=resolve(m,measure);
  assert.equal(measured.get('g')?.overflow,true);
  assert.equal(measured.get('g').contentW,10);
});
test('free group hugs children with negative coordinates and keeps its origin',()=>{
  const m=scene({id:'g',type:'group',x:500,y:500,padding:10,children:['p','bg']},{id:'p',type:'polygon',parent:'g',points:[[0,-50],[40,20],[-40,20]]},{id:'bg',type:'rect',parent:'g',w:'fill',h:'fill'});
  const {boxes}=resolve(m,measure);
  const g=boxes.get('g');
  assert.deepEqual([g.x,g.y,g.w,g.h],[450,440,100,90]);
  assert.deepEqual([g.ox,g.oy],[500,500]);
  assert.deepEqual([boxes.get('bg').x,boxes.get('bg').w],[450,100]);
});
test('grid tracks are content sized and fill cells stretch to their row',()=>{
  const cell=(id,text)=>[{id,type:'group',parent:'t',layout:{type:'stack'},padding:8,w:100,h:'fill',children:[id+'/x']},{id:id+'/x',type:'text',parent:id,w:'fill',text}];
  const m=scene({id:'t',type:'group',layout:{type:'grid',cols:2},children:['a','b','c','d']},...cell('a','a'),...cell('b','b'.repeat(60)),...cell('c','c'),...cell('d','d'));
  const {boxes}=resolve(m,measure);
  assert.equal(boxes.get('a').h,boxes.get('b').h);
  assert.ok(boxes.get('b').h>boxes.get('c').h);
  assert.equal(boxes.get('c').y,boxes.get('a').y+boxes.get('a').h);
  assert.equal(boxes.get('b').x,boxes.get('a').x+100);
});
test('a fill cell keeps its grid index and stays inside the group box',()=>{
  const m=scene({id:'g',type:'group',layout:{type:'grid',cols:2},children:['a','b','c']},{id:'a',type:'rect',parent:'g',w:'fill',h:10},{id:'b',type:'rect',parent:'g',w:40,h:10},{id:'c',type:'rect',parent:'g',w:30,h:10});
  const {boxes}=resolve(m,measure);
  const g=boxes.get('g');
  assert.equal(boxes.get('b').x,30);
  assert.equal(boxes.get('a').w,30);
  for(const id of ['a','b','c']){const k=boxes.get(id);assert.ok(k.x>=g.x&&k.y>=g.y&&k.x+k.w<=g.x+g.w+1e-6&&k.y+k.h<=g.y+g.h+1e-6,id);}
});
test('row layout places children left to right with gaps',()=>{
  const m=scene({id:'r',type:'group',layout:{type:'row',gap:10},padding:5,children:['a','b']},{id:'a',type:'rect',parent:'r',w:30,h:10},{id:'b',type:'rect',parent:'r',w:20,h:40});
  const {boxes}=resolve(m,measure);
  assert.deepEqual([boxes.get('r').w,boxes.get('r').h],[70,50]);
  assert.equal(boxes.get('b').x,45);
});
test('edges stop on the outline: rect side, ellipse rim, polygon edge',()=>{
  const m=scene({id:'a',type:'rect',w:100,h:50},{id:'b',type:'ellipse',x:300,y:15,r:10},{id:'p',type:'polygon',x:0,y:300,points:[[0,0],[100,0],[50,80]]});
  const {boxes}=resolve(m,measure);
  const r=route({from:'a',to:'b'},m,boxes);
  assert.equal(r.start.x,100);assert.equal(r.start.y,25);assert.equal(Math.round(r.end.x),300);
  const tip=edgePoint(m.get('p'),boxes,{x:50,y:0});
  assert.equal(Math.round(tip.y),300);          // top edge of the triangle, not its bounding box centre
});
test('side, at, vertex and fixed anchors resolve to explicit points',()=>{
  const m=scene({id:'a',type:'rect',w:100,h:50},{id:'p',type:'polygon',x:200,y:0,points:[[0,0],[10,0],[0,10]]});
  const {boxes}=resolve(m,measure);
  assert.deepEqual(route({from:{id:'a',side:'right',offset:.2},to:[500,10]},m,boxes).start,{x:100,y:10});
  assert.deepEqual(route({from:{id:'a',at:[.5,.5]},to:[500,25]},m,boxes).start,{x:50,y:25});
  assert.deepEqual(route({from:{id:'p',vertex:1},to:[500,0]},m,boxes).start,{x:210,y:0});
  assert.deepEqual(route({from:[1,2],to:[3,4]},m,boxes).end,{x:3,y:4});
});
test('elbow route is axis aligned with one bend',()=>{
  const m=scene({id:'a',type:'rect',w:100,h:100},{id:'b',type:'rect',x:400,y:300,w:100,h:100});
  const {boxes}=resolve(m,measure);
  const r=route({from:'a',to:'b',route:'elbow'},m,boxes);
  assert.equal(r.points.length,4);
  assert.equal(r.points[0].y,r.points[1].y);assert.equal(r.points[1].x,r.points[2].x);assert.equal(r.points[2].y,r.points[3].y);
  assert.equal(r.points[3].x,400);
});
test('filled heads trim the line so the stroke stays under the head',()=>{
  const tip={x:100,y:0},dir={x:1,y:0};
  assert.ok(headGeometry('arrow',tip,dir,10).trim>0);assert.ok(headGeometry('diamond',tip,dir,10).trim>0);
  assert.equal(headGeometry('open',tip,dir,10).trim,0);assert.equal(headGeometry('none',tip,dir,10).shapes.length,0);
});
test('physics moves roots only and drags their resolved boxes along',()=>{
  const m=scene(...card('c','hi',{body:true,vx:100}));
  const {boxes}=resolve(m,measure);
  stepPhysics(m,settings,.02,new Set(),boxes);
  assert.equal(m.get('c').x,2);
  assert.equal(m.get('c/t').x,0);
  assert.equal(boxes.get('c/t').x,17);
});
test('pinned body stays fixed; collision separates overlapping bodies',()=>{
  const m=scene({id:'a',type:'ellipse',r:8,body:true,pinned:true,vx:100},{id:'b',type:'ellipse',x:8,r:8,body:true});
  const {boxes}=resolve(m,measure);
  stepPhysics(m,{...settings,collision:true},.02,new Set(),boxes);
  assert.equal(m.get('a').x,0);
  assert.ok(m.get('b').x-m.get('a').x>=16);
});
test('numeric and color interpolation',()=>{assert.equal(interpolate(0,10,.5),5);assert.equal(interpolate('#000000','#ffffff',.5),'#808080');assert.equal(interpolate('fill','hug',.5),'fill');});
