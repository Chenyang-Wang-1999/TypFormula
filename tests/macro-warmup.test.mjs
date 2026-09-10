import test from 'node:test';
import assert from 'node:assert/strict';
import {MacroWarmup} from '../web/macro-warmup.js';

const item=(start,size=1)=>({id:`${start}:${start+9}:0:0`,template_source:'cancel(a)',svg:'<svg/>',base_font_size_pt:size,base_font_height_pt:1,environment_font_size_pt:12});
test('warmup cache separates definition versions and repeated Raw ranges without editing source',async()=>{
  const source='#let fixed(x) = $#x + cancel(a)$';let applied=0;
  const warm=new MacroWarmup({request:async body=>{assert.equal(body.source,source);return {results:[{key:'outer',failed:false,items:[item(1),item(20,2)]},{key:'inner',failed:false,items:[item(1,3)]}]};},snapshot:()=>({source}),apply:()=>{applied++;return true;},updated:()=>{}});
  await warm.run();
  assert.equal(warm.lookup({warmup_key:'outer',warmup_range:[20,29],text:'cancel(a)'}).base_font_size_pt,2);
  assert.equal(warm.lookup({warmup_key:'inner',warmup_range:[1,10],text:'cancel(a)'}).base_font_size_pt,3);
  assert.equal(applied,0,'successful preheating must not rebuild the editing session');warm.dispose();
});
test('stale preheats are discarded and failure classification waits for command drafts',async()=>{
  let resolve,blocked=true,applied=0;
  const warm=new MacroWarmup({request:()=>new Promise(done=>{resolve=done;}),snapshot:()=>({source:'old'}),apply:results=>{assert.deepEqual(results,[['new',true]]);if(blocked)return false;applied++;return true;},updated:()=>{}});
  const first=warm.run();warm.schedule();clearTimeout(warm.timer);resolve({results:[{key:'old',failed:true,items:[]}]});await first;clearTimeout(warm.timer);
  assert.equal(warm.records.size,0);
  const second=warm.run();resolve({results:[{key:'new',failed:true,items:[]}]});await second;
  assert.equal(applied,0);blocked=false;warm.flush();assert.equal(applied,1);warm.dispose();
});
