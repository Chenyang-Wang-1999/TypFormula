// Pick a formula from UTF-16 document ranges before handing focus to its slots.
export function arrowTarget({key,head,equations,projected,caret,rectangles=[]}){
  if(key==='ArrowRight')return equations.find(e=>e.from===head);
  if(key==='ArrowLeft')return equations.find(e=>e.to===head);
  const down=key==='ArrowDown';if(!down&&key!=='ArrowUp')return;
  // Prefer the actual next visual row (important for wrapped lines and tall math).
  if(caret){
    const candidates=rectangles.filter(r=>down?r.top>=caret.bottom-1:r.bottom<=caret.top+1);
    candidates.sort((a,b)=>{
      const distance=r=>(down?r.top-caret.bottom:caret.top-r.bottom)*1000+Math.max(r.left-caret.left,caret.left-r.right,0);
      return distance(a)-distance(b);
    });
    const closest=candidates[0];
    if(closest&&Math.abs((down?closest.top-caret.bottom:caret.top-closest.bottom))<=Math.max(8,caret.bottom-caret.top)*1.5)return equations.find(e=>e.start===closest.start);
  }
  if(projected!==undefined&&projected!==head)return equations.find(e=>e.from<=projected&&projected<=e.to&&(down?e.from>head:e.to<head));
}
export function slotForEntry(stops,key,x){
  if(!stops.length)return;
  if(key==='ArrowRight')return stops.find(s=>s.cursor.slices.length===0)||stops[0];
  if(key==='ArrowLeft')return [...stops].reverse().find(s=>s.cursor.slices.length===0)||stops.at(-1);
  const ys=stops.map(s=>s.y),edge=key==='ArrowDown'?Math.min(...ys):Math.max(...ys);
  return stops.filter(s=>Math.abs(s.y-edge)<3).sort((a,b)=>Math.abs(a.x-x)-Math.abs(b.x-x))[0];
}
