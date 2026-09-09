import {FormulaEditor, byteLength, sliceBytes, mapOffset} from './editor.js';
const page = document.querySelector('#document-page');
const sourceBox = document.querySelector('#document-source');
const contextBox = document.querySelector('#formula-context');
const status = document.querySelector('#document-status');
const preview = document.querySelector('#document-preview');
const engineStatus = document.querySelector('#engine-status');
let model, activeId = null, caret = {head: 0, anchor: 0};
let chain = Promise.resolve(), pending = 0, composing = false, rendering = false;
let renderedGeneration = null, sourceDraftRevision = null, svgGeneration = null;
let pageUrls = [], pollBusy = false;
const views = new Map(), macros = new Map();
let svgEntries = new Map();

async function api(path, body) {
  const response = await fetch(path, body === undefined ? {cache:'no-store'} : {
    method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(body),
  });
  const result = await response.json();
  if (!response.ok || result.error) throw new Error(result.error || 'HTTP ' + response.status);
  return result;
}
function message(value, error = false) { status.textContent = value; status.classList.toggle('error', error); }
function enqueue(action) {
  pending++;
  chain = chain.then(action).catch(error => message(error.message, true)).finally(() => { pending--; });
  return chain;
}
const text = (start, end) => sliceBytes(model.source, start, end);
const bounds = formula => formula.projection.range;
const active = () => views.get(activeId);
const insideEditor = event => event.target.closest?.('.formula-editor, .macro-widget, .formula-repair');
function rangeAttributes(element, start, end) { element.dataset.start = start; element.dataset.end = end; }

function bytePosition(node, offset) {
  if (node === page) return offset < page.childNodes.length
    ? Number(page.childNodes[offset].dataset.start) : byteLength(model.source);
  const element = node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement;
  const run = element?.closest('.text-run');
  if (!run || !page.contains(run)) return null;
  const range = document.createRange(); range.selectNodeContents(run);
  try { range.setEnd(node, offset); } catch { return null; }
  return Number(run.dataset.start) + byteLength(range.toString());
}
function readCaret() {
  if (!model || rendering || composing || document.activeElement !== page) return;
  const selection = getSelection();
  if (!selection.rangeCount) return;
  const head = bytePosition(selection.focusNode, selection.focusOffset);
  const anchor = bytePosition(selection.anchorNode, selection.anchorOffset);
  if (head !== null && anchor !== null) caret = {head, anchor};
}
document.addEventListener('selectionchange', readCaret);
page.addEventListener('focusin', event => {
  if (event.target === page || event.target.closest('.macro-widget')) {
    activeId = null; contextBox.value = '';
  }
});
function placeCaret(position) {
  position = Math.max(0, Math.min(position, byteLength(model.source)));
  activeId = null; contextBox.value = '';
  caret = {head: position, anchor: position};
  page.focus({preventScroll:true});
  const range = document.createRange();
  let placed = false;
  for (const element of page.childNodes) {
    const start = Number(element.dataset.start), end = Number(element.dataset.end);
    if (element.classList.contains('text-run') && position >= start && position <= end) {
      const length = text(start, position).length;
      if (!element.firstChild) element.append(document.createTextNode(''));
      range.setStart(element.firstChild, Math.min(length, element.firstChild.length));
      placed = true; break;
    }
    if (position <= start) { range.setStartBefore(element); placed = true; break; }
  }
  if (!placed) { range.selectNodeContents(page); range.collapse(false); }
  range.collapse(true);
  const selection = getSelection(); selection.removeAllRanges(); selection.addRange(range);
  for (const view of views.values()) view.editor?.paintCaret();
}
function focusSnapshot() {
  const element = document.activeElement;
  return {element, start:element?.selectionStart, end:element?.selectionEnd,
    page:element === page, view:activeId};
}
function restoreFocus(saved) {
  if (saved.page) { placeCaret(caret.head); return; }
  if (saved.element?.isConnected) {
    saved.element.focus({preventScroll:true});
    if (saved.start !== undefined && saved.element.type !== undefined) {
      // The hidden keyboard always has an empty value; other draft inputs persist.
      try { saved.element.setSelectionRange(saved.start, saved.end); } catch {}
    }
    active()?.editor?.paintCaret(); return;
  }
  const view = views.get(saved.view);
  if (view?.mode === 'display') focusView(view);
}
function focusView(view, edge = null) {
  activeId = view.id; showContext(view.formula);
  if (!view.formula.valid) { view.repair.focus({preventScroll:true}); return; }
  if (view.editor.rawDraft) view.editor.rawDraft.input.focus({preventScroll:true});
  else if (view.editor.command) view.editor.command.focus({preventScroll:true});
  else view.editor.focus(edge);
}
async function edit(start, end, value, position = start + byteLength(value), owner = null) {
  // A draft elsewhere is not a document-wide edit lock. Protect it only if this
  // source transaction would overwrite that very draft's range.
  for (const row of macros.values()) {
    if (row.input.value !== row.original && start < row.end && end > row.start &&
        !(start === row.start && end === row.end)) throw new Error('这次修改会覆盖未提交的宏草稿，请先保存该草稿');
  }
  const result = await api('/api/edit', {revision:model.revision, start, end, text:value});
  const saved = focusSnapshot();
  const length = byteLength(value);
  caret.head = mapOffset(caret.head, start, end, length);
  caret.anchor = mapOffset(caret.anchor, start, end, length);
  for (const view of views.values()) view.editor?.mapEdit(start, end, length);
  if (owner?.editor) owner.editor.head = owner.editor.anchor = position;
  else if (saved.page) caret = {head:position, anchor:position};
  model = result; renderDocument(); restoreFocus(saved);
  return result;
}
function macroTemplate(view, name) {
  let env = view.formula.environment;
  while (env !== undefined && env !== null) {
    const entry = model.macros.environments[env];
    if (!entry || entry.opaque) return null;
    if (entry.binding !== null) {
      const definition = model.macros.definitions[entry.binding];
      if (definition.names.includes(name)) {
        return definition.function ? name + '(' + definition.parameters.map(() => '""').join(', ') + ')' : name;
      }
    }
    env = entry.parent;
  }
  return null;
}
function showContext(formula) {
  const visible = [], seen = new Set();
  let env = formula.environment;
  while (env !== undefined && env !== null) {
    const entry = model.macros.environments[env];
    if (!entry || entry.opaque) break;
    if (entry.binding !== null) {
      const definition = model.macros.definitions[entry.binding];
      if (definition.names.some(name => !seen.has(name))) visible.push(text(definition.range.start, definition.range.end));
      definition.names.forEach(name => seen.add(name));
    }
    env = entry.parent;
  }
  contextBox.value = visible.reverse().join('\n');
}
async function refreshView(view) {
  if (!views.has(view.id)) return;
  await api('/api/refresh-svg', {revision:model.revision, formulas:[view.formula.node]});
  await loadSvg();
}
async function switchPreview(view) {
  if (!view.formula.valid) throw new Error('当前公式源码尚未完成；可以继续编辑其他区域，修复后再预览');
  if (view.editor?.rawDraft || view.editor?.command) throw new Error('请先完成当前公式内的源码或命令草稿');
  view.mode = 'preview'; activeId = null; contextBox.value = '';
  const end = bounds(view.formula).end;
  renderView(view); placeCaret(end);
  // If the current compile failed, the old image stays visible with its status.
  await refreshView(view);
}
function makeView(formula) {
  const view = {id:formula.projection.cache_id, formula, mode:'preview', editor:null,
    widget:document.createElement('span'), urls:[], painted:null, repair:null};
  view.widget.contentEditable = 'false';
  view.widget.addEventListener('pointerdown', event => {
    if (view.mode !== 'display' || event.target.closest('.formula-editor, .formula-repair')) return;
    event.preventDefault(); event.stopPropagation(); focusView(view);
    if (view.formula.valid) view.editor.enterAt(event.clientX,event.clientY);
  });
  view.widget.addEventListener('click', event => {
    if (view.mode !== 'preview') return;
    event.preventDefault(); event.stopPropagation();
    enqueue(async () => {
      view.mode = 'display'; renderView(view); focusView(view, 'start');
      await refreshView(view);
    });
  });
  return view;
}
function ensureEditor(view) {
  if (view.editor) return;
  view.editor = new FormulaEditor({
    enqueue, text,
    edit: (a,b,value,position) => edit(a,b,value,position,view),
    isFocused: () => activeId === view.id,
    activate: () => { activeId = view.id; showContext(view.formula); },
    macroTemplate: name => macroTemplate(view, name),
    refresh: () => refreshView(view),
    commit: () => switchPreview(view),
    exit: (position, direction) => {
      const stop = view.editor.currentStop()?.element.getBoundingClientRect();
      placeCaret(position);
      if (stop && ['ArrowUp','ArrowDown'].includes(direction)) {
        const down = direction === 'ArrowDown';
        const adjacent = [...views.values()].filter(v => v !== view && v.mode === 'display')
          .map(v => ({v,r:v.widget.getBoundingClientRect()}))
          .filter(({r}) => down ? r.top > stop.bottom : r.bottom < stop.top)
          .sort((a,b) => Math.abs(a.r.top-stop.top)-Math.abs(b.r.top-stop.top))[0];
        if (adjacent && Math.abs(adjacent.r.top-stop.top) < 70) {
          focusView(adjacent.v);
          if (adjacent.v.formula.valid) adjacent.v.editor.enterAt(stop.left, down ? adjacent.r.top : adjacent.r.bottom);
        }
      }
    },
  });
}
function repairView(view) {
  if (!view.repair) {
    const input = document.createElement('textarea'); input.className = 'raw-input formula-repair';
    input.setAttribute('aria-label', '未完成公式的源码草稿');
    view.repair = input; view.repairOriginal = '';
    input.addEventListener('focus', () => { activeId = view.id; showContext(view.formula); });
    input.addEventListener('keydown', event => {
      event.stopPropagation();
      if (event.key === 'Escape') { event.preventDefault(); placeCaret(bounds(view.formula).end); }
      if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        enqueue(async () => {
          const range = bounds(view.formula), value = input.value;
          await edit(range.start, range.end, value, range.start + byteLength(value), view);
          if (view.formula.valid && activeId === view.id) focusView(view);
        });
      }
    });
  }
  const range = bounds(view.formula), spelling = text(range.start, range.end);
  if (view.repair.value === view.repairOriginal || !view.repairOriginal) view.repair.value = spelling;
  view.repairOriginal = spelling;
  view.repair.rows = Math.max(1, view.repair.value.split('\n').length);
  view.widget.replaceChildren(view.repair);
}
function renderView(view) {
  const formula = view.formula, range = bounds(formula), widget = view.widget;
  rangeAttributes(widget, range.start, range.end); widget.dataset.formula = view.id;
  widget.className = 'formula-widget ' + (formula.display ? 'display-widget' : 'inline-widget') +
    (view.mode === 'display' ? ' editing-widget' : ' preview-widget');
  if (view.mode === 'display') {
    if (!formula.valid) { repairView(view); return; }
    ensureEditor(view);
    view.editor.update(formula, view.editor.formula !== undefined);
    widget.replaceChildren(view.editor.root); view.editor.applySvg(svgEntries);
  } else {
    const entry = svgEntries.get(view.id + ':' + view.id);
    paintPreview(view, entry);
  }
}
function paintPreview(view, entry) {
  const signature = entry?.cached ? 'svg:' + entry.cached.render_generation : 'source:' + model.revision;
  if (view.painted !== signature || !view.widget.querySelector('.formula-image, .preview-pending')) {
    view.urls.forEach(url => URL.revokeObjectURL(url)); view.urls = [];
    const children = [];
    if (entry?.cached) {
      for (const part of entry.cached.fragments) {
        const image = document.createElement('img');
        const url = URL.createObjectURL(new Blob([part.svg], {type:'image/svg+xml'}));
        view.urls.push(url); image.src = url; image.className = 'formula-image';
        image.alt = '公式预览';
        image.style.width = part.width + 'pt'; image.style.height = part.height + 'pt';
        image.style.verticalAlign = -(part.height-part.baseline) + 'pt';
        children.push(image);
      }
    } else {
      const placeholder = document.createElement('span'); placeholder.className = 'preview-pending';
      const range = bounds(view.formula); placeholder.textContent = text(range.start, range.end);
      children.push(placeholder);
    }
    view.widget.replaceChildren(...children); view.painted = signature;
  }
  view.widget.classList.toggle('fragment-unavailable', Boolean(entry?.error));
  view.widget.title = entry?.error || (entry?.pending ? '正在更新，保留上次 SVG' :
    entry?.stale ? '保留的 SVG 缓存；点击编辑或全部更新可刷新' : '点击进入显示模式');
}
function macroRow(record) {
  const id = record.definition.cache_id;
  let row = macros.get(id);
  if (!row) {
    const widget = document.createElement('span'); widget.className = 'macro-widget'; widget.contentEditable = 'false';
    const input = document.createElement('textarea'); input.spellcheck = false;
    input.setAttribute('aria-label', '宏定义'); widget.append(input);
    row = {widget,input,original:'',start:record.start,end:record.end}; macros.set(id,row);
    input.addEventListener('input', () => { input.style.height = 'auto'; input.style.height = input.scrollHeight + 'px'; });
    input.addEventListener('keydown', event => {
      event.stopPropagation();
      if (event.key === 'Escape') { event.preventDefault(); placeCaret(row.end); }
      if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        enqueue(async () => {
          const value = input.value;
          await edit(row.start, row.end, value);
          row.original = value;
          placeCaret(row.start + byteLength(value));
        });
      }
    });
  }
  const spelling = text(record.start,record.end);
  if (row.input.value === row.original) row.input.value = spelling;
  row.original = spelling; row.start = record.start; row.end = record.end;
  row.input.rows = Math.max(1,row.input.value.split('\n').length);
  rangeAttributes(row.widget,row.start,row.end);
  return row.widget;
}
function renderDocument() {
  rendering = true;
  const alive = new Set(), liveMacros = new Set();
  const records = model.formulas.map(formula => ({kind:'formula', ...bounds(formula), formula}));
  for (const definition of model.macros.definitions) {
    const start = definition.range.start > 0 && text(definition.range.start-1,definition.range.start) === '#'
      ? definition.range.start-1 : definition.range.start;
    records.push({kind:'macro',start,end:definition.range.end,definition});
  }
  records.sort((a,b) => a.start-b.start || b.end-a.end);
  const children = [];
  function appendText(start,end) {
    const span = document.createElement('span'); span.className = 'text-run';
    rangeAttributes(span,start,end); span.textContent = text(start,end); children.push(span);
  }
  let offset = 0;
  for (const record of records) {
    if (record.start < offset) continue;
    appendText(offset,record.start);
    if (record.kind === 'macro') {
      liveMacros.add(record.definition.cache_id); children.push(macroRow(record));
    } else {
      const id = record.formula.projection.cache_id;
      alive.add(id);
      let view = views.get(id);
      if (!view) { view = makeView(record.formula); views.set(id,view); }
      view.formula = record.formula;
      if (!view.formula.valid) view.mode = 'display';
      renderView(view); children.push(view.widget);
    }
    offset = record.end;
  }
  appendText(offset,byteLength(model.source));
  for (const [id,view] of views) if (!alive.has(id)) {
    view.editor?.destroy(); view.urls.forEach(url => URL.revokeObjectURL(url)); views.delete(id);
    if (activeId === id) activeId = null;
  }
  for (const id of macros.keys()) if (!liveMacros.has(id)) macros.delete(id);
  page.replaceChildren(...children);
  if (sourceDraftRevision === null) sourceBox.value = model.source;
  if (model.diagnostics.length) message(model.diagnostics.map(d => d.message).join('；'),true);
  else message(model.formulas.length + ' 个公式 · 文档版本 ' + model.revision);
  if (active()) showContext(active().formula); else contextBox.value = '';
  rendering = false;
}

page.addEventListener('beforeinput', event => {
  if (insideEditor(event) || event.isComposing || composing) return;
  event.preventDefault(); readCaret();
  const type = event.inputType, data = event.data || '';
  enqueue(async () => {
    let start = Math.min(caret.head,caret.anchor), end = Math.max(caret.head,caret.anchor), inserted = data;
    if (type === 'insertParagraph' || type === 'insertLineBreak') inserted = '\n';
    else if (type.startsWith('delete')) {
      inserted = '';
      if (start === end) {
        if (type.endsWith('Backward')) {
          const formula = model.formulas.find(f => bounds(f).end === start);
          start = formula ? bounds(formula).start : start - byteLength([...text(0,start)].at(-1) || '');
        } else {
          const formula = model.formulas.find(f => bounds(f).start === end);
          end = formula ? bounds(formula).end : end + byteLength([...text(end)].at(0) || '');
        }
      }
    } else if (!type.startsWith('insert')) return;
    await edit(start,end,inserted);
  });
});
page.addEventListener('compositionstart', event => { if (!insideEditor(event)) { readCaret(); composing = true; } });
page.addEventListener('compositionend', event => {
  if (insideEditor(event)) return;
  composing = false;
  enqueue(() => edit(Math.min(caret.head,caret.anchor), Math.max(caret.head,caret.anchor), event.data || ''));
});
page.addEventListener('paste', event => {
  if (insideEditor(event)) return;
  event.preventDefault(); readCaret();
  const value = event.clipboardData.getData('text/plain');
  enqueue(() => edit(Math.min(caret.head,caret.anchor), Math.max(caret.head,caret.anchor), value));
});
page.addEventListener('keydown', event => {
  if (insideEditor(event) || event.shiftKey) return;
  readCaret();
  if (caret.head !== caret.anchor) return;
  if (['ArrowLeft','ArrowRight'].includes(event.key)) {
    const forward = event.key === 'ArrowRight';
    const view = [...views.values()].find(v => (forward ? bounds(v.formula).start : bounds(v.formula).end) === caret.head);
    if (!view) return;
    event.preventDefault();
    if (view.mode === 'display') focusView(view, forward ? 'start' : 'end');
    else placeCaret(forward ? bounds(view.formula).end : bounds(view.formula).start);
  } else if (['ArrowUp','ArrowDown'].includes(event.key)) {
    const selection = getSelection();
    if (!selection.rangeCount || !page.contains(selection.focusNode)) return;
    const cursor = selection.getRangeAt(0).getBoundingClientRect(), down = event.key === 'ArrowDown';
    const nearest = [...views.values()].filter(v => v.mode === 'display').map(v => ({v,r:v.widget.getBoundingClientRect()}))
      .filter(({r}) => (down ? r.top >= cursor.top : r.bottom <= cursor.bottom) &&
        Math.abs((down ? r.top-cursor.bottom : cursor.top-r.bottom)) < 65 &&
        cursor.left >= r.left-15 && cursor.left <= r.right+15)
      .sort((a,b) => Math.abs(a.r.top-cursor.top)-Math.abs(b.r.top-cursor.top))[0];
    if (nearest) {
      event.preventDefault(); focusView(nearest.v);
      if (nearest.v.formula.valid) nearest.v.editor.enterAt(cursor.left, down ? nearest.r.top : nearest.r.bottom);
    }
  }
});

async function insert(kind) {
  // Toolbar acts in document space, after the currently focused formula if
  // focus is inside one. Merely having display-mode formulas is not a lock.
  const current = active();
  if (current) caret = {head:bounds(current.formula).end, anchor:bounds(current.formula).end};
  const start = Math.min(caret.head,caret.anchor), end = Math.max(caret.head,caret.anchor);
  const value = kind === 'macro' ? '\n#let custom(x) = $ #x $\n' : kind === 'display' ? '\n$ "" $\n' : '$""$';
  await edit(start,end,value);
  if (kind === 'macro') {
    for (const row of macros.values()) if (row.start >= start && row.start < start+byteLength(value)) row.input.focus();
  } else {
    const view = [...views.values()].find(v => bounds(v.formula).start >= start && bounds(v.formula).end <= start+byteLength(value));
    if (view) { view.mode = 'display'; renderView(view); focusView(view,'start'); await refreshView(view); }
  }
}
for (const kind of ['macro','inline','display']) {
  const button = document.querySelector('#add-' + kind);
  button.addEventListener('pointerdown', event => { readCaret(); event.preventDefault(); });
  button.addEventListener('click', () => enqueue(() => insert(kind)));
}
sourceBox.addEventListener('input', () => { sourceDraftRevision ??= model.revision; });
sourceBox.addEventListener('keydown', event => {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return;
  event.preventDefault();
  enqueue(async () => {
    if (sourceDraftRevision !== null && sourceDraftRevision !== model.revision) throw new Error('源码草稿基于旧版本，请复制草稿后重新载入并合并');
    model = await api('/api/document', {revision:model.revision,source:sourceBox.value});
    sourceDraftRevision = null; activeId = null; renderDocument(); placeCaret(0);
  });
});
document.querySelector('#refresh-preview').addEventListener('click', () => enqueue(async () => {
  await api('/api/refresh-svg', {revision:model.revision,all:true,dependencies:true});
  message('已请求全部更新；编译成功前保留已有 SVG'); await loadSvg();
}));
async function loadSvg() {
  const result = await api('/api/svg', {since:svgGeneration});
  if (result.revision !== model.revision || (svgGeneration !== null && result.generation < svgGeneration)) return;
  svgGeneration = result.generation;
  const alive = new Set(result.alive);
  for (const key of svgEntries.keys()) if (!alive.has(key)) svgEntries.delete(key);
  for (const entry of result.entries) svgEntries.set(entry.key,entry);
  for (const entry of svgEntries.values()) entry.stale = Boolean(entry.cached && entry.cached.revision !== model.revision);
  for (const view of views.values()) {
    if (view.mode === 'preview') paintPreview(view,svgEntries.get(view.id + ':' + view.id));
    else view.editor?.applySvg(svgEntries);
  }
}
async function poll() {
  if (pollBusy || !model) return;
  pollBusy = true;
  try {
    const state = await api('/api/state');
    engineStatus.textContent = (state.busy ? '编译中' : 'Typst') + ' · 已编译 ' + state.compile_runs + ' 次 · 上次 ' + state.compile_ms + ' ms';
    if (state.attempted_revision === model.revision && state.diagnostics.length) message(state.diagnostics.map(d => d.message).join('；'),true);
    if (state.svg_generation !== svgGeneration) await loadSvg();
    if (state.compiled_revision === null) return;
    preview.classList.toggle('stale-preview',state.compiled_revision !== model.revision);
    preview.dataset.version = '编译版本 ' + state.compiled_revision + (state.compiled_revision !== model.revision ? '（旧预览）' : '');
    if (state.render_generation === renderedGeneration) return;
    const nextUrls = [], images = [];
    try {
      for (let pageNumber = 0; pageNumber < state.pages; pageNumber++) {
        const result = await api('/api/page', {revision:state.compiled_revision,render_generation:state.render_generation,page:pageNumber});
        const url = URL.createObjectURL(new Blob([result.svg], {type:'image/svg+xml'})); nextUrls.push(url);
        const image = document.createElement('img'); image.src = url; image.alt = '第 ' + (pageNumber+1) + ' 页'; images.push(image);
      }
    } catch (error) { nextUrls.forEach(url => URL.revokeObjectURL(url)); throw error; }
    pageUrls.forEach(url => URL.revokeObjectURL(url)); pageUrls = nextUrls;
    preview.replaceChildren(...images); renderedGeneration = state.render_generation;
  } catch (error) { message(error.message,true); }
  finally { pollBusy = false; }
}
window.addEventListener('pagehide', () => {
  for (const view of views.values()) { view.editor?.destroy(); view.urls.forEach(url => URL.revokeObjectURL(url)); }
  pageUrls.forEach(url => URL.revokeObjectURL(url));
});
try {
  model = await api('/api/document'); renderDocument(); await poll(); setInterval(poll,500);
} catch (error) { message(error.message,true); }
