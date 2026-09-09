// Use the math font's mathematical italic glyphs, not a text italic face.
// This transforms display glyphs only; Typst source and clipboard stay intact.
export function mathGlyph(text) {
  if([...text].length !== 1)return text;
  const c=text.codePointAt(0);
  if(text==='h')return 'ℎ';
  if(c>=0x41 && c<=0x5a)return String.fromCodePoint(0x1d434+c-0x41);
  if(c>=0x61 && c<=0x7a)return String.fromCodePoint(0x1d44e+c-0x61);
  if(c>=0x3b1 && c<=0x3c9)return String.fromCodePoint(0x1d6fc+c-0x3b1);
  const variants={'ϵ':'𝜖','ϑ':'𝜗','ϰ':'𝜘','ϕ':'𝜙','ϱ':'𝜚','ϖ':'𝜛'};
  return variants[text] || text;
}
