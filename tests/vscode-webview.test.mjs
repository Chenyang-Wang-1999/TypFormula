import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import {JSDOM} from 'jsdom';
test('VS Code webview edits synchronize through WorkspaceEdit and commands use host history',async()=>{
  let html=await fs.readFile(new URL('../web/index.html',import.meta.url),'utf8');html=html.replace('<head>','<head><meta name="asset-base" content="https://assets.test/">');
  const dom=new JSDOM(html,{url:'https://editor.test',pretendToBeVisual:true}),win=dom.window;
  for(const name of ['window','Window','document','navigator','HTMLElement','Element','Node','Text','MutationObserver','DOMParser','getComputedStyle','KeyboardEvent'])Object.defineProperty(globalThis,name,{value:name==='window'?win:win[name],configurable:true});
  globalThis.requestAnimationFrame=win.requestAnimationFrame.bind(win);globalThis.cancelAnimationFrame=win.cancelAnimationFrame.bind(win);globalThis.innerWidth=1280;globalThis.innerHeight=900;
  win.Range.prototype.getClientRects=()=>[];win.Range.prototype.getBoundingClientRect=()=>({x:0,y:0,top:0,left:0,right:0,bottom:0,width:0,height:0});win.HTMLElement.prototype.scrollIntoView=()=>{};document.fonts={ready:Promise.resolve()};
  let source='hello $x$',version=1;const requests=[];
  const deliver=message=>win.dispatchEvent(new win.MessageEvent('message',{data:message}));
  globalThis.acquireVsCodeApi=()=>({postMessage(message){
    if(message.type!=='request')return;requests.push(message);
    queueMicrotask(()=>{
      let result;
      if(message.route==='/api/status')result={available:false,attachments:false};
      else if(message.route==='/api/document')result={source,version,dirty:false,path:'main.typ',uri:'file:///main.typ',settings:{fontSize:17,fontFamily:'Consolas',previewOnOpen:false}};
      else if(message.route==='/api/edit'){assert.equal(message.body.version,version);assert.equal(message.body.base,source);source=message.body.source;result={source,version:++version,accepted:true,dirty:true};}
      else if(message.route==='/api/lsp')result={result:null,diagnostics:[]};
      else if(message.route==='/api/host-command'){
        if(message.body.command==='undo'){source='hello $x$';deliver({type:'document',source,version:++version,dirty:false});}
        result={};
      }else throw new Error('Unexpected host request '+message.route);
      deliver({type:'reply',id:message.id,result});
    });
  }});
  globalThis.fetch=async url=>{assert.equal(url,'https://assets.test/core.wasm');return new Response(await fs.readFile(new URL('../web/core.wasm',import.meta.url)),{headers:{'Content-Type':'application/wasm'}});};
  const {editorView:view,destroyEditor}=await import('../web/editor.bundle.js');
  assert.equal(document.documentElement.style.getPropertyValue('--editor-size'),'17px');
  view.dispatch({changes:{from:0,insert:'a'}});view.dispatch({changes:{from:0,insert:'b'}});
  await new Promise(r=>setTimeout(r,20));assert.equal(source,'bahello $x$');
  assert.equal(requests.filter(r=>r.route==='/api/edit').length,2);
  deliver({type:'command',id:'undo'});await new Promise(r=>setTimeout(r,20));assert.equal(view.state.doc.toString(),'hello $x$');
  assert.ok(requests.some(r=>r.route==='/api/host-command'&&r.body.command==='undo'));
  deliver({type:'command',id:'shortcuts'});await new Promise(r=>setTimeout(r,20));assert.ok(requests.some(r=>r.body?.command==='configureShortcuts'));
  destroyEditor();win.close();delete globalThis.acquireVsCodeApi;
});
