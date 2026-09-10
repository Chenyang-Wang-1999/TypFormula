import test from 'node:test';
import assert from 'node:assert/strict';
import {revealInFormula,visibleCaret,installFormulaViewport} from '../web/formula-layout.js';

function viewport(){return {clientWidth:200,clientHeight:100,clientLeft:1,clientTop:1,scrollWidth:900,scrollHeight:600,scrollLeft:0,scrollTop:0,getBoundingClientRect:()=>({left:20,top:30})};}
test('typing reveals overflowing slots by scrolling both axes, without resizing content',()=>{
  const box=viewport();
  assert.ok(revealInFormula(box,{left:430,right:432,top:250,bottom:270}));
  assert.equal(box.scrollLeft,215);assert.equal(box.scrollTop,143);
  assert.equal(box.clientWidth,200);assert.equal(box.clientHeight,100);assert.equal(box.scrollWidth,900);
  assert.ok(revealInFormula(box,{left:0,right:2,top:0,bottom:15}));
  assert.equal(box.scrollLeft,190);assert.equal(box.scrollTop,108);
});
test('visible slots do not move manually scrolled formula viewports',()=>{
  const box=viewport();box.scrollLeft=150;
  assert.equal(revealInFormula(box,{left:50,right:52,top:50,bottom:65}),false);
  assert.equal(box.scrollLeft,150);
});
test('fixed caret clips to the formula viewport and disappears outside scrollable bounds',()=>{
  const box=viewport();
  assert.equal(visibleCaret({left:15,right:17,top:50,bottom:65},box),null);
  assert.equal(visibleCaret({left:50,right:52,top:150,bottom:165},box),null);
  assert.deepEqual(visibleCaret({left:50,right:52,top:20,bottom:45},box),{left:50,right:52,top:31,bottom:45});
});
test('formula width follows the editor viewport when preview narrows the column',()=>{
  const properties=new Map();let resized,disconnected=false;
  globalThis.window={addEventListener(){},removeEventListener(){},requestAnimationFrame:()=>1,cancelAnimationFrame(){}};
  globalThis.ResizeObserver=class{constructor(callback){resized=callback;}observe(){}disconnect(){disconnected=true;}};
  const view={scrollDOM:{clientWidth:700},dom:{style:{setProperty:(k,v)=>properties.set(k,v)},querySelector:()=>({getBoundingClientRect:()=>({width:40})})}};
  const handle=installFormulaViewport(view);assert.equal(properties.get('--formula-max-width'),'616px');
  view.scrollDOM.clientWidth=360;resized();assert.equal(properties.get('--formula-max-width'),'276px');
  handle.dispose();assert.equal(disconnected,true);
});
