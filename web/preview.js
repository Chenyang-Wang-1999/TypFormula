export class Preview {
  constructor({request,snapshot,container,status,onError,button}){Object.assign(this,{request,snapshot,container,status,onError,button});this.visible=false;this.revision=0;this.urls=[];this.zoom=1;this.busy=false;}
  toggle(){this.visible=!this.visible;this.container.closest('#preview-pane').hidden=!this.visible;document.body.classList.toggle('preview-open',this.visible);if(this.button){this.button.textContent=this.visible?'隐藏预览':'预览';this.button.setAttribute('aria-expanded',String(this.visible));}if(this.visible)this.schedule(0);else{this.revision++;clearTimeout(this.timer);}}
  schedule(delay=450){this.revision++;clearTimeout(this.timer);if(this.visible)this.timer=setTimeout(()=>this.render(),delay);}
  async render(){
    if(!this.visible)return;
    if(this.busy){this.schedule();return;}
    const revision=this.revision, snapshot=this.snapshot();this.busy=true;this.status.textContent='编译中…';
    try{
      const result=await this.request({...snapshot,preview:true,raw:[],formulas:[]});
      if(revision!==this.revision||!this.visible)return;
      const urls=[],nodes=[];
      try{for(const [i,page] of result.pages.entries()){
        if(!page.svg?.includes('<svg')||!Number.isFinite(page.width)||page.width<=0)throw new Error('无效的预览页面');
        const url=URL.createObjectURL(new Blob([page.svg],{type:'image/svg+xml'}));urls.push(url);
        const figure=document.createElement('figure'),image=document.createElement('img'),caption=document.createElement('figcaption');
        image.src=url;image.alt=`第 ${i+1} 页`;image.style.width=`${page.width*4/3}px`;caption.textContent=`${i+1} / ${result.pages.length}`;figure.append(image,caption);nodes.push(figure);
      }}catch(error){urls.forEach(URL.revokeObjectURL);throw error;}
      this.urls.forEach(URL.revokeObjectURL);this.urls=urls;this.container.replaceChildren(...nodes);this.scale();
      this.status.textContent=`${result.pages.length} 页${result.warnings?.length?' · '+result.warnings.join('；'):''}`;
    }catch(error){if(revision===this.revision){this.status.textContent='编译失败，保留上次预览：'+error.message;this.onError?.(error);}}
    finally{this.busy=false;if(revision!==this.revision&&this.visible)this.schedule(0);}
  }
  scale(delta=0){this.zoom=delta===null?1:Math.max(.25,Math.min(3,this.zoom+delta));this.container.style.setProperty('--preview-zoom',String(this.zoom));}
  dispose(){this.visible=false;this.revision++;clearTimeout(this.timer);this.urls.forEach(URL.revokeObjectURL);}
}
