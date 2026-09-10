import test from 'node:test';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {DocumentSync} from '../web/document-sync.js';
import {minimalEdit,applyDocumentEdit} from '../extensions/vscode/src/document.cjs';
import {EditHistory,LIMIT} from '../extensions/vscode/src/history.cjs';
import {arrowTarget,slotForEntry} from '../web/navigation.js';
import {matches} from '../web/shortcuts.js';
import {manifest,media} from '../scripts/build-vscode.mjs';
import fs from 'node:fs/promises';
const require=createRequire(import.meta.url);
const tick=()=>new Promise(resolve=>setImmediate(resolve));
const read=file=>fs.readFile(new URL(file,import.meta.url),'utf8');
const optional=file=>fs.readFile(new URL(file,import.meta.url),'utf8').then(text=>text,()=>null);
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
  const actions=JSON.parse(await read('../config/editor-commands.json'));
  const html=await read('../web/index.html');
  for(const match of html.matchAll(/<button id="([^"]+)"/g))assert.ok(actions.some(a=>a.id===match[1]),'missing command for '+match[1]);
  const e={key:'i',ctrlKey:true,altKey:true,metaKey:false,shiftKey:false,isComposing:false};
  assert.ok(matches(e,'ctrl+alt+i'));assert.ok(!matches({...e,isComposing:true},'ctrl+alt+i'));assert.ok(!matches({...e,shiftKey:true},'ctrl+alt+i'));
});
test('the committed extension manifest and command list are what the generator produces',async()=>{
  const actions=JSON.parse(await read('../config/editor-commands.json'));
  // The extension registers commands from commands.json and VS Code only exposes
  // what package.json declares, so both committed copies must track the registry.
  assert.deepEqual(JSON.parse(await read('../extensions/vscode/commands.json')),actions,'run node scripts/build-vscode.mjs');
  assert.deepEqual(JSON.parse(await read('../extensions/vscode/package.json')),manifest(actions),'run node scripts/build-vscode.mjs');
  const declared=new Set(manifest(actions).contributes.commands.map(c=>c.command));
  for(const action of actions)assert.ok(declared.has('visualTypst.'+action.command),'undeclared command '+action.command);
  for(const action of actions.filter(a=>a.key))assert.ok(manifest(actions).contributes.keybindings.some(k=>k.command==='visualTypst.'+action.command),'missing keybinding '+action.command);
});
test('every element the editor binds exists in the served shell',async()=>{
  const html=await read('../web/index.html');
  const ids=new Set([...html.matchAll(/id="([^"]+)"/g)].map(m=>m[1]));
  for(const file of ['../web/editor.js','../web/app.js','../web/preview.js']){
    const source=await read(file);
    // A binding to a missing id throws while the module is evaluated, which
    // leaves the whole webview blank instead of one inert control.
    for(const match of source.matchAll(/\$\('#([^']+)'\)/g))assert.ok(ids.has(match[1]),`${file} binds missing #${match[1]}`);
    for(const match of source.matchAll(/getElementById\('([^']+)'\)/g))assert.ok(ids.has(match[1]),`${file} looks up missing #${match[1]}`);
  }
});
test('the packaged webview copy is byte-identical to the web shell it is built from',async()=>{
  // media/ is a build output; when it exists it must not be a stale shell.
  if(!await optional('../extensions/vscode/media/index.html'))return;
  for(const file of media){
    const [built,packaged]=await Promise.all([fs.readFile(new URL('../web/'+file,import.meta.url)),fs.readFile(new URL('../extensions/vscode/media/'+file,import.meta.url))]);
    assert.ok(built.equals(packaged),'stale extension media: '+file);
  }
});
test('host history keeps every document text, drops redo after a new edit and stays bounded',()=>{
  const history=new EditHistory('a');
  assert.equal(history.canUndo,false);assert.equal(history.peek('undo'),null);assert.equal(history.commit('undo'),null);
  history.record('a');assert.equal(history.canUndo,false,'an identical text is not a step');
  history.record('ab');history.record('abc');assert.equal(history.peek('undo'),'ab');
  assert.equal(history.commit('undo'),'ab');assert.equal(history.peek('redo'),'abc');
  history.record('abx');assert.equal(history.canRedo,false,'a new edit invalidates the redo step');
  for(let i=0;i<LIMIT+10;i++)history.record('x'+i);
  let steps=0;while(history.canUndo){history.commit('undo');steps++;}
  assert.equal(steps,LIMIT);
});
test('host undo and redo replay this document instead of the workbench code-editor command',async()=>{
  const executed=[],document={text:'$x$',version:1,isDirty:false,uri:{toString:()=>'file:///main.typ'},getText(){return this.text;},positionAt(n){return n;}};
  const vscodeStub={
    window:{createOutputChannel:()=>({appendLine(){},dispose(){}})},
    Range:class{constructor(from,to){Object.assign(this,{from,to});}},
    WorkspaceEdit:class{replace(uri,range,text){Object.assign(this,{uri,range,text});}},
    commands:{async executeCommand(id){executed.push(id);}},
    workspace:{async applyEdit(edit){const {from,to}=edit.range;document.text=document.text.slice(0,from)+edit.text+document.text.slice(to);document.version++;document.isDirty=true;return true;}}
  };
  const Module=require('node:module'),load=Module._load;
  Module._load=(request,parent,isMain)=>request==='vscode'?vscodeStub:load(request,parent,isMain);
  let Provider;
  try{({Provider}=require('../extensions/vscode/src/extension.cjs'));}finally{Module._load=load;}
  const provider=new Provider({subscriptions:[],extensionPath:''}),session={document};
  const change=text=>{document.text=text;document.version++;provider.history(session).record(document.getText());};
  assert.equal((await provider.hostCommand(session,'undo')).changed,false,'nothing to undo yet');
  change('$xy$');
  assert.equal((await provider.hostCommand(session,'undo')).changed,true);
  assert.equal(document.text,'$x$');provider.history(session).record(document.getText());
  assert.equal(document.isDirty,true);
  assert.equal((await provider.hostCommand(session,'redo')).changed,true);
  assert.equal(document.text,'$xy$');provider.history(session).record(document.getText());
  assert.equal((await provider.hostCommand(session,'undo')).changed,true);assert.equal(document.text,'$x$');
  assert.deepEqual(executed,[],'the workbench undo command must not be used');
  assert.deepEqual(await provider.hostCommand(session,'configureShortcuts'),{});
  await assert.rejects(provider.hostCommand(session,'unknown'),/未知宿主命令/);
});
