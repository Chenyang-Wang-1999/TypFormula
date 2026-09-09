// Run the production preview scheduler against actual WASM document snapshots.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';

const {instance}=await WebAssembly.instantiate(await readFile(new URL('../web/core.wasm',import.meta.url)),{});
const wasm=instance.exports, encoder=new TextEncoder(), decoder=new TextDecoder();
function call(action) {
  const bytes=encoder.encode(JSON.stringify(action)),ptr=wasm.alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer,ptr,bytes.length).set(bytes);
  const out=wasm.dispatch(ptr,bytes.length);
  const state=JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer,out,wasm.output_len())));
  assert.equal(state.error,undefined);return state;
}
const previews=new Map(), attachments=new Map(), requests=[], revoked=[], timers=new Set();
let urls=0;
const context=vm.createContext({
  state:call({action:'import',source:'$ cancel(x) $'}),previews,attachments,call,rawReady:true,
  attachmentKey(expression,definitions,display){return JSON.stringify([definitions,display,expression]);},
  scheduleAttachments(){},render(){context.schedulePreviews();},$(){return {};},
  setTimeout(fn){timers.add(fn);return fn;},clearTimeout(fn){timers.delete(fn);},
  redraw(){},Blob,
  URL:{createObjectURL(){return 'blob:preview-'+(++urls);},revokeObjectURL(url){revoked.push(url);}},
  DOMParser:class {parseFromString(){return {documentElement:{localName:'svg'}};}},
  api(path,body){
    assert.equal(path,'/api/render');
    const ids=[];
    function visit(node){if(node.render_id&&body.raw.some(r=>node.render_id.startsWith(r.id+':')))ids.push(node.render_id);node.children.forEach(visit);}
    context.state.blocks.forEach(block=>visit(block.view));
    return new Promise((resolve,reject)=>requests.push({body,ids,resolve,reject}));
  },
});
const app=await readFile(new URL('../web/app.js',import.meta.url),'utf8');
vm.runInContext(app.slice(app.indexOf('let renderTimer,'),app.indexOf("window.addEventListener('pagehide'")),context);
const schedule=()=>context.schedulePreviews();
const change=action=>{context.state=call(action);schedule();};
function finish(request){request.resolve({items:request.ids.map(id=>({id,svg:'<svg/>',width:10,height:12}))});}

schedule();const first=context.runPreviews();assert.equal(requests.length,1);
// Move source ranges and change context while the first compile is in flight.
change({action:'set_context',index:0,text:'前文 #set text(size: 18pt)\n'});
finish(requests[0]);await first;
const original=previews.get('cancel(x)');assert.equal(original.status,'ready');
const originalUrl=original.url;
assert.equal(timers.size,0);

// Typing, context edits, deletion/undo and insertion of the same Raw reuse it.
change({action:'input',text:'abc'});
change({action:'set_context',index:0,text:'#set text(size: 40pt)\n'});
await context.runPreviews();assert.equal(requests.length,1);
change({action:'clear'});change({action:'undo'});
change({action:'insert_formula',index:1,offset:0,display:false});
change({action:'input',text:'\\cancel(x)'});
change({action:'key',key:'Enter'});
await context.runPreviews();assert.equal(requests.length,1);
assert.equal(context.previewFor({text:'cancel(x)',render_id:'shifted'}).url,originalUrl);

// A newly introduced Raw requests only uncached source ranges. Its failure
// cannot clear or overwrite an already successful preview.
change({action:'input',text:'\\cancel(y)'});change({action:'key',key:'Enter'});
const second=context.runPreviews();assert.equal(requests.length,2);
assert.equal(requests[1].body.raw.length,1);
assert.equal(requests[1].body.source.slice(requests[1].body.raw[0].start,requests[1].body.raw[0].end),'cancel(y)');
requests[1].reject(new Error('document diagnostic'));await second;
assert.equal(previews.get('cancel(y)').status,'error');
assert.equal(original.url,originalUrl);assert.equal(original.status,'ready');
change({action:'input',text:'z'});await context.runPreviews();assert.equal(requests.length,2);

// Explicit retry removes only the failed spelling, like the Enter handler.
previews.delete('cancel(y)');schedule();const retry=context.runPreviews();
assert.equal(requests.length,3);
// A later edit introduces another Raw while this request is outstanding.
change({action:'input',text:'\\cancel(z)'});change({action:'key',key:'Enter'});
finish(requests[2]);await retry;
assert.equal(previews.get('cancel(y)').status,'ready');
assert.equal(previews.get('cancel(z)').status,'waiting');
const third=context.runPreviews();assert.equal(requests.length,4);finish(requests[3]);await third;
assert.equal(original.url,originalUrl);assert.equal(urls,3);assert.deepEqual(revoked,[]);
console.log('Preview cache passed: stable SVG URLs, shifted ranges, context edits, undo, shared source, failures, retries and in-flight insertions.');

// A global refresh retires both kinds of SVG and discards an earlier response.
change({action:'input',text:'\\cancel(w)'});change({action:'key',key:'Enter'});
const stale=context.runPreviews(), staleRequest=requests.at(-1);
attachments.set('old',{status:'ready',upper:'limits',lower:null});
context.refreshAllSvg();
assert.equal(attachments.size,0);
assert.equal(previews.get('cancel(x)').status,'waiting');
const beforeStale=urls;finish(staleRequest);await stale;assert.equal(urls,beforeStale);
const fresh=context.runPreviews(), freshRequest=requests.at(-1);
assert.ok(freshRequest.body.raw.length>1);finish(freshRequest);await fresh;
assert.notEqual(previews.get('cancel(x)').url,originalUrl);

function find(node,kind){return node.kind===kind?node:node.children.map(c=>find(c,kind)).find(Boolean);}
function edit(action){context.state=call(action);context.updateAttachmentEdits(context.state);schedule();}
change({action:'import',source:'$ stretch(arrow.r)_b^a + cancel(x) $'});
let script=find(context.state.view,'script');
const baseName=find(script.children[0],'raw').text;
const baseRecord={status:'ready',expression:baseName,url:'blob:stretch-base'};
previews.set(baseName,baseRecord);
const unrelated=previews.get('cancel(x)').url;

// Merely visiting either slot must leave the base alone.
edit({action:'click',cursor:script.children[1].children[0].cursor});
edit({action:'click',cursor:context.state.view.children.at(-1).cursor});
assert.equal(previews.get(baseName),baseRecord);

// Keep a dirty session across upper/lower slots, then refresh on keyboard exit.
script=find(context.state.view,'script');
edit({action:'click',cursor:script.children[1].children[0].cursor});
edit({action:'input',text:'x'});
assert.equal(previews.get(baseName),baseRecord);
script=find(context.state.view,'script');
edit({action:'click',cursor:script.children[2].children[0].cursor});
assert.equal(previews.get(baseName),baseRecord);
edit({action:'input',text:'y'});
const key=context.attachmentKey(find(context.state.view,'script').attachment,context.state.formula_definitions,context.state.display);
attachments.set(key,{status:'ready',upper:'limits',lower:null});
edit({action:'click',cursor:context.state.view.children.at(-1).cursor});
assert.equal(previews.get(baseName).status,'waiting');
assert.equal(attachments.has(key),false);
assert.ok(revoked.includes('blob:stretch-base'));
assert.equal(previews.get('cancel(x)').url,unrelated);

// Real DOM focus loss uses the same rule; no keyboard movement is required.
previews.set(baseName,{status:'ready',expression:baseName,url:'blob:second-base'});
script=find(context.state.view,'script');
edit({action:'click',cursor:script.children[1].children[0].cursor});
edit({action:'input',text:'z'});
context.updateAttachmentEdits(context.state,false);
assert.ok(revoked.includes('blob:second-base'));
assert.equal(previews.get(baseName).status,'waiting');

// A cancelled draft and cursor-only moves do not count as completed edits.
previews.set(baseName,{status:'ready',expression:baseName,url:'blob:unchanged'});
context.updateAttachmentEdits(context.state);
edit({action:'input',text:'\\cancel(q)'});
edit({action:'key',key:'Escape'});
context.updateAttachmentEdits(context.state,false);
assert.equal(previews.get(baseName).url,'blob:unchanged');
console.log('SVG refresh passed: global refresh, obsolete responses, edited script exits, DOM blur, unchanged visits and cancelled drafts.');

// Attachment placement also refreshes across inactive formulas, and never updates
// continuously while a script slot is being edited.
context.attachmentReady=true;context.attachmentBusy=false;context.attachmentTimer=undefined;
vm.runInContext(app.slice(app.indexOf('function attachmentKey('),app.indexOf('async function api(')),context);
const attachmentRequests=[];
context.api=(path,body)=>{
  assert.equal(path,'/api/attachments');
  return new Promise(resolve=>attachmentRequests.push({body,resolve}));
};
change({action:'import',source:'$ stretch(arrow.r)^a $\nBetween $stretch(arrow.r)^b$'});
attachments.clear();
script=find(context.state.view,'script');
edit({action:'click',cursor:script.children[1].children[0].cursor});
await context.runAttachments();assert.equal(attachmentRequests.length,0);
context.updateAttachmentEdits(context.state,false);
const discarded=context.runAttachments();assert.equal(attachmentRequests.length,1);
context.refreshAllSvg();
const beforeAttachment=urls;
const attachmentResult={upper:'limits',lower:null};
attachmentRequests[0].resolve(structuredClone(attachmentResult));await discarded;
assert.equal(urls,beforeAttachment);
const activeAttachment=context.runAttachments();attachmentRequests[1].resolve(structuredClone(attachmentResult));await activeAttachment;
const inactiveAttachment=context.runAttachments();
assert.equal(attachmentRequests[2].body.display,false);
assert.ok(attachmentRequests[2].body.definitions.includes('Between'));
attachmentRequests[2].resolve(structuredClone(attachmentResult));await inactiveAttachment;
assert.equal(attachments.size,2);assert.ok([...attachments.values()].every(r=>r.status==='ready'));
console.log('Attachment refresh passed: editing pause, global invalidation, obsolete response disposal and inactive formulas.');
