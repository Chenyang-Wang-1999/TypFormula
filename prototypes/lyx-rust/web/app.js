// SPDX-License-Identifier: GPL-2.0-or-later
// Painter/event bridge only. Rust/WASM owns MathData, all commands and the cursor.
import { mathGlyph } from './math-font.js';
const $ = selector => document.querySelector(selector);
const keyboard = $('#keyboard'), canvas = $('#canvas'), caret = $('#caret'), popup = $('#completions');
const encoder = new TextEncoder(), decoder = new TextDecoder();
let wasm, state, composing = false, suppressComposition = false;
let completionTimer, completionBusy = false, requestedCommand = '', serviceReady = false;
const previews = new Map(), previewQueue = [];
let previewBusy = 0, redrawQueued = false;
let attachmentReady = false, attachmentBusy = false, attachmentTimer;
const attachments = new Map();
function attachmentKey(expression) { return JSON.stringify([state.definitions,state.display,expression]); }
function attachmentFor(node) {
  if(!attachmentReady || !node.attachment)return null;
  const key=attachmentKey(node.attachment);
  if(!attachments.has(key))attachments.set(key,{status:'waiting'});
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
  if(attachmentBusy)return;
  const candidates=[];
  function visit(node) { if(node.attachment)candidates.push(node.attachment);node.children.forEach(visit); }
  visit(state.view);
  const expression=candidates.find(e=>attachments.get(attachmentKey(e))?.status==='waiting');
  if(expression===undefined)return;
  const key=attachmentKey(expression), record=attachments.get(key);
  const request={expression,definitions:state.definitions,display:state.display};
  attachmentBusy=true;
  try {
    const result=await api('/api/attachments',request);
    if(![null,'limits','scripts'].includes(result.upper)||![null,'limits','scripts'].includes(result.lower))throw new Error('无效的 Typst 附件位置');
    if(result.base) {
      const base=result.base;
      if(!Number.isFinite(base.width)||!Number.isFinite(base.height)||base.width<0||base.height<0)throw new Error('无效的伸展字形尺寸');
      if(new DOMParser().parseFromString(base.svg,'image/svg+xml').documentElement.localName!=='svg')throw new Error('无效的伸展 SVG');
      base.url=URL.createObjectURL(new Blob([base.svg],{type:'image/svg+xml'}));base.status='ready';
    }
    Object.assign(record,{status:'ready',...result});
  } catch(error) { Object.assign(record,{status:'error',error:error.message}); }
  finally { attachmentBusy=false;redraw(); }
}
async function api(path, body) {
  const response = await fetch(path, body === undefined ? {} : {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(16000)});
  const result = await response.json();
  if(!response.ok || result.error)throw new Error(result.error || `HTTP ${response.status}`);
  return result;
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
function previewFor(node) {
  const definitions=node.definitions ?? state.definitions;
  const display=state.display, key=JSON.stringify([definitions,display,node.text]);
  if(!previews.has(key)) {
    const record={status:'waiting',expression:node.text};previews.set(key,record);
    previewQueue.push({key,expression:node.text,definitions,display,record});queueMicrotask(runPreviews);
  }
  return previews.get(key);
}
function runPreviews() {
  while(serviceReady && previewBusy<2 && previewQueue.length) {
    const task=previewQueue.shift();previewBusy++;
    api('/api/render',{expression:task.expression,definitions:task.definitions,display:task.display}).then(result=>{
      const doc=new DOMParser().parseFromString(result.svg,'image/svg+xml');
      const svg=doc.documentElement;
      if(svg.localName!=='svg')throw new Error('无效的 SVG');
      const box=svg.getAttribute('viewBox')?.split(/[ ,]+/).map(Number);
      const width=box?.[2] ?? parseFloat(svg.getAttribute('width'));
      const height=box?.[3] ?? parseFloat(svg.getAttribute('height'));
      if(!Number.isFinite(width)||!Number.isFinite(height)||width<0||height<0)throw new Error('SVG 尺寸无效');
      Object.assign(task.record,{status:'ready',url:URL.createObjectURL(new Blob([result.svg],{type:'image/svg+xml'})),width,height});
    }).catch(error=>{Object.assign(task.record,{status:'error',error:error.message});})
      .finally(()=>{
        state=call({action:'preview_result',source:task.expression,definitions:task.definitions,display:task.display,failed:task.record.status==='error'});
        previewBusy--;redraw();runPreviews();
      });
  }
}
window.addEventListener('pagehide',()=>{
  for(const record of previews.values())if(record.url)URL.revokeObjectURL(record.url);
  for(const record of attachments.values())if(record.base?.url)URL.revokeObjectURL(record.base.url);
});
const examples = {
  blank: '$ "" $',
  mixed: '$ frac(dif x, 2 pi) $',
  matrix: '$ mat(a_1, b^2; sqrt(x), frac(1, y)) $',
  fallback: '$ cancel(x) + cases(x, y) + arrow.r.double $',
  macros: '#let pd(f, x) = $frac(partial #f, partial #x)$\n#let jac(f1, f2, x1, x2) = $mat(pd(#f1, #x1), pd(#f1, #x2); pd(#f2, #x1), pd(#f2, #x2))$\n#let marked(x) = $cancel(#x)$\n$ jac(f, g, x, y) + marked(z) $',
  attachments: '$ sum_(i=0)^n + stretch(arrow.r)^("long label") $',
};
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
    state = call(action);
    if(action.action==='key' && action.key==='Enter' && !state.pending && draft) {
      for(const [key,record] of previews)if(record.status==='error' && record.expression===draft.trim())previews.delete(key);
      for(const [key,record] of attachments)if(record.status==='error')attachments.delete(key);
    }
    render(); if (focus) keyboard.focus({preventScroll:true}); paintCaret(); scheduleCompletion();
  }
  catch(error) { $('#status').textContent = error.message; $('#status').classList.add('error'); }
}
function element(kind, text) {
  const el = document.createElement('span'); el.className = kind;
  if (text !== undefined) el.textContent = text;
  return el;
}
function draw(node, inText=false, basePreview=null) {
  const el = element(node.kind); el.classList.toggle('selected', node.selected);
  if(node.kind==='macro-argument'){el.style.setProperty('--argument-color',['#317bb5','#ae6430','#8b59b0','#288473','#b44970','#767323'][node.columns%6]);el.title=`参数 ${node.text} · 同色框共享内容`;el.setAttribute('aria-label',`参数 ${node.text}`);}
  if(node.kind==='macro' || node.kind==='macro-collapsed')el.title=node.text;
  if(node.kind==='raw' || (basePreview && ['char','symbol'].includes(node.kind))) {
    const record=basePreview || (serviceReady?previewFor(node):{status:'unavailable'});
    if(record.status==='ready') {
      el.classList.add('rendered');const img=document.createElement('img');img.src=record.url;img.alt=node.text;
      img.style.width=(record.width/24)+'em';img.style.height=(record.height/24)+'em';img.addEventListener('load',measure,{once:true});el.append(img);
      el.title=`${node.text}\nTypst 渲染的整体公式，可整块删除`;
    } else {
      el.textContent=node.text;el.classList.toggle('render-error',record.status==='error');
      el.title=record.status==='error'?`Typst 未能渲染：${record.error}\n从左侧按 → 或右侧按 ← 可进入源码编辑` :record.status==='waiting'?'Typst 正在渲染…':node.text;
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
    // Replace only an atomic base's image. All its cursor stops and the
    // original source stay in the Rust tree; structured bases remain editable.
    const atoms=node.children[0].children.filter(n=>n.kind!=='stop');
    const base=draw(node.children[0],false,atoms.length===1&&['raw','char','symbol'].includes(atoms[0].kind)?resolved?.base:null);
    if(resolved && (resolved.upper==='limits'||resolved.lower==='limits')) {
      el.classList.add('attachment-layout');base.classList.add('attachment-base');el.append(base);
      for(const [index,place,side] of [[1,resolved.upper,'upper'],[2,resolved.lower,'lower']]) {
        if(node.children[index].kind==='absent')continue;
        const slot=element(`attachment-slot ${side} ${place==='limits'?'center':'side'}`);
        slot.append(draw(node.children[index]));el.append(slot);
      }
    } else {
      el.append(base);const stack=element('scripts');stack.append(draw(node.children[1]),draw(node.children[2]));el.append(stack);
    }
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
    node.children.forEach(child=>el.append(draw(child,inText,basePreview)));
  }
  return el;
}
function render() {
  renderMacros();
  canvas.replaceChildren(draw(state.view));
  paintAttachmentStatus();
  $('#source').value = state.source;
  $('#undo').disabled=!state.undo; $('#redo').disabled=!state.redo;
  $('#display-mode').textContent=state.display?'行间公式':'行内公式';
  $('#display-mode').setAttribute('aria-pressed',String(state.display));
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
  if(!wasm || !state)return;
  const stops=[...canvas.querySelectorAll('[data-cursor]')].map(el=>{const r=el.getBoundingClientRect();return {cursor:JSON.parse(el.dataset.cursor),x:r.x,y:(r.top+r.bottom)/2};});
  call({action:'geometry',stops});
  paintCaret();
}
function paintCaret() {
  const stop=canvas.querySelector('[data-active]');
  const active=state?.pending ? stop?.previousElementSibling?.querySelector('.draft-caret') : stop;
  caret.hidden=!active || document.activeElement!==keyboard || !!state?.selected_source;
  if(!active)return;
  const r=active.getBoundingClientRect();
  caret.style.cssText=`left:${r.x}px;top:${r.y}px;height:${Math.max(16,r.height)}px`;
  keyboard.style.left=r.x+'px';keyboard.style.top=r.bottom+'px';
  if(!popup.hidden){const width=220;popup.style.left=Math.max(8,Math.min(r.x,innerWidth-width-8))+'px';popup.style.top=Math.min(r.bottom+8,innerHeight-220)+'px';popup.querySelector('.chosen')?.scrollIntoView({block:'nearest'});}
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
  if(ctrl && ['c','v','x'].includes(key.toLowerCase()))return;
  if(ctrl && key.toLowerCase()==='z'){event.preventDefault();send({action:event.shiftKey?'redo':'undo'});return;}
  if(ctrl && key.toLowerCase()==='y'){event.preventDefault();send({action:'redo'});return;}
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
keyboard.addEventListener('compositionend',event=>{composing=false;keyboard.value='';if(event.data)send({action:'input',text:event.data});suppressComposition=true;queueMicrotask(()=>{suppressComposition=false;});});
keyboard.addEventListener('paste',event=>{event.preventDefault();send({action:'paste',text:event.clipboardData.getData('text/plain')});});
keyboard.addEventListener('copy',event=>{if(state?.selected_source){event.preventDefault();event.clipboardData.setData('text/plain',state.selected_source);}});
keyboard.addEventListener('cut',event=>{if(state?.selected_source){event.preventDefault();event.clipboardData.setData('text/plain',state.selected_source);send({action:'key',key:'Backspace'});}});
let macroBase = null, macroDirty = false, macroPrefix = '';
function markMacrosDirty() {
  macroDirty = true;
  $('#macro-status').textContent = '有未应用的修改';
  $('#macro-status').classList.remove('error');
  $('#macro-apply').disabled = false;
}
function appendMacroRow(source='', definition=null) {
  const row=document.createElement('div');row.className='macro-row';
  const heading=document.createElement('div');heading.className='macro-row-heading';
  const label=document.createElement('span');
  label.textContent=definition?`${definition.name} · ${definition.expandable?'可展':'不可展'}${definition.shadowed?'（已被后续同名定义覆盖）':''} · ${definition.reason}`:'新定义 · 待应用';
  label.className=definition?.expandable?'expandable':'';
  const remove=document.createElement('button');remove.type='button';remove.textContent='删除';remove.setAttribute('aria-label',`删除 ${definition?.name||'新定义'}`);
  const input=document.createElement('textarea');input.value=source.trim();input.spellcheck=false;input.setAttribute('aria-label',`${definition?.name||'新定义'} 的 Typst 宏源码`);input.placeholder='#let ratio(x, y) = $frac(#x, #x + #y)$';
  input.oninput=()=>{label.textContent='待应用 · 应用后重新分析';label.className='';markMacrosDirty();};
  input.onkeydown=event=>{if((event.ctrlKey||event.metaKey)&&event.key==='Enter'){event.preventDefault();$('#macro-apply').click();}};
  remove.onclick=()=>{row.remove();markMacrosDirty();};
  heading.append(label,remove);row.append(heading,input);$('#macro-list').append(row);
  return input;
}
function renderMacros() {
  if(macroBase!==state.definitions && !macroDirty) {
    macroBase=state.definitions;macroPrefix=state.macros.length?'':state.definitions;
    $('#macro-list').replaceChildren();state.macros.forEach(d=>appendMacroRow(d.source,d));
    $('#macro-status').textContent=state.macros.length?`${state.macros.filter(d=>d.expandable).length} / ${state.macros.length} 条定义可展`:'还没有宏定义，可添加源码或载入示例。';
    $('#macro-status').classList.remove('error');
  }
  const conflict=macroDirty && macroBase!==state.definitions;
  $('#macro-apply').disabled=!macroDirty||conflict;
  if(conflict){$('#macro-status').textContent='公式定义已由导入或撤销改变；请先复制未应用的源码，再还原列表。';$('#macro-status').classList.add('error');}
}
$('#macro-add').onclick=()=>{const input=appendMacroRow();markMacrosDirty();input.focus();};
$('#macro-reset').onclick=()=>{macroDirty=false;macroBase=null;renderMacros();};
$('#macro-apply').onclick=()=>{
  try {
    const sources=[...document.querySelectorAll('#macro-list textarea')].map(el=>el.value.trim()).filter(Boolean);
    state=call({action:'set_definitions',definitions:[macroPrefix,...sources].filter(Boolean).join('\n')});
    macroDirty=false;macroBase=null;render();paintCaret();scheduleCompletion();
  } catch(error){$('#macro-status').textContent=error.message;$('#macro-status').classList.add('error');}
};
$('#macro-example').onclick=()=>{macroDirty=false;macroBase=null;send({action:'import',source:examples.macros});};
$('#undo').onclick=()=>send({action:'undo'});$('#redo').onclick=()=>send({action:'redo'});$('#clear').onclick=()=>send({action:'clear'});
$('#display-mode').onclick=()=>send({action:'set_display',display:!state.display});
$('#row').onclick=()=>send({action:'add_row'});$('#column').onclick=()=>send({action:'add_column'});
document.querySelectorAll('[data-example]').forEach(button=>button.onclick=()=>send({action:'import',source:examples[button.dataset.example]}));
$('#import').onclick=()=>{$('#import-source').value=state.source;$('#import-error').textContent='';$('#import-dialog').showModal();$('#import-source').focus();};
$('#import-form').onsubmit=event=>{event.preventDefault();try{state=call({action:'import',source:$('#import-source').value});$('#import-dialog').close();render();keyboard.focus();paintCaret();scheduleCompletion();}catch(error){$('#import-error').textContent=error.message;}};
$('#help').onclick=()=>$('#help-dialog').showModal();document.querySelectorAll('[data-close]').forEach(button=>button.onclick=()=>{$('#'+button.dataset.close).close();keyboard.focus();paintCaret();});
$('#copy').onclick=async()=>{try{await navigator.clipboard.writeText(state.source);$('#status').textContent='已复制 Typst 源码';}catch{const source=$('#source');source.focus();source.select();$('#status').textContent='请按 Ctrl+C 复制';}};
$('#download').onclick=()=>{const url=URL.createObjectURL(new Blob([state.source],{type:'text/plain;charset=utf-8'}));const a=document.createElement('a');a.href=url;a.download='formula.typ';a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
window.addEventListener('resize',measure);document.addEventListener('scroll',()=>{paintCaret();},true);document.addEventListener('focusin',paintCaret);
try {
  const {instance}=await WebAssembly.instantiateStreaming(fetch('/core.wasm'),{});wasm=instance.exports;
  send({action:'state'}); document.fonts.ready.then(measure);
  api('/api/status').then(result=>{
    serviceReady=result.available;attachmentReady=result.attachments;
    serviceLabel(serviceReady?'LSP 待连接':'内置补全',result.error||'');
    $('#display-mode').title=attachmentReady?'切换公式样式；顶层上下标由 Typst 决定位置':'切换公式样式；limits/stretch 需要运行 build-native.cmd 并重启服务';
    render();scheduleCompletion();
  }).catch(error=>serviceLabel('请重启 start.cmd',error.message));
} catch(error) { $('#status').textContent='加载失败：'+error.message; }
