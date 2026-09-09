// Added to the pinned Typst layout crate by prepare.ps1.
// This is an access bridge, not a second implementation of math layout.
/// Lay out an already resolved math item using Typst's own font and glyph engine.
pub fn editor_math_frame(
    engine: &mut Engine,
    item: &MathItem,
    styles: StyleChain,
) -> SourceResult<Frame> {
    let styles = item.styles().unwrap_or(styles);
    let font = get_font(engine.world, styles, Span::detached())?;
    let scale = style_for_script_scale(&font);
    let styles = styles.chain(&scale);
    let mut ctx = MathContext::new(engine, Size::splat(Abs::inf()), font);
    Ok(ctx.layout_into_fragment(item, styles)?.into_frame())
}
