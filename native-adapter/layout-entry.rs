// Added to the pinned Typst layout crate by prepare.ps1.
// Labels identify the final rendered fragment; they never replace math IR.
fn editor_label_fragments(
    ctx: &mut MathContext, start: usize, label: typst_library::foundations::Label,
    props: &MathProperties, styles: StyleChain,
) {
    if ctx.fragments.len() == start + 1
        && matches!(ctx.fragments[start], MathFragment::Glyph(_) | MathFragment::Frame(_))
    {
        ctx.fragments[start].editor_label(label);
    } else {
        let run: MathRun = ctx.fragments.drain(start..).collect();
        let mut frame = run.into_frame();
        frame.label(label);
        ctx.push(FrameFragment::new(props, styles, frame));
    }
}
