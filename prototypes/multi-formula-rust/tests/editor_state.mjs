// User-run tests. No server, browser, fonts or downloaded npm dependencies.
import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';

const moduleUrl = source => 'data:text/javascript;base64,' + Buffer.from(source).toString('base64');
const fontSource = await readFile(new URL('../web/math-font.js', import.meta.url), 'utf8');
const fontUrl = moduleUrl(fontSource);
const editorSource = await readFile(new URL('../web/editor.js', import.meta.url), 'utf8');
const {FormulaEditor, byteLength, sliceBytes, mapOffset} = await import(moduleUrl(
  editorSource.replace("'./math-font.js'", JSON.stringify(fontUrl))));
const {mathGlyph} = await import(fontUrl);

test('mathematical italic glyphs do not change source text', () => {
  const source = 'x';
  assert.equal(mathGlyph(source), '𝑥');
  assert.equal(source, 'x');
  assert.equal(mathGlyph('h'), 'ℎ');
  assert.equal(mathGlyph('123'), '123');
});

test('offset mapping uses UTF-8 bytes and keeps unrelated selections', () => {
  const source = '中文 $x$';
  assert.equal(byteLength('中文'), 6);
  assert.equal(sliceBytes(source, 7, 10), '$x$');
  assert.equal(mapOffset(8, 0, 0, byteLength('前言')), 14);
  assert.equal(mapOffset(8, 8, 9, 4), 12);
  assert.equal(mapOffset(2, 8, 9, 4), 2);
});

function editor() {
  const stops = [2,3,4].map((offset, i) => ({offset,path:'body',
    element:{getBoundingClientRect:() => ({left:i*10,top:0,bottom:20})}}));
  const exits = [];
  const instance = Object.create(FormulaEditor.prototype);
  Object.assign(instance, {stops, head:2, anchor:2, cursorPath:'body',
    formula:{projection:{range:{start:1,end:5}},body:{start:2,end:4}},
    host:{exit:(...args) => exits.push(args)},paintCaret:() => {}});
  return {instance,exits};
}

test('leaving a formula transfers the cursor without requiring preview', () => {
  const {instance,exits} = editor();
  instance.move('ArrowLeft',false);
  assert.deepEqual(exits, [[1,'ArrowLeft']]);
  assert.equal(instance.formula.body.start,2);
  instance.head = 4;
  instance.move('ArrowRight',false);
  assert.deepEqual(exits[1], [5,'ArrowRight']);
});

test('multiple display editors retain independent cursors', () => {
  const first = editor().instance, second = editor().instance;
  first.move('ArrowRight',false);
  assert.equal(first.head,3);
  assert.equal(second.head,2);
  second.mapEdit(0,0,6);
  assert.equal(second.head,8);
  assert.equal(first.head,3);
});

test('shift-tab at the first slot exits on the left', () => {
  const {instance,exits} = editor();
  instance.move('Tab',true);
  assert.deepEqual(exits, [[1,'Tab']]);
});
