import {request,assetUrl,inVSCode} from './transport.js';
import {slotForEntry} from './navigation.js';
import {revealInFormula,visibleCaret} from './formula-layout.js';
import {normalizedMetrics,applySvgMetrics,rawPreviewKey} from './svg-metrics.js';
import {MacroWarmup} from './macro-warmup.js';
// SPDX-License-Identifier: GPL-2.0-or-later
// Painter/event bridge only. Rust/WASM owns MathData, all commands and the cursor.
import { mathGlyph } from './math-font.js';
const $ = selector => document.querySelector(selector);
const keyboard = $('#keyboard'), canvas = $('#canvas'), caret = $('#caret'), popup = $('#completions');
const encoder = new TextEncoder(), decoder = new TextDecoder();
let beforeRequest = async () => {};
let changed = () => {}, exit = () => {}, history = () => {}, documentPath = () => "main.typ";
let previewSession=0;
let macroWarmup;
let warmupPath;
let wasm, state, composing = false, suppressComposition = false;
let completionTimer, completionBusy = false, requestedCommand = '', serviceReady = false;
const previews = new Map();
let rawReady=false, redrawQueued = false;
let attachmentReady = false, attachmentBusy = false, attachmentTimer;
const attachments = new Map();
function attachmentKey(expression,definitions=state.formula_definitions,display=state.display) { return JSON.stringify([definitions,display,expression]); }
function attachmentFor(node) {
  if(!attachmentReady || !node.attachment)return null;
  const key=attachmentKey(node.attachment,node.attachmentDefinitions,node.attachmentDisplay);
  if(!attachments.has(key)){
    // Layout results depend on the definition prefix as well, so a long editing
    // session leaves keys behind that no block references any more.
    if(attachments.size>=256)for(const [old,record] of attachments){if(record.status==='waiting')continue;attachments.delete(old);if(attachments.size<192)break;}
    attachments.set(key,{status:'waiting'});
  }
  return attachments.get(key);
}
function scheduleAttachments() {
  clearTimeout(attachmentTimer);
  if(attachmentReady)attachmentTimer=setTimeout(runAttachments,160);
}
function paintAttachmentStatus() {
  const label=$('#attachment-status'), expressions=[];
  function visit(node) { if(node.attachment)expressions.push(node.attachment);node.children.forEach(visit); }
  visit(state.view);
  label.hidden=expressions.length===0;
  label.classList.remove('error');label.title='';
  if(label.hidden)return;
  if(!attachmentReady) {
    label.textContent='附件布局未连接';label.title='请运行 start.cmd 构建原生适配器，并刷新页面。';return;
  }
  const records=expressions.map(e=>attachments.get(attachmentKey(e)));
  const failed=records.find(r=>r?.status==='error');
  if(failed) {
    label.textContent=`附件布局失败：${failed.error}`;
    label.title=failed.error;label.classList.add('error');
  } else {
    label.textContent=records.some(r=>r?.status!=='ready')?'Typst 正在解析附件…':'附件布局：Typst';
  }
}
async function runAttachments() {
  if(attachmentBusy || state.pending || attachmentEdits.size)return;
  const candidates=[];
  for(const block of state.blocks) {
    function visit(node) {
      if(node.attachment) {
        const key=attachmentKey(node.attachment,block.definitions,block.display);
        if(!attachments.has(key))attachments.set(key,{status:'waiting'});
        candidates.push({key,request:{expression:node.attachment,definitions:block.definitions,display:block.display}});
      }
      node.children.forEach(visit);
    }
    visit(block.view);
  }
  const task=candidates.find(c=>attachments.get(c.key)?.status==='waiting');
  if(!task)return;
  const {key,request}=task, record=attachments.get(key);
  attachmentBusy=true;
  try {
    const result=await api('/api/attachments',request);
    if(attachments.get(key)!==record)return;
    if(![null,'limits','scripts'].includes(result.upper)||![null,'limits','scripts'].includes(result.lower))throw new Error('无效的 Typst 附件位置');
    Object.assign(record,{status:'ready',...result});
  } catch(error) { Object.assign(record,{status:'error',error:error.message}); }
  finally { attachmentBusy=false;redraw(); }
}
async function api(path, body) {
  if(body && ["/api/render","/api/attachments"].includes(path))body={...body,path:documentPath()};
  if(path!=='/api/status')await beforeRequest();
  return request(path,body);
}
function serviceLabel(text, detail='') { const badge=$('.badge');badge.textContent=text;badge.title=detail; }
function commandSignature() { return state?.command ? JSON.stringify(state.command) : ''; }
function scheduleCompletion() {
  clearTimeout(completionTimer);
  if(!serviceReady || !state?.command)return;
  completionTimer=setTimeout(async()=>{
    const signature=commandSignature();
    if(completionBusy || !signature || signature===requestedCommand)return;
    const context=state.command, local=state.candidates;
    requestedCommand=signature;completionBusy=true;
    try {
      const result=await api('/api/completion',context);
      if(commandSignature()!==signature)return;
      const items=result.items;
      // Keep explicit editor templates (e.g. frac's empty slots) available too.
      for(const label of local)if(!items.some(i=>i.label===label))items.push({label,replacement:label,caret:encoder.encode(label).length});
      serviceLabel('Tinymist LSP');
      state=call({action:'lsp_completions',draft:context.draft,caret:context.draft_caret,items});
      render();paintCaret();
    } catch(error) { serviceLabel('内置补全',error.message); }
    finally { completionBusy=false;if(commandSignature()!==signature)scheduleCompletion(); }
  },120);
}
function redraw() { if(!redrawQueued){redrawQueued=true;requestAnimationFrame(()=>{redrawQueued=false;render();paintCaret();});} }
let renderTimer, renderBusy=false, contextFocus=null;
// Different occurrences and entry sessions can have different environment
// sizes. Keep the dimensions and their denominator in the same cache record.
function invalidatePreview(sourceOrKey) {
  for(const [key,record] of previews){
    if(key!==sourceOrKey&&(record.expression!==sourceOrKey||record.session!==previewSession))continue;
    if(record.url)URL.revokeObjectURL(record.url);
    previews.delete(key);
  }
}
function invalidateAttachment(key) {
  attachments.delete(key);
}
function refreshAllSvg() {
  for(const source of previews.keys())invalidatePreview(source);
  for(const key of attachments.keys())invalidateAttachment(key);
  render();
  $('#status').textContent=state.pending?'已请求更新；确认或取消命令后刷新所有 SVG':'正在更新所有 SVG…';
}

// Track edited script slots using the view's existing cursor projections.
// Navigation alone is not an edit. Stay in the same session when moving
// between an upper and lower slot, and include nested script ancestors.
const attachmentEdits=new Map();
function attachmentContent(node) {
  return [node.kind,node.text,node.columns,node.children.filter(c=>!['stop','draft-caret'].includes(c.kind)).map(attachmentContent)];
}
function focusedAttachments(snapshot) {
  const found=new Map(), cursor=snapshot.cursor;
  function containsCursor(node) {
    const c=node.cursor;
    return (c && c.pos===cursor.pos && c.occurrence===cursor.occurrence && JSON.stringify(c.slices)===JSON.stringify(cursor.slices)) || node.children.some(containsCursor);
  }
  function visit(node,path) {
    if(node.kind==='script' && node.children.slice(1).some(containsCursor)) {
      const raw=new Set();
      function base(part){if(part.kind==='raw')raw.add(part.text);part.children.forEach(base);}
      base(node.children[0]);
      found.set(JSON.stringify([snapshot.active_formula,path]),{
        signature:JSON.stringify(node.children.slice(1).map(attachmentContent)),raw,
        attachment:node.attachment?attachmentKey(node.attachment,snapshot.formula_definitions,snapshot.display):null,
      });
    }
    node.children.forEach((child,i)=>visit(child,[...path,i]));
  }
  visit(snapshot.view,[]);return found;
}
function updateAttachmentEdits(snapshot,focused=true) {
  if(!focused && snapshot.pending)return;
  const current=focusedAttachments(snapshot);
  let refreshed=false;
  for(const [key,edit] of attachmentEdits) {
    const next=current.get(key);
    if(next){edit.latest=next;}
    if(focused && next)continue;
    if(edit.initial!==edit.latest.signature) {
      for(const source of edit.latest.raw)invalidatePreview(source);
      if(edit.latest.attachment)invalidateAttachment(edit.latest.attachment);
      refreshed=true;
    }
    attachmentEdits.delete(key);
  }
  if(focused)for(const [key,latest] of current)if(!attachmentEdits.has(key))attachmentEdits.set(key,{initial:latest.signature,latest});
  if(refreshed){schedulePreviews();scheduleAttachments();}
}
function previewFor(node) {
  const actual=previews.get(rawPreviewKey(previewSession,node));
  return actual?.status==='ready'?actual:macroWarmup?.lookup(node)||actual||{status:'waiting'};
}
function visitRaw(callback) {
  for(const block of state.blocks) {
    function visit(node) {
      if(node.kind==='raw')callback(node,block);
      node.children.forEach(visit);
    }
    visit(block.view);
  }
}
function schedulePreviews() {
  clearTimeout(renderTimer);
  let waiting=false;
  // A record belongs to one compile snapshot, and its key embeds that snapshot,
  // so records from an older session can never be looked up again. Drop them
  // with their blob URLs instead of accumulating one per keystroke.
  for(const [key,record] of previews)if(record.session!==previewSession){if(record.url)URL.revokeObjectURL(record.url);previews.delete(key);}
  visitRaw(node=>{
    const key=rawPreviewKey(previewSession,node);
    if(!previews.has(key))previews.set(key,{key,session:previewSession,status:'waiting',expression:node.text});
    if(previews.get(key).status==='waiting')waiting=true;
  });
  if(waiting && !state.pending)renderTimer=setTimeout(runPreviews,160);
}
function syncPreviewResults() {
  visitRaw((node,block)=>{
    const status=previewFor(node).status;
    if(status==='ready'||status==='error')state=call({action:'preview_result',formula:block.index,source:node.text,definitions:node.definitions??block.definitions,display:block.display,failed:status==='error'});
  });
}
async function runPreviews() {
  if(renderBusy || !rawReady || state.pending)return;
  // IDs belong only to this compile snapshot. Keep their record references so
  // typing, source-offset shifts and formula insertion cannot redirect replies.
  const targets=new Map();
  visitRaw(node=>{
    const record=previews.get(rawPreviewKey(previewSession,node));
    if(node.render_id && record?.status==='waiting')targets.set(node.render_id,record);
  });
  if(!targets.size)return;
  const request={...state.render,raw:state.render.raw.filter(range=>[...targets.keys()].some(id=>id.startsWith(range.id+':')))};
  const records=new Set(targets.values());
  for(const record of records)record.status='loading';
  renderBusy=true;
  try {
    const result=await api('/api/render',request);
    for(const item of result.items) {
      const record=targets.get(item.id);
      if(!record || previews.get(record.key)!==record || record.status==='ready')continue;
      const svg=new DOMParser().parseFromString(item.svg,'image/svg+xml').documentElement;
      if(svg.localName!=='svg'||![item.width,item.height].every(n=>Number.isFinite(n)&&n>=0))throw new Error('无效的 Typst SVG');
      normalizedMetrics(item);
      if(record.url)URL.revokeObjectURL(record.url);
      Object.assign(record,item,{status:'ready',mapped:true,url:URL.createObjectURL(new Blob([item.svg],{type:'image/svg+xml'}))});
    }
    for(const record of records)if(record.status==='loading')Object.assign(record,{status:'error',error:'当前文档没有此 Raw 的可见排版结果'});
  } catch(error) {
    for(const record of records)if(record.status!=='ready')Object.assign(record,{status:'error',error:error.message});
  } finally {
    renderBusy=false;
    syncPreviewResults();
    redraw();
    schedulePreviews();
  }
}
window.addEventListener('pagehide',()=>{
  for(const record of previews.values())if(record.url)URL.revokeObjectURL(record.url);
});
function call(action) {
  const bytes = encoder.encode(JSON.stringify(action)), ptr = wasm.alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  const result = wasm.dispatch(ptr, bytes.length);
  const data = JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer, result, wasm.output_len())));
  if (data.error) throw new Error(data.error);
  return data;
}
function send(action, focus = true) {
  if (!wasm) return;
  try {
    const draft=state?.command?.draft;
    if(action.action==='undo' || action.action==='redo') { history(action.action); return; }
    const before=state?.source; state = call(action);
    if(state.source!==before)changed(state);
    if(state.source!==before)macroWarmup?.schedule();
    macroWarmup?.flush();
    updateAttachmentEdits(state,focus || document.activeElement===keyboard);
    if(action.action==='key' && action.key==='Enter' && !state.pending && draft) {
      for(const [key,record] of previews)if(record.status==='error' && record.expression===draft.trim())previews.delete(key);
      for(const [key,record] of attachments)if(record.status==='error')attachments.delete(key);
    }
    render(); if (focus) {keyboard.focus({preventScroll:true});revealActiveCaret();} paintCaret(); scheduleCompletion();
  }
  catch(error) { $('#status').textContent = error.message; $('#status').classList.add('error'); }
}
function element(kind, text) {
  const el = document.createElement('span'); el.className = kind;
  if (text !== undefined) el.textContent = text;
  return el;
}
function draw(node, inText=false) {
  const el = element(node.kind); el.classList.toggle('selected', node.selected);
  if(node.kind==='macro-argument'){el.style.setProperty('--argument-color',['#317bb5','#ae6430','#8b59b0','#288473','#b44970','#767323'][node.columns%6]);el.title=`参数 ${node.text} · 同色框共享内容`;el.setAttribute('aria-label',`参数 ${node.text}`);}
  if(node.kind==='macro' || node.kind==='macro-collapsed')el.title=node.text;
  if(node.kind==='raw') {
    const record=rawReady?previewFor(node):{status:'unavailable'};
    if(record.status==='ready') {
      el.classList.add('rendered');const img=document.createElement('img');img.src=record.url;img.alt=node.text;
      // Mapped SVGs already include Typst's script sizing; do not shrink them
      // a second time by using the attachment's local em.
      const metrics=normalizedMetrics(record);
      img.dataset.baseFontWidth=metrics.width;img.dataset.baseFontHeight=metrics.height;
      img.dataset.environmentFontSizePt=metrics.environment;
      applySvgMetrics(img);img.addEventListener('load',measure,{once:true});el.append(img);
      el.title=`${node.text}\n环境基准字号 ${metrics.environment}pt；宽度/字号 ${metrics.width.toFixed(4)}`;
    } else {
      el.textContent=node.text;el.classList.toggle('render-error',record.status==='error');
      el.title=record.status==='error'?`Typst 未能渲染：${record.error}\n从左侧按 → 或右侧按 ← 可进入源码编辑` :['waiting','loading'].includes(record.status)?'Typst 正在渲染…':node.text;
      if(record.status==='error' && node.edit) {
        const edit=document.createElement('button');edit.type='button';edit.className='edit-source';edit.textContent='编辑';edit.title='编辑保留的命令源码';
        edit.addEventListener('pointerdown',event=>{event.stopPropagation();});
        edit.addEventListener('click',event=>{event.stopPropagation();send({action:'edit_source',cursor:node.edit,source:node.text});});el.append(edit);
      }
    }
    return el;
  }
  if (node.cursor) { el.dataset.cursor = JSON.stringify(node.cursor); if(node.active) el.dataset.active = 'true'; el.setAttribute('aria-hidden','true'); }
  if (['char','symbol','draft-text','draft-placeholder'].includes(node.kind)) {
    const text=!inText && node.kind==='char'?(node.display_glyph ?? mathGlyph(node.text)):node.text;
    el.append(document.createTextNode(text));
  }
  else if(node.kind==='text') { node.children.forEach(child=>el.append(draw(child,true))); }
  else if (node.kind === 'sqrt' || node.kind === 'root') {
    if (node.kind === 'root') { const idx = element('root-index'); idx.append(draw(node.children[1])); el.append(idx); }
    const shell=element('root-body'), radical=document.createElementNS('http://www.w3.org/2000/svg','svg');
    radical.classList.add('radical');radical.setAttribute('viewBox','0 0 24 100');radical.setAttribute('preserveAspectRatio','none');radical.setAttribute('aria-hidden','true');
    const stroke=document.createElementNS('http://www.w3.org/2000/svg','path');
    stroke.setAttribute('d','M 0 62 L 5 56 L 11 94 L 23 0 L 24 0');stroke.setAttribute('vector-effect','non-scaling-stroke');radical.append(stroke);
    const body=element('radicand'); body.append(draw(node.children[0])); shell.append(radical,body);el.append(shell);
  } else if(node.kind === 'script') {
    const record=attachmentFor(node), resolved=record?.status==='ready'?record:null;
    const base=draw(node.children[0]);
    el.classList.add('attachment-layout');base.classList.add('attachment-base');el.append(base);
    // The side stack shares the base's grid row and stretches to its bbox.
    // Keeping an empty opposite slot anchors single attachments to the same
    // top/bottom edges. Centered limits stay in their own rows outside the base.
    const sides=element('attachment-sides');let hasSide=false;
    for(const [index,place,side] of [[1,resolved?.upper,'upper'],[2,resolved?.lower,'lower']]) {
      if(node.children[index].kind==='absent') { sides.append(element('absent'));continue; }
      const centered=place==='limits';
      const slot=element(`attachment-slot ${side} ${centered?'center':'side'}`);
      slot.append(draw(node.children[index]));
      if(centered) { el.append(slot);sides.append(element('absent')); }
      else { sides.append(slot);hasSide=true; }
    }
    if(hasSide)el.append(sides);
    if(record?.status==='error')el.title=`Typst 附件布局暂不可用，保留右侧槽位：${record.error}`;
    else if(record?.status==='waiting')el.title='Typst 正在解析附件位置…';
  } else if(node.kind === 'delim') {
    const [left,right] = node.text.split('\n'); el.append(element('delimiter',left),draw(node.children[0]),element('delimiter',right));
  } else if(node.kind === 'grid') {
    el.style.setProperty('--columns',node.columns); const cells=element('grid-cells'); node.children.forEach(c=>cells.append(draw(c))); el.append(element('matrix-bracket','('),cells,element('matrix-bracket',')'));
  } else if(node.kind === 'aligned') {
    el.style.setProperty('--columns',node.columns);
    node.children.forEach((child,i)=>{
      const slot=draw(child);const column=i%node.columns;
      slot.classList.add('alignment-slot');
      slot.dataset.align=node.columns===1?'center':column%2===0?'right':'left';
      if(column>0 && column%2===0)slot.classList.add('alignment-pair');
      el.append(slot);
    });
  } else {
    if(node.kind === 'decoration') el.dataset.decoration=node.text;
    node.children.forEach(child=>el.append(draw(child,inText)));
  }
  return el;
}
function render() {
  schedulePreviews();
  canvas.replaceChildren(draw(state.view));
  paintAttachmentStatus();
  $('#status').classList.remove('error');
  $('#status').textContent=state.message || (state.string_mode?'字符模式 · Enter 或 " 结束':state.pending?'输入命令 · Enter 确认，Esc 取消':'就绪 · 空格退出结构，Tab 切换槽位');
  $('#position').textContent=`${state.cursor.slices.length} 层 · 位置 ${state.cursor.pos}`;
  popup.replaceChildren(); popup.hidden=!state.pending || !state.candidates.length;
  state.candidates.forEach((name,i)=>{
    const button=document.createElement('button');button.type='button';button.textContent=name;button.title='填入候选，按 Enter 确认';button.setAttribute('role','option');button.setAttribute('aria-selected',String(i===state.completion_index));
    if(i===state.completion_index)button.className='chosen';
    button.addEventListener('pointerdown',event=>{event.preventDefault();send({action:'complete',name});});popup.append(button);
  });
  measure();
  scheduleAttachments();
}
function measure() {
  if(!wasm || !state?.active_range)return;
  const stops=[...canvas.querySelectorAll('[data-cursor]')].map(el=>{const r=el.getBoundingClientRect();return {cursor:JSON.parse(el.dataset.cursor),x:r.x,y:(r.top+r.bottom)/2};});
  call({action:'geometry',stops});
  paintCaret();
}
function activeCaretElement() {
  const stop=canvas.querySelector('[data-active]');
  return state?.pending ? stop?.previousElementSibling?.querySelector('.draft-caret') : stop;
}
function revealActiveCaret(){
  const active=activeCaretElement();if(!active)return;
  if(revealInFormula(canvas.closest('.formula-widget'),active.getBoundingClientRect()))measure();
}
function paintCaret() {
  const active=activeCaretElement();
  const rect=active?.getBoundingClientRect();
  const r=rect?visibleCaret({left:rect.left,right:rect.right,top:rect.top,bottom:rect.bottom},canvas.closest('.formula-widget')):null;
  const focused=document.activeElement===keyboard;
  caret.hidden=!r || !focused || !!state?.selected_source;
  popup.hidden=!r || !focused || !state?.pending || !state.candidates.length;
  if(!r)return;
  caret.style.cssText=`left:${r.left}px;top:${r.top}px;height:${Math.max(1,r.bottom-r.top)}px`;
  keyboard.style.left=r.left+'px';keyboard.style.top=r.bottom+'px';
  if(!popup.hidden){const width=220;popup.style.left=Math.max(8,Math.min(r.left,innerWidth-width-8))+'px';popup.style.top=Math.min(r.bottom+8,innerHeight-220)+'px';popup.querySelector('.chosen')?.scrollIntoView({block:'nearest'});}
}
canvas.addEventListener('pointerdown',event=>{
  event.preventDefault();const cell=event.target.closest('.cell,.empty-cell');
  let stops=[...canvas.querySelectorAll('[data-cursor]')];
  if(cell){const local=stops.filter(s=>s.parentElement===cell);if(local.length)stops=local;}
  let best;
  for(const el of stops){const r=el.getBoundingClientRect(),distance=Math.abs(r.x-event.clientX)+3*Math.abs((r.top+r.bottom)/2-event.clientY);if(!best||distance<best.distance)best={el,distance};}
  if(best)send({action:'click',cursor:JSON.parse(best.el.dataset.cursor),shift:event.shiftKey});
});
keyboard.addEventListener('keydown',event=>{
  if(event.isComposing || composing)return;
  const key=event.key,ctrl=event.ctrlKey||event.metaKey;
  if(key==='Escape' && !state.pending && !state.selected_source && !state.cursor.slices.length) { event.preventDefault(); exit(); return; }
  if(ctrl && key==='Enter') { event.preventDefault(); exit(); return; }
  if(ctrl && ['c','v','x'].includes(key.toLowerCase()))return;
  if(ctrl && key.toLowerCase()==='z'){event.preventDefault();send({action:event.shiftKey?'redo':'undo'});return;}
  if(ctrl && key.toLowerCase()==='y'){event.preventDefault();send({action:'redo'});return;}
  // Only the combinations the structural editor implements are captured here.
  // Everything else (Ctrl+F, Ctrl+P, Ctrl+S, reload, …) stays with the browser.
  if(ctrl && !['a',' ','arrowleft','arrowright'].includes(key.toLowerCase()))return;
  if(['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End','Backspace','Delete','Escape','Tab','Enter',' '].includes(key)||ctrl){event.preventDefault();send({action:'key',key,shift:event.shiftKey,ctrl});}
});
keyboard.addEventListener('beforeinput',event=>{
  if(composing || event.isComposing)return;
  event.preventDefault();
  if(event.inputType==='insertFromComposition'||suppressComposition)return;
  if(event.inputType==='insertText'&&event.data)send({action:'input',text:event.data});
  keyboard.value='';
});
keyboard.addEventListener('compositionstart',()=>{composing=true;});
keyboard.addEventListener('blur',()=>{updateAttachmentEdits(state,false);redraw();});
keyboard.addEventListener('focus',()=>updateAttachmentEdits(state));
keyboard.addEventListener('compositionend',event=>{composing=false;keyboard.value='';if(event.data)send({action:'input',text:event.data});suppressComposition=true;queueMicrotask(()=>{suppressComposition=false;});});
keyboard.addEventListener('paste',event=>{event.preventDefault();send({action:'paste',text:event.clipboardData.getData('text/plain')});});
keyboard.addEventListener('copy',event=>{if(state?.selected_source){event.preventDefault();event.clipboardData.setData('text/plain',state.selected_source);}});
keyboard.addEventListener('cut',event=>{if(state?.selected_source){event.preventDefault();event.clipboardData.setData('text/plain',state.selected_source);send({action:'key',key:'Backspace'});}});

export async function initMath(callbacks) {
  changed=callbacks.changed; exit=callbacks.exit; history=callbacks.history; documentPath=callbacks.path; beforeRequest=callbacks.beforeRequest||beforeRequest;
  const {instance}=await WebAssembly.instantiateStreaming(fetch(assetUrl('core.wasm')),{}); wasm=instance.exports;
  state=call({action:'state'});
  const result=await api('/api/status').catch(error=>({available:false,attachments:false,error:error.message}));
  serviceReady=result.available; attachmentReady=result.attachments; rawReady=result.attachments;
  if(rawReady&&!inVSCode)macroWarmup=new MacroWarmup({
    request:body=>api('/api/prewarm',body),
    snapshot:()=>callbacks.warmupSnapshot?.()||({path:documentPath(),source:state.source}),
    apply:results=>{if(state.pending)return false;state=call({action:'macro_warmup',results});previewSession++;callbacks.projectionChanged?.(state);return true;},
    updated:()=>{callbacks.projectionChanged?.(state);redraw();},
    onError:error=>serviceLabel('宏预热失败',error.message)
  });
  serviceLabel(serviceReady?'Tinymist':'Tinymist 未找到',result.error||'');
  document.fonts.ready.then(measure);
  window.addEventListener('resize',measure);document.addEventListener('scroll',event=>{if(event.target===canvas.closest('.formula-widget'))measure();else paintCaret();},true);document.addEventListener('focusin',paintCaret);
}
export function resizeMathSvg(){document.querySelectorAll('img[data-base-font-width]').forEach(applySvgMetrics);measure();}
export function syncSource(source) { const path=documentPath(),reset_warmups=warmupPath!==path;warmupPath=path;previewSession++;state=call({action:'set_source',source,reset_warmups});macroWarmup?.schedule();return state; }
export function disposeMath(){macroWarmup?.dispose();clearTimeout(renderTimer);clearTimeout(attachmentTimer);clearTimeout(completionTimer);}
export function refreshMacroWarmup(){if(warmupPath!==documentPath()){state=call({action:'macro_warmup',clear:true,results:[]});warmupPath=documentPath();}macroWarmup?.schedule();}
export function enterMath(start) { state=call({action:'activate_formula',start});previewSession++;render();return state; }
export function leaveMath() { state=call({action:'deactivate_formula'});popup.hidden=true;caret.hidden=true;return state; }
export function focusMath() { keyboard.focus({preventScroll:true});requestAnimationFrame(()=>{revealActiveCaret();measure();}); }
export function enterMathFromArrow(key,x=0) {
  const stops=[...canvas.querySelectorAll('[data-cursor]')].map(el=>{const r=el.getBoundingClientRect();return {cursor:JSON.parse(el.dataset.cursor),x:r.left,y:(r.top+r.bottom)/2};});
  const stop=slotForEntry(stops,key,x);if(stop)send({action:'click',cursor:stop.cursor});else focusMath();
}
export function mathState() { return state; }
export function mathAction(action) { send(action); }
export function passiveMath() {
  const clone=canvas.cloneNode(true);clone.removeAttribute('id');clone.className='passive-math';
  for(const el of clone.querySelectorAll('[data-cursor],[data-active]')) {el.removeAttribute('data-cursor');el.removeAttribute('data-active');}
  clone.querySelectorAll('button').forEach(el=>el.remove());return clone;
}
export { refreshAllSvg, measure as measureMath };
