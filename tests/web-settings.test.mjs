import test from 'node:test';
import assert from 'node:assert/strict';
import {JSDOM} from 'jsdom';
import {Typography} from '../web/typography.js';
import {openTypstFile,saveTypstAs,writeTypstFile} from '../web/file-access.js';
import {normalizedMetrics,applySvgMetrics,rawPreviewKey} from '../web/svg-metrics.js';

test('SVG uses backend normalized dimensions and follows editor zoom without article-size input',async()=>{
  const dom=new JSDOM('<output id="editor-font-value"></output><input id="article-font-value"><input id="svg-scale-value">');
  globalThis.document=dom.window.document;
  const saved=[];const typography=new Typography({persist:patch=>saved.push(patch)});
  typography.update({fontSize:18,articleFontSize:12,svgScale:1});
  assert.equal(typography.settings.articleFontSize,undefined,'legacy manual denominator is ignored');
  const metrics=normalizedMetrics({base_font_size_pt:.8,base_font_height_pt:.6,environment_font_size_pt:12});
  const img=document.createElement('img');img.dataset.baseFontWidth=metrics.width;img.dataset.baseFontHeight=metrics.height;
  applySvgMetrics(img);assert.match(img.style.width,/--editor-size.*0\.8.*--svg-scale/);
  assert.equal(metrics.width*typography.size,14.4);
  await typography.set({editorFontSize:24,svgScale:1.25});
  assert.ok(Math.abs(metrics.width*typography.size*typography.settings.svgScale-24)<1e-6);
  assert.equal(document.documentElement.style.getPropertyValue('--editor-size'),'24px');
  assert.deepEqual(saved,[{editorFontSize:24,svgScale:1.25}]);
  assert.throws(()=>normalizedMetrics({width:12,height:8}),/更新并重启/);
  assert.equal(normalizedMetrics({base_font_size_pt:0,base_font_height_pt:0,environment_font_size_pt:11}).width,0);
  assert.notEqual(rawPreviewKey(1,{text:'x',render_id:'a:0'}),rawPreviewKey(1,{text:'x',render_id:'a:1'}));
  assert.notEqual(rawPreviewKey(1,{text:'x',render_id:'a:0'}),rawPreviewKey(2,{text:'x',render_id:'a:0'}));
  dom.window.close();
});

test('native file picker opens arbitrary typ files and save/save-as write through handles',async()=>{
  const writes=[];let closed=0;
  const writable={write:async text=>writes.push(text),close:async()=>closed++};
  const openHandle={name:'outside.typ',getFile:async()=>({name:'outside.typ',text:async()=>'= Outside'})};
  const saveHandle={name:'copy.typ',createWritable:async()=>writable};
  const scope={showOpenFilePicker:async options=>{assert.deepEqual(options.types[0].accept['text/plain'],['.typ']);return [openHandle];},showSaveFilePicker:async options=>{assert.equal(options.suggestedName,'outside.typ');return saveHandle;}};
  assert.deepEqual(await openTypstFile(scope),{name:'outside.typ',source:'= Outside',handle:openHandle,writable:true});
  assert.deepEqual(await saveTypstAs('new text','outside.typ',scope),{name:'copy.typ',handle:saveHandle,writable:true});
  await writeTypstFile(saveHandle,'again');
  assert.deepEqual(writes,['new text','again']);assert.equal(closed,2);
});
