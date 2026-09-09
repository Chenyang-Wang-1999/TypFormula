// SPDX-License-Identifier: GPL-2.0-or-later
// Exercise the actual browser artifact through its JSON ABI, without a server.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const bytes = await readFile(new URL('../web/core.wasm', import.meta.url));
const { instance } = await WebAssembly.instantiate(bytes, {});
const wasm = instance.exports;
const encoder = new TextEncoder(), decoder = new TextDecoder();
const symbols = JSON.parse(await readFile(new URL('../config/symbols.json', import.meta.url), 'utf8'));
function send(action) {
  const bytes = encoder.encode(JSON.stringify(action));
  const ptr = wasm.alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  const out = wasm.dispatch(ptr, bytes.length);
  const result = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, out, wasm.output_len())));
  assert.equal(result.error, undefined);
  return result;
}
const input = text => send({ action: 'input', text });
const key = (key, modifiers = {}) => send({ action: 'key', key, ...modifiers });
function nodes(view, kind) {
  return [...(view.kind === kind ? [view] : []), ...view.children.flatMap(child => nodes(child, kind))];
}

let state = input('\\frac(a, b) + alpha beta');
assert.equal(state.pending, true);
assert.equal(nodes(state.view, 'fraction').length, 0);
for (const name of ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Tab', 'Home', 'End']) {
  state = key(name); assert.equal(state.pending, true, name);
}
state = key('Enter');
assert.equal(state.pending, false);
assert.equal(nodes(state.view, 'fraction').length, 1);
assert.equal(state.source.trim(), '$ frac(a, b) + alpha beta $');

send({ action: 'clear' }); input('\\alp');
state = send({ action: 'complete', name: 'alpha' });
assert.equal(state.pending, true); assert.equal(nodes(state.view, 'symbol').length, 0);
state = key('Enter'); assert.equal(nodes(state.view, 'symbol')[0].text, symbols.alpha);

input('\\sqrt(x'); state = key('Enter');
assert.equal(state.pending, true); assert.ok(state.message);
input(')'); state = key('Enter'); assert.equal(state.pending, false);
assert.equal(nodes(state.view, 'sqrt').length, 1);

input('\\α中'); key('ArrowLeft'); input(' + ');
state = key('Delete');
assert.equal(nodes(state.view, 'draft-text').map(n => n.text).join(''), 'α + ');
state = key('a', { ctrl: true }); assert.equal(state.selected_source, 'α + ');
state = key('Backspace'); assert.equal(state.pending, true);
state = key('Backspace'); assert.equal(state.pending, true);
state = key('Escape'); assert.equal(state.pending, false);

send({action:'clear'});state=input('\\cases(alph');
const context=state.command;
assert.equal(context.draft,'cases(alph');
state=send({action:'lsp_completions',draft:context.draft,caret:context.draft_caret,items:[{label:'alpha',replacement:'cases(alpha',caret:11}]});
state=key('Tab');assert.equal(state.pending,true);
input(', beta)');state=key('Enter');
assert.equal(nodes(state.view,'raw').length,1);
assert.equal(nodes(state.view,'raw')[0].children.length,0);
assert.ok(state.source.trim().endsWith('$ cases(alpha, beta) $'));
state=key('Backspace');assert.equal(nodes(state.view,'raw').length,0);
state=send({action:'undo'});assert.equal(nodes(state.view,'raw').length,1);

send({action:'clear'});state=send({action:'paste',text:'frac(dif x, 2 pi)'});
assert.equal(state.pending,true);assert.equal(nodes(state.view,'fraction').length,0);
state=key('Enter');assert.equal(nodes(state.view,'fraction').length,1);
assert.deepEqual(nodes(state.view,'raw').map(n=>n.text),['dif']);
send({action:'clear'});input('\\a &= 1 && "given" \\ b &= 2');state=key('Enter');
assert.equal(nodes(state.view,'aligned').length,1);assert.equal(nodes(state.view,'aligned')[0].columns,4);
assert.equal(nodes(state.view,'aligned')[0].children.length,8);
const alignedSource=state.source;state=send({action:'import',source:alignedSource});assert.equal(state.source,alignedSource);
send({action:'clear'});input('\\x/y');state=key('Enter');assert.equal(nodes(state.view,'fraction').length,1);
send({action:'clear'});input('"hello"');state=key('Home');state=key('a',{ctrl:true});
assert.equal(state.selected_source,'"hello"');
console.log('WASM smoke passed: drafts, LSP edits, nested SVG atoms, fractions, alignment, clipboard, Unicode.');

const definitions='#let ratio(x, y) = $frac(#x, #x + #y)$\n#let opaque(x) = $cancel(#x)$';
state=send({action:'import',source:`${definitions}\n$ ratio(a, b) + opaque(c) $`});
assert.deepEqual(state.macros.map(m=>m.expandable),[true,false]);
let args=nodes(state.view,'macro-argument');assert.equal(args.length,3);
const second=nodes(args[1],'stop')[1].cursor;
state=send({action:'click',cursor:second});state=input('z');
assert.ok(state.source.endsWith('$ ratio(a z, b) + opaque(c) $'));
assert.equal(state.cursor.occurrence,second.occurrence.replace('.p1','.p2'));
assert.equal(nodes(state.view,'stop').filter(n=>n.active).length,1);
state=send({action:'undo'});assert.ok(state.source.endsWith('$ ratio(a, b) + opaque(c) $'));
state=send({action:'set_definitions',definitions:'#let ratio(x, y) = $cancel(#x + #y)$'});
assert.equal(nodes(state.view,'macro').length,0);assert.equal(nodes(state.view,'raw').length,2);
state=send({action:'undo'});assert.equal(nodes(state.view,'macro-argument').length,3);
const sourceBefore=state.source;
assert.throws(()=>send({action:'set_definitions',definitions:'#let broken(x) = $'}));
assert.equal(send({action:'state'}).source,sourceBefore);
state=send({action:'set_definitions',definitions:''});assert.equal(nodes(state.view,'macro').length,0);
state=send({action:'set_definitions',definitions});assert.equal(nodes(state.view,'macro').length,1);
console.log('WASM macro checks passed: classification, linked projections, occurrence, undo, invalid drafts, removal and re-addition.');

const pd='#let pd(f, x) = $frac(partial #f, partial #x)$';
const jac='#let jac(f1, f2, x1, x2) = $mat(pd(#f1, #x1), pd(#f1, #x2); pd(#f2, #x1), pd(#f2, #x2))$';
state=send({action:'import',source:`${pd}\n${jac}\n$jac(a, b, x, y)$`});
assert.deepEqual(state.macros.map(m=>m.expandable),[true,true]);
assert.equal(nodes(state.view,'grid').length,1);assert.equal(nodes(state.view,'fraction').length,4);
args=nodes(state.view,'macro-argument');
assert.deepEqual(args.map(n=>n.columns),[0,2,0,3,1,2,1,3]);
assert.deepEqual(args.map(n=>n.text),['f1','x1','f1','x2','f2','x1','f2','x2']);
const repeated=nodes(args[2],'stop')[1].cursor;
send({action:'click',cursor:repeated});state=input('z');
assert.ok(state.source.endsWith('$jac(a z, b, x, y)$'));
assert.equal(state.cursor.occurrence,repeated.occurrence.replace('.p1','.p2'));
for(const arg of nodes(state.view,'macro-argument').filter(n=>n.text==='f1'))assert.equal(nodes(arg,'char').map(n=>n.text).join(''),'az');
assert.equal(nodes(state.view,'stop').filter(n=>n.active).length,1);
state=send({action:'set_definitions',definitions:`${pd}\n${jac}\n#let pd(f, x) = $cancel(#f + #x)$`});
assert.equal(state.macros[0].shadowed,true);assert.equal(nodes(state.view,'fraction').length,4);
state=send({action:'set_definitions',definitions:`#let pd(f, x) = $cancel(#f + #x)$\n${jac}`});
assert.equal(state.macros[1].expandable,false);assert.equal(nodes(state.view,'macro').length,0);
state=send({action:'undo'});assert.equal(nodes(state.view,'fraction').length,4);
const roundtrip=state.source;state=send({action:'import',source:roundtrip});assert.equal(state.source,roundtrip);
console.log('WASM nested macros passed: Jacobian, outer parameter mapping, captured versions, cache invalidation, undo and source roundtrip.');

// Informational timings include the JSON ABI and view generation, exclude DOM/SVG.
const many=Array.from({length:100},(_,i)=>`#let macro${i}(x, y) = $frac(#x, #x + #y)$`).join('\n');
const begin=performance.now();state=send({action:'import',source:`${many}\n$ macro0(a, b) $`});
const cold=performance.now()-begin;
for(let i=0;i<20;i++)send({action:'state'});
const samples=[];
for(let i=0;i<200;i++){const t=performance.now();send({action:'key',key:i%2?'ArrowLeft':'ArrowRight'});samples.push(performance.now()-t);}
samples.sort((a,b)=>a-b);
console.log(`100 definitions / one call: import ${cold.toFixed(2)} ms, cached key median ${samples[100].toFixed(2)} ms, p95 ${samples[190].toFixed(2)} ms (local Node/WASM, no DOM or SVG).`);
