// Compatibility protocol: despite the suffix, base_font_*_pt are ratios.
export function normalizedMetrics(item){
  const width=item.base_font_size_pt,height=item.base_font_height_pt;
  const environment=item.environment_font_size_pt;
  if(!Number.isFinite(width)||width<0||!Number.isFinite(height)||height<0||!Number.isFinite(environment)||environment<=0){
    throw new Error('SVG 缺少有效的归一化字号信息，请更新并重启原生渲染器');
  }
  return {width,height,environment};
}
export function applySvgMetrics(img){
  img.style.width=`calc(var(--editor-size,16px) * ${Number(img.dataset.baseFontWidth)} * var(--svg-scale,1))`;
  img.style.height=`calc(var(--editor-size,16px) * ${Number(img.dataset.baseFontHeight)} * var(--svg-scale,1))`;
}
export function rawPreviewKey(session,node){
  return JSON.stringify([session,node.render_id??node.edit??null,node.text]);
}
