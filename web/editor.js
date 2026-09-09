import {EditorView, Decoration, WidgetType, keymap, hoverTooltip} from '@codemirror/view';
import {EditorState, StateField, StateEffect, Annotation} from '@codemirror/state';
import {basicSetup} from 'codemirror';
import {undo, redo, indentWithTab, isolateHistory} from '@codemirror/commands';
import {autocompletion} from '@codemirror/autocomplete';
import {setDiagnostics} from '@codemirror/lint';
import {StreamLanguage} from '@codemirror/language';
import {initMath,syncSource,enterMath,leaveMath,focusMath,mathState,passiveMath,mathAction,refreshAllSvg} from './app.js';
import {byteOffset,charOffset,position,offset,edits,insertion} from './source.js';

const $=s=>document.querySelector(s), host=$('#formula-host');
const formulaChange=Annotation.define(), projection=StateEffect.define();
let view, snapshot, active=null, visual=true, path='main.typ', diskBase=null, diagnosticsTimer, generation=0;
const passive=new Map(), buffers=new Map();
const report=e=>{$('#status').textContent=e.message||String(e);$('#status').classList.add('error');};
async function api(route,body) {
  const response=await fetch(route,body===undefined?{}:{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
  const data=await response.json();if(!response.ok||data.error)throw new Error(data.error||`HTTP ${response.status}`);return data;
}
function source(){return view.state.doc.toString();}
async function language(method,pos=0) {
  const text=source(), file=path;
  const result=await api('/api/lsp',{method,path:file,source:text,position:position(text,pos)});
  if(text!==source()||file!==path)throw new Error('文档已变化，忽略过期 LSP 响应');
  return result;
}
class FormulaWidget extends WidgetType {
  constructor(start,text,display,current,dom){super();Object.assign(this,{start,text,display,current,dom});}
  eq(other){return this.start===other.start&&this.text===other.text&&this.display===other.display&&this.current===other.current&&this.dom===other.dom;}
  toDOM(){
    if(this.current){host.className='formula-widget '+(this.display?'display-formula':'inline-formula');return host;}
    const el=document.createElement('span');el.className='formula-widget '+(this.display?'display-formula':'inline-formula');
    el.append(this.dom.cloneNode(true));el.title='点击编辑公式';el.setAttribute('aria-label','进入公式 '+this.text);
    el.onpointerdown=e=>{e.preventDefault();activate(this.start);};return el;
  }
  ignoreEvent(){return true;}
}
const formulaField=StateField.define({
  create:()=>Decoration.none,
  update(value,tr){value=value.map(tr.changes);for(const effect of tr.effects)if(effect.is(projection))value=effect.value;return value;},
  provide:field=>[EditorView.decorations.from(field),EditorView.atomicRanges.of(v=>v.state.field(field))]
});
function decorations(){
  if(!visual)return Decoration.none;
  const text=source(), ranges=[];
  const equations=active?[...snapshot.equations.filter(e=>e.end<=active.start||e.start>=active.end),active]:snapshot.equations;
  for(const eq of equations){
    const from=charOffset(text,eq.start),to=charOffset(text,eq.end),formula=text.slice(from,to);
    const current=active?.start===eq.start, dom=passive.get(formula);
    if(current||dom)ranges.push(Decoration.replace({widget:new FormulaWidget(eq.start,formula,eq.display,current,dom),block:eq.display}).range(from,to));
  }
  return Decoration.set(ranges,true);
}
function project(){view.dispatch({effects:projection.of(decorations()),annotations:formulaChange.of(true)});}
function stash(){if(active)passive.set(source().slice(charOffset(source(),active.start),charOffset(source(),active.end)),passiveMath());}
function finish(focus=true){
  if(!active)return true;
  if(mathState().pending){report('请先按 Enter 确认或 Esc 取消公式命令');return false;}
  const at=charOffset(source(),active.end);stash();snapshot=leaveMath();active=null;
  project();$('#math-tools').hidden=true;
  if(focus){view.dispatch({selection:{anchor:Math.min(at,view.state.doc.length)}});view.focus();}
  return true;
}
function activate(start){
  try{
    if(active?.start===start){focusMath();return;}
    if(!finish(false))return;
    visual=true;$('#source-mode').textContent='查看纯源码';snapshot=enterMath(start);active=snapshot.active_range;
    project();$('#math-tools').hidden=false;focusMath();
  }catch(e){report(e);}
}
function history(action){if(!finish(false))return;(action==='redo'?redo:undo)(view);view.focus();}
function changed(next){
  const old=active, before=source();snapshot=next;active=next.active_range;
  view.dispatch({changes:{from:charOffset(before,old.start),to:charOffset(before,old.end),insert:next.source.slice(charOffset(next.source,active.start),charOffset(next.source,active.end))},annotations:formulaChange.of(true),userEvent:'input.type'});
  project();
}
function scheduleDiagnostics(){
  clearTimeout(diagnosticsTimer);const ticket=++generation;
  diagnosticsTimer=setTimeout(async()=>{try{
    const result=await language('diagnostics');if(ticket!==generation)return;
    const text=source(), items=result.diagnostics.map(d=>({from:offset(text,d.range.start),to:offset(text,d.range.end),severity:d.severity===1?'error':d.severity===2?'warning':'info',message:d.message,source:'Tinymist'}));
    view.dispatch(setDiagnostics(view.state,items));
    $('#problems').replaceChildren(...items.map(d=>{const b=document.createElement('button');b.textContent=`${position(text,d.from).line+1}: ${d.message}`;b.onclick=()=>{if(finish(false)){view.dispatch({selection:{anchor:d.from},scrollIntoView:true});view.focus();}};return b;}));
    $('#problem-count').textContent=`诊断 ${items.length}`;
  }catch(e){if(ticket===generation)$('#lsp-detail').textContent=e.message;}},450);
}
async function complete(context){
  const word=context.matchBefore(/[\w.@-]*/);if(!context.explicit&&!word?.text)return null;
  try{
    const result=(await language('completion',context.pos)).result;
    const text=source();return {from:word?.from??context.pos,options:(Array.isArray(result)?result:result?.items||[]).map(item=>({label:item.label,detail:item.detail,
      apply(v){try{
        const edit=item.textEdit, range=edit?.range||edit?.replace;
        // Snippets without negotiated support are kept readable; no hidden placeholders.
        const insert=(edit?.newText||item.insertText||item.label).replace(/\$\{\d+:([^}]*)\}/g,'$1').replace(/\$\d+|\$\{\d+\}/g,'');
        const primary=range?{range,newText:insert}:{range:{start:position(text,word?.from??context.pos),end:position(text,context.pos)},newText:insert};
        if(source()!==text)return;v.dispatch({changes:edits(text,[primary,...item.additionalTextEdits||[]]),userEvent:'input.complete'});
      }catch(e){report(e);}}
    }))};
  }catch{return null;}
}
const typstLanguage=StreamLanguage.define({
  startState:()=>({comment:0,string:false}),
  token(stream,state){
    if(state.comment){if(stream.match('*/'))state.comment--;else if(stream.match('/*'))state.comment++;else stream.next();return 'comment';}
    if(stream.match('//')){stream.skipToEnd();return 'comment';}
    if(stream.match('/*')){state.comment++;return 'comment';}
    if(stream.match('"')){while(!stream.eol()){const c=stream.next();if(c==='\\')stream.next();else if(c==='"')break;}return 'string';}
    if(stream.match(/#(?:let|set|show|import|include|if|else|for|while|return)\b/))return 'keyword';
    if(stream.sol()&&stream.match(/=+ /))return 'heading';
    if(stream.match(/\b\d+(?:\.\d+)?(?:pt|em|cm|mm|%)?/))return 'number';
    if(stream.match(/[#${}()[\]^_]/))return 'operator';stream.next();return null;
  }
});
async function definition(){try{
  if(!finish(false))return;
  const reply=await language('definition',view.state.selection.main.head),result=reply.result, item=Array.isArray(result)?result[0]:result;
  if(!item)return;const uri=item.uri||item.targetUri,range=item.range||item.targetSelectionRange;
  if(uri!==reply.uri){const target=decodeURIComponent(new URL(uri).pathname),prefix=decodeURIComponent(new URL(reply.rootUri).pathname);if(!target.startsWith(prefix))throw new Error('定义位于项目外：'+uri);await openFile(target.slice(prefix.length));}
  view.dispatch({selection:{anchor:offset(source(),range.start)},scrollIntoView:true});view.focus();
}catch(e){report(e);}}
function updateTitle(){$('#filename').textContent=path+(source()!==diskBase?' ●':'');}
async function openFile(name){
  if(!finish(false))return;
  if(view)buffers.set(path,{state:view.state,base:diskBase});
  const saved=buffers.get(name),text=saved?null:(await api('/api/file',{path:name})).source;
  path=name;diskBase=saved?saved.base:text;
  view.setState(saved?saved.state:createState(text));snapshot=syncSource(source());active=null;project();updateTitle();scheduleDiagnostics();
}
async function save(){
  if(!finish(false))return;const text=source(),file=path;
  await api('/api/file',{path:file,source:text,base:diskBase});if(path===file){diskBase=text;updateTitle();}await fileList();
}
async function fileList(){const info=await api('/api/files');$('#project-root').textContent=info.root;$('#files').replaceChildren(...info.files.map(file=>{const b=document.createElement('button');b.textContent=file;b.onclick=()=>openFile(file).catch(report);return b;}));return info;}
function createState(doc){return EditorState.create({doc,extensions:[basicSetup,formulaField,typstLanguage,EditorView.lineWrapping,
  keymap.of([{key:'Mod-s',run:()=>{save().catch(report);return true;}},{key:'F12',run:()=>{definition();return true;}},indentWithTab]),
  autocompletion({override:[complete]}),hoverTooltip(async(v,pos)=>{try{const reply=(await language('hover',pos)).result;if(!reply)return null;const c=reply.contents,text=Array.isArray(c)?c.map(x=>typeof x==='string'?x:x.value).join('\n'):typeof c==='string'?c:c?.value;return {pos,create(){const dom=document.createElement('pre');dom.className='hover-doc';dom.textContent=text;return {dom};}};}catch{return null;}}),
  EditorView.updateListener.of(update=>{
    if(update.docChanged){if(!update.transactions.some(t=>t.annotation(formulaChange))){active=null;snapshot=syncSource(source());$('#math-tools').hidden=true;queueMicrotask(project);}updateTitle();scheduleDiagnostics();}
  }),EditorView.domEventHandlers({pointerdown(){if(active&&!finish(false))return true;return false;}})
]});}
try{
  await initMath({changed,exit:finish,history,path:()=>path});
  const initial='= Untitled\n\n在这里编写 Typst。用工具栏插入行内或行间公式。\n';
  view=new EditorView({state:createState(initial),parent:$('#editor')});snapshot=syncSource(initial);
  const info=await fileList();if(info.files.includes('main.typ')){diskBase=(await api('/api/file',{path})).source;view.setState(createState(diskBase));snapshot=syncSource(diskBase);}updateTitle();
  $('#status').textContent='就绪 · Ctrl+S 保存 · Ctrl+Space 补全 · F12 跳转';scheduleDiagnostics();
}catch(e){report(e);}
for(const [id,display] of [['insert-inline',false],['insert-display',true]]){
  $('#'+id).onpointerdown=e=>e.preventDefault();$('#'+id).onclick=()=>{try{if(!finish())return;const selection=view.state.selection.main,change=insertion(source(),selection.from,selection.to,display);view.dispatch({changes:change,annotations:isolateHistory.of('full'),selection:{anchor:change.start}});activate(byteOffset(source(),change.start));}catch(e){report(e);}};
}
$('#edit-formula').onpointerdown=e=>e.preventDefault();$('#edit-formula').onclick=()=>{const head=byteOffset(source(),view.state.selection.main.head),eq=snapshot.equations.find(e=>e.start<=head&&head<=e.end);if(eq)activate(eq.start);else report('请将光标放在 $…$ 公式中');};
$('#source-mode').onclick=()=>{if(!finish(false))return;visual=!visual;project();$('#source-mode').textContent=visual?'查看纯源码':'显示公式';view.focus();};
$('#finish-formula').onclick=()=>finish();$('#undo').onclick=()=>history('undo');$('#redo').onclick=()=>history('redo');
$('#row').onclick=()=>mathAction({action:'add_row'});$('#column').onclick=()=>mathAction({action:'add_column'});
$('#refresh-svg').onclick=()=>{passive.clear();refreshAllSvg();project();};$('#save').onclick=()=>save().catch(report);
$('#format').onclick=async()=>{try{if(!finish(false))return;const result=await language('formatting');view.dispatch({changes:edits(source(),result.result||[]),userEvent:'input.format',annotations:isolateHistory.of('full')});}catch(e){report(e);}};
$('#new-file').onclick=()=>$('#new-dialog').showModal();
$('#new-form').onsubmit=e=>{e.preventDefault();const name=$('#new-name').value.trim();if(!name||buffers.has(name)){report('文件名为空或已打开');return;}if(!finish(false))return;buffers.set(path,{state:view.state,base:diskBase});path=name;diskBase=null;view.setState(createState(''));snapshot=syncSource('');project();updateTitle();$('#new-dialog').close();view.focus();};
$('#download').onclick=()=>{if(!finish(false))return;const url=URL.createObjectURL(new Blob([source()],{type:'text/plain;charset=utf-8'}));const a=document.createElement('a');a.href=url;a.download=path.split('/').at(-1);a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
$('#packages-form').onsubmit=async e=>{e.preventDefault();$('#package-status').textContent='正在读取官方包索引…';try{
  const result=await api('/api/packages',{action:'search',query:$('#package-query').value});$('#package-status').textContent=`${result.items.length} 个版本 · 安装后可离线使用`;
  $('#package-results').replaceChildren(...result.items.map(p=>{const row=document.createElement('article'),title=document.createElement('strong'),description=document.createElement('p'),button=document.createElement('button');title.textContent=`${p.name} ${p.version}`;description.textContent=p.description;button.textContent=p.installed?'插入 import':'安装并插入 import';button.onclick=async()=>{button.disabled=true;try{const installed=await api('/api/packages',{action:'install',spec:`@preview/${p.name}:${p.version}`});if(finish(false)){view.dispatch({changes:{from:0,insert:installed.import},userEvent:'input.package',annotations:isolateHistory.of('full')});view.focus();}button.textContent='已安装 · 再次插入';}catch(e){$('#package-status').textContent=e.message;}finally{button.disabled=false;}};row.append(title,description,button);return row;}));
}catch(e){$('#package-status').textContent=e.message;}};
window.addEventListener('beforeunload',event=>{if(source()!==diskBase||[...buffers.entries()].some(([name,b])=>name!==path&&b.state.doc.toString()!==b.base)){event.preventDefault();event.returnValue='';}});
export {view as editorView};
export function destroyEditor(){clearTimeout(diagnosticsTimer);generation++;view.destroy();}
