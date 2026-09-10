export function bounded(value,min,max,name){
  if(typeof value!=='number'||!Number.isFinite(value)||value<min||value>max)throw new Error(`${name}须在 ${min}–${max} 之间`);
  return value;
}
export class Typography {
  constructor({persist=async()=>{},changed=()=>{}}={}){this.persist=persist;this.changed=changed;this.settings={fontSize:16,editorFontSize:0,svgScale:1};this.revision=0;}
  get size(){return this.settings.editorFontSize||this.settings.fontSize;}
  update(settings){
    const {articleFontSize,...current}=settings;
    this.settings={...this.settings,...current};this.revision++;
    document.documentElement.style.setProperty('--editor-size',`${this.size}px`);
    document.documentElement.style.setProperty('--svg-scale',String(this.settings.svgScale));
    if(this.settings.fontFamily)document.documentElement.style.setProperty('--editor-font',this.settings.fontFamily);
    const label=document.getElementById('editor-font-value');if(label)label.textContent=`${this.size}px`;
    const input=document.getElementById('svg-scale-value');if(input)input.value=String(this.settings.svgScale);
    const ratio=document.getElementById('svg-ratio-value');if(ratio)ratio.textContent='按各 Raw 环境字号自动换算';
    this.changed();
  }
  async set(patch){
    if(patch.editorFontSize!==undefined&&patch.editorFontSize!==0)bounded(patch.editorFontSize,8,64,'编辑字号');
    if(patch.svgScale!==undefined)bounded(patch.svgScale,.1,8,'SVG 倍率');
    const previous=this.settings;this.update(patch);const revision=this.revision;
    try{await this.persist(patch);}catch(error){if(revision===this.revision)this.update(previous);throw error;}
  }
  zoom(delta){return this.set({editorFontSize:Math.max(8,Math.min(64,this.size+delta))});}
  reset(){return this.set({editorFontSize:0});}
}
