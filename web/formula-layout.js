// The viewport limits the box, never the dimensions of its math contents.
export function viewportBounds(viewport){
  const r=viewport.getBoundingClientRect();
  return {left:r.left+viewport.clientLeft,top:r.top+viewport.clientTop,
    right:r.left+viewport.clientLeft+viewport.clientWidth,bottom:r.top+viewport.clientTop+viewport.clientHeight};
}
export function revealInFormula(viewport,rect,padding=4){
  if(!viewport?.clientWidth||!viewport.clientHeight)return false;
  const bounds=viewportBounds(viewport);
  const dx=rect.left<bounds.left+padding?rect.left-bounds.left-padding:rect.right>bounds.right-padding?rect.right-bounds.right+padding:0;
  const dy=rect.top<bounds.top+padding?rect.top-bounds.top-padding:rect.bottom>bounds.bottom-padding?rect.bottom-bounds.bottom+padding:0;
  if(dx)viewport.scrollLeft=Math.max(0,Math.min(viewport.scrollWidth-viewport.clientWidth,viewport.scrollLeft+dx));
  if(dy)viewport.scrollTop=Math.max(0,Math.min(viewport.scrollHeight-viewport.clientHeight,viewport.scrollTop+dy));
  return !!(dx||dy);
}
export function visibleCaret(rect,viewport){
  if(!viewport?.clientWidth||!viewport.clientHeight)return rect;
  const b=viewportBounds(viewport);
  if(rect.left<b.left||rect.left>b.right||rect.bottom<=b.top||rect.top>=b.bottom)return null;
  return {...rect,top:Math.max(rect.top,b.top),bottom:Math.min(rect.bottom,b.bottom)};
}
export function installFormulaViewport(view,onResize=()=>{}){
  let frame;
  const update=()=>{
    const width=view.scrollDOM.clientWidth;
    if(width){const gutters=view.dom.querySelector('.cm-gutters')?.getBoundingClientRect().width||0;view.dom.style.setProperty('--formula-max-width',`${Math.max(32,width-gutters-44)}px`);window.cancelAnimationFrame(frame);frame=window.requestAnimationFrame(onResize);}
  };
  const observer=typeof ResizeObserver==='function'?new ResizeObserver(update):null;
  observer?.observe(view.dom);window.addEventListener('resize',update);update();
  return {dispose(){observer?.disconnect();window.removeEventListener('resize',update);window.cancelAnimationFrame(frame);}};
}
