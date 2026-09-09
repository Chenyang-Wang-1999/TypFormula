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

// Render mapping boxes with the *enclosing* math styles and region. Going
// through ordinary inline-box layout would reset script/display style via an
// inner equation's show-set rule. The label survives frame flattening.
fn editor_mapped_box(
    item: &BoxItem, ctx: &mut MathContext, styles: StyleChain,
) -> SourceResult<Option<Frame>> {
    let Some(label) = item.elem.label().filter(|label| label.resolve().starts_with("visual-typst-raw-")) else { return Ok(None); };
    let Some(equation) = item.elem.body.get_ref(styles).as_ref()
        .and_then(|body| body.to_packed::<EquationElem>()) else { return Ok(None); };
    let arenas = Arenas::default();
    let resolved = resolve_equation(equation, ctx.engine, item.locator.relayout(), &arenas, styles)?;
    let mut frame = ctx.layout_into_fragment(&resolved, styles)?.into_frame();
    frame.label(label);
    Ok(Some(frame))
}
