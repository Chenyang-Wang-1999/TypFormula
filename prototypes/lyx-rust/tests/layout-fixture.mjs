// Render the production painter, stylesheet, fonts and WASM views in a local
// standalone page for visual inspection, without starting an HTTP service.
import {readFile,writeFile} from 'node:fs/promises';
const base=new URL('../',import.meta.url);
const {instance}=await WebAssembly.instantiate(await readFile(new URL('web/core.wasm',base)),{});
const wasm=instance.exports,encoder=new TextEncoder(),decoder=new TextDecoder();
function send(action){const b=encoder.encode(JSON.stringify(action)),p=wasm.alloc(b.length);new Uint8Array(wasm.memory.buffer,p,b.length).set(b);const out=wasm.dispatch(p,b.length);const r=JSON.parse(decoder.decode(new Uint8Array(wasm.memory.buffer,out,wasm.output_len())));if(r.error)throw new Error(r.error);return r;}
const examples=['sqrt(3)','root(3, x)','sqrt(frac(1, 2))','sqrt(sqrt(x))','x^sqrt(3)','a &= 1 && "given" \\ x^2 &= 2 & "why"','frac(dif x, 2 pi)','hide(0)'];
const samples=examples.map(source=>({source,view:send({action:'import',source}).view}));
let css=await readFile(new URL('web/style.css',base),'utf8');
for(const name of ['NewCMMath-Regular.otf','NewCM10-Italic.otf'])css=css.replace(`/fonts/${name}`,`data:font/otf;base64,${(await readFile(new URL(`web/fonts/${name}`,base))).toString('base64')}`);
const app=await readFile(new URL('web/app.js',base),'utf8');
const painter=app.slice(app.indexOf('function element('),app.indexOf('function render()'));
const glyph=(await readFile(new URL('web/math-font.js',base),'utf8')).replace('export function','function');
const html=`<!doctype html><meta charset="utf-8"><title>公式布局检查</title><style>${css}
main{max-width:1000px}article{display:grid;grid-template-columns:270px 1fr;gap:35px;background:white;padding:18px 24px;border-bottom:1px solid #eee}.sample{font:32px/1.35 var(--math);display:flex;align-items:center;min-height:70px}code{font:14px Consolas;align-self:center}#checks{white-space:pre-wrap;font:13px Consolas}</style>
<main><h1>公式布局检查</h1><p>生产绘制代码；SVG 样例使用占位图，仅检查布局。</p><div id="samples"></div><pre id="checks">正在等待字体…</pre></main><script>
const serviceReady=true;function measure(){}function send(){}function attachmentFor(){return null;}
function previewFor(node){const empty=node.text==='hide(0)';return {status:'ready',width:empty?0:16,height:empty?0:24,url:'data:image/svg+xml,'+encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="'+(empty?0:16)+'" height="'+(empty?0:24)+'"><text y="19" font-size="22">'+(empty?'':node.text==='dif'?'d':node.text)+'</text></svg>')};}
${glyph}\n${painter}
const samples=${JSON.stringify(samples).replaceAll('<','\\u003c')};
for(const sample of samples){const row=document.createElement('article'),label=document.createElement('code'),drawing=document.createElement('div');label.textContent=sample.source;drawing.className='sample';drawing.append(draw(sample.view));row.append(label,drawing);document.getElementById('samples').append(row);}
document.fonts.ready.then(()=>requestAnimationFrame(()=>{
const issues=[];for(const root of document.querySelectorAll('.root-body')){const mark=root.querySelector(':scope > .radical').getBoundingClientRect(),bar=root.querySelector(':scope > .radicand').getBoundingClientRect();if(Math.abs(mark.right-bar.left)>1||Math.abs(mark.top-bar.top)>1)issues.push('根号轮廓未连接横线');}
const aligned=document.querySelector('.aligned');const columns=Number(aligned.style.getPropertyValue('--columns'));const slots=[...aligned.children];for(let c=0;c<columns;c++){const cells=slots.filter((_,i)=>i%columns===c);if(cells.length>1&&Math.abs(cells[0].getBoundingClientRect().left-cells[1].getBoundingClientRect().left)>1)issues.push('对齐列错位');}
const empty=document.querySelector('article:last-child .raw.rendered').getBoundingClientRect();if(empty.width<10||empty.height<20)issues.push('空 SVG 无可见范围');
document.getElementById('checks').textContent=issues.length?issues.join('\\n'):'通过：根号横线连接、对齐列位置、空 SVG 最小尺寸';
}));</script>`;
await writeFile(new URL('target/layout-preview.html',base),html);
console.log(new URL('target/layout-preview.html',base).href);
