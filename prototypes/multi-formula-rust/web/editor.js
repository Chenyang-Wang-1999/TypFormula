// The native backend owns syntax and projection. This component owns only
// selection, unfinished command input and drawing. No iframe or second parser.
import {mathGlyph} from './math-font.js';
const encoder = new TextEncoder();
export const byteLength = text => encoder.encode(text).length;
export const sliceBytes = (text, start, end) => new TextDecoder().decode(encoder.encode(text).slice(start, end));
export const mapOffset = (offset, start, end, length) =>
  offset < start ? offset : offset >= end ? offset + length - (end-start) : start + length;
const templates = {
  frac: 'frac("", "")', sqrt: 'sqrt("")', root: 'root("", "")',
  mat: 'mat("", ""; "", "")', abs: 'abs("")', norm: 'norm("")',
  vec: 'vec("")', hat: 'hat("")', overline: 'overline("")', underline: 'underline("")',
};

export class FormulaEditor {
  constructor(host) {
    this.host = host;
    this.root = document.createElement('span');
    this.root.className = 'formula-editor';
    this.root.contentEditable = 'false';
    this.canvas = document.createElement('span');
    this.canvas.className = 'math-canvas';
    this.keyboard = document.createElement('textarea');
    this.keyboard.className = 'math-keyboard';
    this.keyboard.setAttribute('aria-label', '公式键盘输入');
    this.keyboard.autocomplete = 'off';
    this.root.append(this.canvas, this.keyboard);
    this.stops = [];
    this.head = 0;
    this.anchor = 0;
    this.command = null;
    this.composing = false;
    this.urls = new Map();
    this.rawCache = new Map();
    this.rawDraft = null;
    this.root.addEventListener('focusin', () => { this.host.activate(); this.paintCaret(); });
    this.root.addEventListener('focusout', () => queueMicrotask(() => this.paintCaret()));
    this.canvas.addEventListener('pointerdown', event => {
      if (event.target.closest('input, textarea, .raw-fragment')) return;
      event.preventDefault(); event.stopPropagation();
      this.enterAt(event.clientX, event.clientY);
    });
    this.keyboard.addEventListener('compositionstart', () => { this.composing = true; });
    this.keyboard.addEventListener('compositionend', event => {
      this.composing = false;
      this.keyboard.value = '';
      if (event.data) this.enqueueInsert(event.data);
    });
    this.keyboard.addEventListener('beforeinput', event => {
      if (event.isComposing || this.composing) return;
      event.preventDefault();
      if (event.inputType.startsWith('insert') && event.data) this.enqueueInsert(event.data);
    });
    this.keyboard.addEventListener('paste', event => {
      event.preventDefault();
      this.enqueueInsert(event.clipboardData.getData('text/plain'));
    });
    this.keyboard.addEventListener('copy', event => {
      event.preventDefault();
      event.clipboardData.setData('text/plain', this.host.text(Math.min(this.head, this.anchor), Math.max(this.head, this.anchor)));
    });
    this.keyboard.addEventListener('keydown', event => this.keydown(event));
  }

  update(formula, preserve = true) {
    this.formula = formula;
    if (!preserve) this.head = this.anchor = formula.body.start;
    this.head = Math.max(formula.body.start, Math.min(this.head, formula.body.end));
    this.anchor = Math.max(formula.body.start, Math.min(this.anchor, formula.body.end));
    this.render();
  }

  focus(edge = null) {
    if (edge === true || edge === 'end') this.head = this.anchor = this.formula.body.end;
    if (edge === 'start') this.head = this.anchor = this.formula.body.start;
    this.keyboard.focus({preventScroll: true});
    this.paintCaret();
  }

  enterAt(x, y) {
    const target = this.stops.map(s => ({s, r: s.element.getBoundingClientRect()}))
      .sort((a, b) => (Math.abs(a.r.left-x)+Math.abs((a.r.top+a.r.bottom)/2-y)*2) -
        (Math.abs(b.r.left-x)+Math.abs((b.r.top+b.r.bottom)/2-y)*2))[0]?.s;
    if (target) { this.head = this.anchor = target.offset; this.cursorPath = target.path; }
    this.focus();
  }

  mapEdit(start, end, length) {
    this.head = mapOffset(this.head, start, end, length);
    this.anchor = mapOffset(this.anchor, start, end, length);
  }

  stop(parent, offset, slot, path) {
    if (!slot.editable) return;
    const stop = document.createElement('span');
    stop.className = 'math-stop';
    stop.dataset.offset = offset;
    stop.append(document.createTextNode('\u200b'));
    const record = {element: stop, offset, slot, path};
    this.stops.push(record);
    stop.addEventListener('pointerdown', event => {
      event.preventDefault(); event.stopPropagation();
      this.head = offset;
      if (!event.shiftKey) this.anchor = offset;
      this.cursorPath = path;
      this.focus();
    });
    parent.append(stop);
  }

  slot(slot, path) {
    const element = document.createElement('span');
    element.className = `math-slot slot-${slot.role}`;
    this.stop(element, slot.range.start, slot, path);
    if (slot.hole || !slot.children.length) {
      const hole = document.createElement('span');
      hole.className = 'math-hole';
      hole.textContent = '□';
      hole.addEventListener('pointerdown', event => {
        if (!slot.editable) return;
        event.preventDefault(); event.stopPropagation();
        this.head = this.anchor = slot.range.start; this.cursorPath = path; this.focus();
      });
      element.append(hole);
    }
    slot.children.forEach((node, index) => {
      const child = this.node(node, `${path}/${index}`);
      element.append(child);
      // Plain tokens have per-character stops; structural nodes have boundaries.
      this.stop(element, node.range.end, slot, path);
    });
    if (!slot.hole) this.stop(element, slot.range.end, slot, path);
    return element;
  }

  node(node, path) {
    const element = document.createElement('span');
    element.className = `math-node math-${node.kind}`;
    element.dataset.node = node.cache_id;
    const slot = role => node.slots.find(s => s.role === role);
    const append = role => { const s = slot(role); if (s) element.append(this.slot(s, `${path}/${role}`)); };
    if (node.kind === 'raw') {
      element.classList.add('raw-fragment');
      element.dataset.raw = node.cache_id;
      element.title = '点击编辑这段源码；SVG 来自当前文档编译结果';
      element.textContent = this.host.text(node.range.start, node.range.end);
      if (node.editable) element.addEventListener('click', event => {
        event.stopPropagation();
        if (!element.querySelector('textarea')) this.openRaw(element, node);
      });
      const cached = this.rawCache.get(String(node.cache_id));
      if (cached?.cached) this.paintRaw(element, cached.cached.fragments);
    } else if (node.kind === 'text' || node.kind === 'symbol') {
      const spelling = this.host.text(node.range.start, node.range.end);
      element.classList.toggle('math-literal', node.literal);
      if (!node.literal && ['=', '<', '>', '≤', '≥', '≠', '≈'].includes(node.text)) element.classList.add('math-relation');
      if (!node.literal && ['+', '−', '-', '×', '⋅', '±'].includes(node.text)) element.classList.add('math-binary');
      if (node.editable && node.kind === 'text' && spelling === node.text) {
        let offset = node.range.start;
        const pseudoSlot = {editable: true, range: node.range, hole: false, children: []};
        for (const character of node.text) {
          this.stop(element, offset, pseudoSlot, path);
          element.append(document.createTextNode(node.literal ? character : mathGlyph(character)));
          offset += byteLength(character);
        }
      } else {
        element.textContent = node.literal ? node.text : mathGlyph(node.text || '');
        if (node.editable) element.addEventListener('dblclick', event => {
          event.stopPropagation(); this.openRaw(element, node);
        });
      }
    } else if (node.kind === 'fraction') {
      append('numerator'); append('denominator');
    } else if (node.kind === 'root') {
      append('index');
      if (node.text) { const index = document.createElement('sup'); index.textContent = node.text; element.append(index); }
      const body = document.createElement('span'); body.className = 'root-body';
      const radical = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      radical.classList.add('radical'); radical.setAttribute('viewBox', '0 0 24 100');
      radical.setAttribute('preserveAspectRatio', 'none'); radical.setAttribute('aria-hidden', 'true');
      const stroke = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      stroke.setAttribute('d', 'M 0 62 L 5 56 L 11 94 L 23 0 L 24 0');
      stroke.setAttribute('vector-effect', 'non-scaling-stroke'); radical.append(stroke);
      body.append(radical, this.slot(slot('radicand'), `${path}/radicand`)); element.append(body);
    } else if (node.kind === 'script') {
      append('base');
      const scripts = document.createElement('span'); scripts.className = 'script-slots';
      for (const role of ['upper', 'lower']) { const s = slot(role); if (s) scripts.append(this.slot(s, `${path}/${role}`)); }
      element.append(scripts);
      element.dataset.script = node.cache_id;
      this.paintScript(element, this.rawCache.get(String(node.cache_id))?.cached?.layout);
    } else if (node.kind === 'matrix') {
      const left = document.createElement('span'); left.className = 'matrix-bracket'; left.textContent = '(';
      const grid = document.createElement('span'); grid.className = 'matrix-grid';
      grid.style.gridTemplateColumns = `repeat(${node.columns}, auto)`;
      node.slots.forEach((s, i) => grid.append(this.slot(s, `${path}/${i}`)));
      const right = document.createElement('span'); right.className = 'matrix-bracket'; right.textContent = ')';
      element.append(left, grid, right);
    } else if (node.kind === 'delimiter') {
      element.append(document.createTextNode([...node.text][0] || '(')); append('body');
      element.append(document.createTextNode([...node.text][1] || ')'));
    } else {
      if (node.kind === 'macro') element.title = `${node.text} 的编辑投影；参数仍引用调用处，模板正文只读`;
      if (node.kind === 'decoration') element.classList.add(`decoration-${node.text}`);
      node.slots.forEach((s, i) => element.append(this.slot(s, `${path}/${i}`)));
    }
    if (this.rawDraft?.id === node.cache_id) {
      this.rawDraft.node = node;
      element.replaceChildren(this.rawDraft.input);
    }
    return element;
  }

  render() {
    this.stops = [];
    this.canvas.replaceChildren(this.node(this.formula.projection, 'root'));
    if (this.command) (this.currentStop()?.element || this.canvas).append(this.command);
    this.paintCaret();
  }

  currentStop() {
    return this.stops.find(s => s.offset === this.head && s.path === this.cursorPath)
      || this.stops.find(s => s.offset === this.head)
      || this.stops.reduce((best, s) => !best || Math.abs(s.offset - this.head) < Math.abs(best.offset - this.head) ? s : best, null);
  }

  paintCaret() {
    this.canvas.querySelectorAll('.caret-active').forEach(n => n.classList.remove('caret-active'));
    const stop = this.currentStop();
    if (stop && document.activeElement === this.keyboard) stop.element.classList.add('caret-active');
    this.canvas.querySelectorAll('.selected-node').forEach(n => n.classList.remove('selected-node'));
    if (this.head !== this.anchor) {
      for (const stop of this.stops) {
        if (stop.offset >= Math.min(this.head, this.anchor) && stop.offset < Math.max(this.head, this.anchor)) stop.element.classList.add('selected-node');
      }
    }
  }

  enqueueInsert(text) {
    this.host.enqueue(async () => {
      let start = Math.min(this.head, this.anchor), end = Math.max(this.head, this.anchor);
      const stop = this.currentStop();
      if (start === end && stop?.slot.hole) { start = stop.slot.range.start; end = stop.slot.range.end; }
      const caret = start + byteLength(text);
      await this.host.edit(start, end, text, caret);
      this.head = this.anchor = caret;
      if (this.host.isFocused()) this.focus();
    });
  }

  keydown(event) {
    if (event.isComposing || this.composing) return;
    if (event.key === 'Enter') { event.preventDefault(); this.host.enqueue(() => this.host.commit()); return; }
    if (event.key === 'Escape') { event.preventDefault(); this.host.exit(this.formula.projection.range.end); return; }
    if (event.key === '\\') { event.preventDefault(); this.openCommand(); return; }
    if (event.key === '^' || event.key === '_') {
      event.preventDefault(); this.insertTemplate(`${event.key}("")`); return;
    }
    if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Tab', 'Home', 'End'].includes(event.key)) {
      event.preventDefault(); this.host.enqueue(() => this.move(event.key, event.shiftKey)); return;
    }
    if (event.key === 'Backspace' || event.key === 'Delete') {
      event.preventDefault();
      this.host.enqueue(async () => {
        let start = Math.min(this.head, this.anchor), end = Math.max(this.head, this.anchor);
        if (start === end) {
          const offsets = [...new Set(this.stops.map(s => s.offset))].sort((a,b) => a-b);
          if (event.key === 'Backspace') start = offsets.filter(n => n < this.head).at(-1) ?? start;
          else end = offsets.find(n => n > this.head) ?? end;
        }
        if (start === end) return;
        await this.host.edit(start, end, '', start);
        this.head = this.anchor = start;
        if (this.host.isFocused()) this.focus();
      });
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a') {
      event.preventDefault(); this.anchor = this.formula.body.start; this.head = this.formula.body.end; this.paintCaret();
    }
  }

  move(key, extend) {
    const current = this.currentStop();
    if (!current) return;
    const stops = this.stops.filter((s, i, all) => i === 0 || s.offset !== all[i - 1].offset || s.path !== all[i - 1].path);
    let target;
    if (key === 'Home' || key === 'End') target = key === 'Home' ? stops[0] : stops.at(-1);
    else if (key === 'ArrowUp' || key === 'ArrowDown') {
      const box = current.element.getBoundingClientRect(), down = key === 'ArrowDown';
      target = stops.map(s => ({s, r:s.element.getBoundingClientRect()}))
        .filter(({r}) => down ? r.top > box.top + 5 : r.top < box.top - 5)
        .sort((a,b) => (Math.abs(a.r.top-box.top)*2+Math.abs(a.r.left-box.left))-(Math.abs(b.r.top-box.top)*2+Math.abs(b.r.left-box.left)))[0]?.s;
    } else {
      const forward = key === 'ArrowRight' || (key === 'Tab' && !extend);
      const index = stops.indexOf(current);
      const candidates = forward ? stops.slice(index + 1) : stops.slice(0, index).reverse();
      target = candidates.find(s => key === 'Tab' ? s.path !== current.path : s.offset !== current.offset);
    }
    if (!target) {
      this.host.exit(key === 'ArrowLeft' || key === 'ArrowUp' || (key === 'Tab' && extend) ? this.formula.projection.range.start : this.formula.projection.range.end, key);
      return;
    }
    this.head = target.offset; this.cursorPath = target.path;
    if (!extend || key === 'Tab') this.anchor = this.head;
    this.paintCaret();
  }

  insertTemplate(source) {
    this.host.enqueue(async () => {
      let start = Math.min(this.head, this.anchor), end = Math.max(this.head, this.anchor);
      const stop = this.currentStop();
      if (start === end && stop?.slot.hole) { start = stop.slot.range.start; end = stop.slot.range.end; }
      const hole = source.indexOf('""');
      const caret = start + (hole < 0 ? byteLength(source) : byteLength(source.slice(0, hole)));
      await this.host.edit(start, end, source, caret);
      this.head = this.anchor = caret;
      if (this.host.isFocused()) this.focus();
    });
  }

  openCommand() {
    if (this.command) return;
    const input = document.createElement('input');
    input.className = 'math-command'; input.placeholder = 'frac / sqrt / mat / 宏名称';
    const stop = this.currentStop();
    (stop?.element || this.canvas).append(input);
    this.command = input;
    input.focus();
    input.addEventListener('keydown', event => {
      event.stopPropagation();
      if (event.key === 'Escape') { event.preventDefault(); input.remove(); this.command = null; this.focus(); }
      if (event.key === 'Enter') {
        event.preventDefault();
        const name = input.value.trim();
        if (!name) return;
        input.remove(); this.command = null;
        this.insertTemplate(templates[name] || this.host.macroTemplate(name) || name);
      }
    });
  }

  openRaw(element, node) {
    if (this.rawDraft) { this.rawDraft.input.focus(); return; }
    const input = document.createElement('textarea');
    input.className = 'raw-input'; input.value = this.host.text(node.range.start, node.range.end);
    input.rows = Math.max(1, input.value.split('\n').length);
    input.style.width = `${Math.max(8, Math.min(70, input.value.length + 2))}ch`;
    this.rawDraft = {id: node.cache_id, node, input};
    element.replaceChildren(input); input.focus();
    input.addEventListener('keydown', event => {
      event.stopPropagation();
      if (event.key === 'Escape') { event.preventDefault(); this.rawDraft = null; this.render(); this.focus(); }
      if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        this.host.enqueue(async () => {
          const current = this.rawDraft.node;
          const caret = current.range.start + byteLength(input.value);
          await this.host.edit(current.range.start, current.range.end, input.value, caret);
          this.rawDraft = null; this.head = this.anchor = caret; this.render();
          await this.host.refresh();
          if (this.host.isFocused()) this.focus();
        });
      }
    });
  }

  clearFragments() {
    for (const url of this.urls.values()) URL.revokeObjectURL(url);
    this.urls.clear(); this.rawCache.clear();
  }

  applySvg(entries) {
    const prefix = this.formula.projection.cache_id + ':';
    for (const [key, entry] of entries) {
      if (!key.startsWith(prefix)) continue;
      const id = key.slice(prefix.length), old = this.rawCache.get(id);
      this.rawCache.set(id, entry);
      const changed = entry.cached && old?.cached?.render_generation !== entry.cached.render_generation;
      if (changed) {
        for (const [key, url] of this.urls) if (key.startsWith(id + ':')) { URL.revokeObjectURL(url); this.urls.delete(key); }
      }
      this.canvas.querySelectorAll('[data-raw]').forEach(element => {
        if (element.dataset.raw !== id) return;
        element.title = entry.error || (entry.pending ? '正在更新；保留缓存' : entry.stale ? '保留的 SVG 缓存；切换模式或全部更新可刷新' : '点击编辑源码');
        if (changed && !element.querySelector('textarea')) {
          this.paintRaw(element, entry.cached.fragments);
        }
      });
      this.canvas.querySelectorAll('[data-script]').forEach(element => {
        if (element.dataset.script === id) this.paintScript(element, entry.cached?.layout);
      });
    }
    for (const id of this.rawCache.keys()) if (!entries.has(prefix + id)) {
      this.rawCache.delete(id);
      for (const [key, url] of this.urls) if (key.startsWith(id + ':')) { URL.revokeObjectURL(url); this.urls.delete(key); }
    }
  }

  paintScript(element, layout = {}) {
    element.classList.toggle('limits-layout', layout?.upper === 'limits' || layout?.lower === 'limits');
    for (const role of ['upper', 'lower']) {
      element.querySelector(`:scope > .script-slots > .slot-${role}`)?.classList.toggle('limits', layout?.[role] === 'limits');
    }
  }

  paintRaw(element, fragments) {
    element.replaceChildren();
    for (const fragment of fragments) {
      const key = `${element.dataset.raw}:${fragment.instance}`;
      let url = this.urls.get(key);
      if (!url) { url = URL.createObjectURL(new Blob([fragment.svg], {type:'image/svg+xml'})); this.urls.set(key, url); }
      const image = document.createElement('img'); image.src = url; image.alt = 'Typst 子表达式';
      const em = fragment.em > 0 ? fragment.em : 11;
      image.style.width = `${fragment.width / em}em`; image.style.height = `${fragment.height / em}em`;
      image.style.verticalAlign = `${-(fragment.height - fragment.baseline) / em}em`;
      element.append(image);
    }
    if (fragments.length > 1) element.title = `此源码节点有 ${fragments.length} 个输出实例`;
  }

  destroy() { this.clearFragments(); this.root.remove(); }
}
