import test from 'node:test';
import assert from 'node:assert/strict';
import {DocumentSync} from '../web/document-sync.js';
import {minimalEdit,applyDocumentEdit} from '../extensions/vscode/src/document.cjs';
import {arrowTarget,slotForEntry} from '../web/navigation.js';
import {matches} from '../web/shortcuts.js';
import fs from 'node:fs/promises';
const tick=()=>new Promise(resolve=>setImmediate(resolve));
test('minimal workspace edits do not split emoji or rewrite adjacent Typst',()=>{
  for(const [before,after] of [['a😀b','a😃b'],['中文\r\n$x$','中文\r\n$x+y$'],['😀','😀x'],['abc','']]){
    const edit=minimalEdit(before,after);assert.equal(before.slice(0,edit.from)+edit.text+before.slice(edit.to),after);
    assert.ok(!/[\uD800-\uDBFF]$/.test(before.slice(0,edit.from)));
  }
});
test('WorkspaceEdit checks the actual TextDocument version before applying',async()=>{
  const document={text:'$x$',version:1,isDirty:false,uri:{toString:()=>'/main.typ'},getText(){return this.text;},positionAt(n){return n;}};
  class Range{constructor(from,to){Object.assign(this,{from,to});}}
  class WorkspaceEdit{replace(uri,range,text){Object.assign(this,{range,text});}}
  const vscode={Range,WorkspaceEdit,workspace:{async applyEdit(e){document.text=document.text.slice(0,e.range.from)+e.text+document.text.slice(e.range.to);document.version++;document.isDirty=true;return true;}}};
  const accepted=await applyDocumentEdit(vscode,document,{version:1,base:'$x$',source:'$xy$'});assert.equal(accepted.accepted,true);assert.equal(document.text,'$xy$');
  const stale=await applyDocumentEdit(vscode,document,{version:1,base:'$x$',source:'LOST'});assert.equal(stale.accepted,false);assert.equal(document.text,'$xy$');
});
test('one pending edit coalesces rapid typing without replacing the active formula',async()=>{
  const requests=[],resolvers=[],replaced=[];
  const sync=new DocumentSync({source:'a',version:1},body=>{requests.push(body);return new Promise(r=>resolvers.push(r));},{replace:s=>replaced.push(s),conflict:()=>{}});
  sync.edit('ab');sync.edit('abc');assert.equal(requests.length,1);
  resolvers.shift()({accepted:true,source:'ab',version:2,dirty:true});await tick();
  assert.equal(requests.length,2);assert.equal(requests[1].base,'ab');assert.equal(requests[1].source,'abc');assert.deepEqual(replaced,[]);
  resolvers.shift()({accepted:true,source:'abc',version:3,dirty:true});await sync.flush();assert.equal(sync.confirmed.source,'abc');
});
test('external changes preserve unacknowledged edits and require explicit conflict resolution',async()=>{
  let reply;const conflicts=[],replaced=[];
  const sync=new DocumentSync({source:'a',version:1},()=>new Promise(r=>reply=r),{replace:s=>replaced.push(s),conflict:c=>conflicts.push(c)});
  sync.edit('local');sync.remote({source:'remote',version:2});assert.equal(sync.local,'local');assert.equal(replaced.length,0);
  reply({accepted:false,source:'remote',version:2});await assert.rejects(sync.flush());
  await sync.resolve(false);assert.equal(sync.local,'remote');assert.deepEqual(replaced,['remote']);assert.equal(conflicts.at(-1),null);
});
test('up down left right select formula boundaries and the nearest entry slot',()=>{
  const e={start:9,from:5,to:12};
  assert.equal(arrowTarget({key:'ArrowRight',head:5,equations:[e]}),e);
  assert.equal(arrowTarget({key:'ArrowLeft',head:12,equations:[e]}),e);
  assert.equal(arrowTarget({key:'ArrowDown',head:0,projected:6,equations:[e]}),e);
  assert.equal(arrowTarget({key:'ArrowUp',head:15,projected:11,equations:[e]}),e);
  assert.equal(arrowTarget({key:'ArrowRight',head:4,equations:[e]}),undefined);
  const stops=[{cursor:{slices:[]},x:0,y:10},{cursor:{slices:[1]},x:15,y:0},{cursor:{slices:[2]},x:30,y:0},{cursor:{slices:[3]},x:15,y:20},{cursor:{slices:[]},x:60,y:10}];
  assert.equal(slotForEntry(stops,'ArrowRight',50),stops[0]);assert.equal(slotForEntry(stops,'ArrowLeft',0),stops[4]);
  assert.equal(slotForEntry(stops,'ArrowDown',28),stops[2]);assert.equal(slotForEntry(stops,'ArrowUp',17),stops[3]);
});
test('all editing toolbar buttons have configurable commands; shortcuts ignore IME and extra modifiers',async()=>{
  const actions=JSON.parse(await fs.readFile(new URL('../config/editor-commands.json',import.meta.url),'utf8'));
  const html=await fs.readFile(new URL('../web/index.html',import.meta.url),'utf8');
  for(const match of html.matchAll(/<button id="([^"]+)"/g))assert.ok(actions.some(a=>a.id===match[1]),'missing command for '+match[1]);
  const e={key:'i',ctrlKey:true,altKey:true,metaKey:false,shiftKey:false,isComposing:false};
  assert.ok(matches(e,'ctrl+alt+i'));assert.ok(!matches({...e,isComposing:true},'ctrl+alt+i'));assert.ok(!matches({...e,shiftKey:true},'ctrl+alt+i'));
});
