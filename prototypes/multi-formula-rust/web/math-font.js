// Display-only mathematical italics. Never rewrite source/clipboard text.
export function mathGlyph(text) {
  if ([...text].length !== 1) return text;
  const code = text.codePointAt(0);
  if (text === 'h') return 'ℎ';
  if (code >= 65 && code <= 90) return String.fromCodePoint(0x1d434 + code - 65);
  if (code >= 97 && code <= 122) return String.fromCodePoint(0x1d44e + code - 97);
  if (code >= 0x3b1 && code <= 0x3c9) return String.fromCodePoint(0x1d6fc + code - 0x3b1);
  return ({'ϵ':'𝜖', 'ϑ':'𝜗', 'ϰ':'𝜘', 'ϕ':'𝜙', 'ϱ':'𝜚', 'ϖ':'𝜛'})[text] || text;
}
