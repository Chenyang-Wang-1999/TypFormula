import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {byteOffset,charOffset,position,offset,edits,insertion} from '../web/source.js';
test('Chinese, emoji and CRLF round trip through Rust bytes and LSP UTF16',()=>{
  const text='中文😀\r\n$x$';for(const pos of [0,1,2,4,6,7,8,9])assert.equal(charOffset(text,byteOffset(text,pos)),pos);
  assert.deepEqual(position(text,7),{line:1,character:1});assert.equal(offset(text,{line:1,character:1}),7);
  assert.throws(()=>offset(text,{line:99,character:0}));
});
test('button insertion wraps selection and separates display equations',()=>{
  assert.deepEqual(insertion('ab',1,1,false),{from:1,to:1,insert:'$""$',start:1});
  assert.deepEqual(insertion('a+b',0,3,false),{from:0,to:3,insert:'$a+b$',start:0});
  assert.equal(insertion('ab',1,1,true).insert,'\n$ "" $\n');
});
test('LSP edits preserve adjacent Unicode and reject overlaps',()=>{
  const text='😀abc';assert.deepEqual(edits(text,[{range:{start:{line:0,character:2},end:{line:0,character:3}},newText:'x'}]),[{from:2,to:3,insert:'x'}]);
  assert.throws(()=>edits('abc',[{range:{start:{line:0,character:0},end:{line:0,character:2}},newText:''},{range:{start:{line:0,character:1},end:{line:0,character:3}},newText:''}]));
});
test('production WASM shares one formula session and retains full source',async()=>{
  const {instance}=await WebAssembly.instantiate(await fs.readFile(new URL('../web/core.wasm',import.meta.url)));
  const w=instance.exports,enc=new TextEncoder(),dec=new TextDecoder();
  const call=a=>{const bytes=enc.encode(JSON.stringify(a)),p=w.alloc(bytes.length);new Uint8Array(w.memory.buffer,p,bytes.length).set(bytes);const out=w.dispatch(p,bytes.length);const result=JSON.parse(dec.decode(new Uint8Array(w.memory.buffer,out,w.output_len())));assert.equal(result.error,undefined);return result;};
  const text='中文😀\n$a/b$ 后文 $x$';let s=call({action:'set_source',source:text});assert.equal(s.source,text);assert.equal(s.blocks.length,0);
  s=call({action:'activate_formula',start:s.equations[0].start});assert.equal(s.source,text);assert.equal(s.blocks.length,1);
  s=call({action:'input',text:'q'});assert.ok(s.source.endsWith(' 后文 $x$'));
  s=call({action:'activate_formula',start:s.equations[1].start});assert.equal(s.blocks.length,1);assert.equal(s.cursor.pos,0);
});
