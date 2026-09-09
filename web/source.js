// Positions at the JS/LSP boundary are UTF-16; Rust and Typst use UTF-8 bytes.
export function byteOffset(source, pos) { return new TextEncoder().encode(source.slice(0,pos)).length; }
export function charOffset(source, bytes) { return new TextDecoder().decode(new TextEncoder().encode(source).slice(0,bytes)).length; }
export function position(source, pos) { const lines=source.slice(0,pos).split('\n');return {line:lines.length-1,character:lines.at(-1).length}; }
export function offset(source, pos) {
  const lines=source.split('\n');
  if(!pos || !Number.isInteger(pos.line)||!Number.isInteger(pos.character)||pos.line<0||pos.line>=lines.length||pos.character<0||pos.character>lines[pos.line].replace(/\r$/,'').length)throw new Error('LSP 位置无效');
  return lines.slice(0,pos.line).reduce((n,l)=>n+l.length+1,0)+pos.character;
}
export function edits(source, items) {
  const changes=items.map(e=>({from:offset(source,e.range.start),to:offset(source,e.range.end),insert:e.newText})).sort((a,b)=>a.from-b.from);
  for(let i=0;i<changes.length;i++)if(changes[i].to<changes[i].from||(i&&changes[i-1].to>changes[i].from))throw new Error('LSP 替换区间重叠');
  return changes;
}
export function insertion(source, from, to, display) {
  const selected=source.slice(from,to);
  const equation=display?`$ ${selected||'""'} $`:`$${selected||'""'}$`;
  const before=display && from>0 && source[from-1]!=='\n'?'\n':'';
  const after=display && source[to]!=='\n'?'\n':'';
  return {from,to,insert:before+equation+after,start:from+before.length};
}
