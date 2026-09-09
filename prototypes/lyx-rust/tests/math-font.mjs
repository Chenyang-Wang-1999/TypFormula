import assert from 'node:assert/strict';
import { mathGlyph } from '../web/math-font.js';
for(const [source,glyph] of [['x','𝑥'],['h','ℎ'],['α','𝛼'],['ϕ','𝜙'],['1','1'],['sin','sin'],['Γ','Γ']])assert.equal(mathGlyph(source),glyph);
console.log('Math font glyph mapping passed.');
