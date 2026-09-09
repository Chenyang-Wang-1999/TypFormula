// SPDX-License-Identifier: GPL-2.0-or-later
//! One bounded native request per process. Never infer operators from their names.
use std::io::{self, Read};
use serde::Deserialize;
use serde_json::{Value, json};
use typst::{Library, LibraryExt, World};
use typst::comemo::Track;
use typst::diag::{FileError, FileResult};
use typst::engine::{Engine, Route, Sink, Traced};
use typst::foundations::{Bytes, Content, Datetime, Duration, Packed, SequenceElem, Smart, StyleChain, StyledElem};
use typst::introspection::{EmptyIntrospector, Locator};
use typst::layout::{Abs, Axis, Sides};
use typst::math::{EquationElem, MathSize};
use typst::math::ir::{MathItem, MathKind, ScriptsItem, resolve_equation};
use typst::routines::Arenas;
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::{LazyHash, Protected};

#[derive(Deserialize)]
struct Request { expression: String, #[serde(default)] definitions: String, display: bool }

struct FormulaWorld { library: LazyHash<Library>, book: LazyHash<FontBook>, font: Font, source: Source }
impl World for FormulaWorld {
    fn library(&self) -> &LazyHash<Library> { &self.library }
    fn book(&self) -> &LazyHash<FontBook> { &self.book }
    fn main(&self) -> FileId { self.source.id() }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main() { Ok(self.source.clone()) } else { Err(FileError::AccessDenied) }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.source(id).map(|s| Bytes::from_string(s.text().to_owned()))
    }
    fn font(&self, index: usize) -> Option<Font> { (index == 0).then(|| self.font.clone()) }
    fn today(&self, _: Option<Duration>) -> Option<Datetime> { None }
}

// Only unwrap a singleton group. Never mistake a nested script inside the
// base (or an attachment) for the outer editor Script node.
fn outer_script<'a, 'b>(item: &'b MathItem<'a>) -> Option<(&'b ScriptsItem<'a>, StyleChain<'a>)> {
    let MathItem::Component(comp) = item else { return None };
    match &comp.kind {
        MathKind::Scripts(script) => Some((script, comp.styles)),
        MathKind::Group(group) => {
            let mut parts = group.items.iter().filter(|i| !matches!(i, MathItem::Tag(_) | MathItem::Space));
            let only = parts.next()?;
            if parts.next().is_some() { None } else { outer_script(only) }
        }
        _ => None,
    }
}
fn placement(center: &Option<MathItem>, side: &Option<MathItem>) -> Result<Option<&'static str>, String> {
    match (center.is_some(), side.is_some()) {
        (true, false) => Ok(Some("limits")), (false, true) => Ok(Some("scripts")),
        (false, false) => Ok(None), (true, true) => Err("Typst 合并出了多个附件，暂不能对应单个编辑槽".into()),
    }
}
fn diagnostics(errors: typst::ecow::EcoVec<typst::diag::SourceDiagnostic>) -> String {
    errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("\n")
}

// A module is document content, not a math body. In particular, blank lines
// outside the equation become ParbreakElem, which math IR treats as External.
// Extract only the equation, carrying the styles of its enclosing wrappers.
// Do not search inside equations or arbitrary content: those may contain scripts
// belonging to the base or attachments rather than the requested outer branch.
fn collect_equations<'a>(
    content: &'a Content,
    styles: StyleChain<'a>,
    arenas: &'a Arenas,
    out: &mut Vec<(&'a Packed<EquationElem>, StyleChain<'a>)>,
) {
    if let Some(equation) = content.to_packed::<EquationElem>() {
        out.push((equation, styles));
    } else if let Some(sequence) = content.to_packed::<SequenceElem>() {
        for child in &sequence.children { collect_equations(child, styles, arenas, out); }
    } else if let Some(styled) = content.to_packed::<StyledElem>() {
        let parent = arenas.bump.alloc(styles);
        collect_equations(&styled.child, parent.chain(&styled.styles), arenas, out);
    }
}
fn resolve(req: Request) -> Result<Value, String> {
    let font = Font::new(Bytes::new(include_bytes!("../../web/fonts/NewCMMath-Regular.otf").as_slice()), 0)
        .ok_or("无法读取公式字体")?;
    let space = if req.display { " " } else { "" };
    let source = Source::detached(format!("#set text(font: \"New Computer Modern Math\", size: 24pt)\n{}\n${space}{}{space}$", req.definitions, req.expression));
    let world = FormulaWorld { library: LazyHash::new(Library::default()), book: LazyHash::new(FontBook::from_fonts([&font])), font, source };
    let world_ref: &dyn World = &world;
    let traced = Traced::default();
    let mut sink = Sink::new();
    let module = typst_eval::eval(world_ref.track(), &world.library, traced.track(), sink.track_mut(), Route::default().track(), &world.source).map_err(diagnostics)?;
    let content = module.content();
    let size = EquationElem::size.set(if req.display { MathSize::Display } else { MathSize::Text }).wrap();
    let base_styles = StyleChain::new(&world.library.styles);
    let styles = base_styles.chain(&size);
    let arenas = Arenas::default();
    let mut equations = vec![];
    collect_equations(&content, styles, &arenas, &mut equations);
    if equations.len() != 1 { return Err("适配请求必须包含一个顶层公式".into()); }
    let (equation, styles) = equations[0];
    let introspector = EmptyIntrospector;
    let mut engine = Engine { world: world_ref.track(), library: &world.library, introspector: Protected::new(introspector.track()), traced: traced.track(), sink: sink.track_mut(), route: Route::default() };
    let item = resolve_equation(equation, &mut engine, Locator::root(), &arenas, styles).map_err(diagnostics)?;
    let (script, script_styles) = outer_script(&item).ok_or("分支未解析成单个上下标对象")?;
    if script.top_left.is_some() || script.bottom_left.is_some() { return Err("暂不支持左侧附件的槽位映射".into()); }
    let upper = placement(&script.top, &script.top_right)?;
    let lower = placement(&script.bottom, &script.bottom_right)?;
    let stretch = matches!(&script.base, MathItem::Component(comp) if matches!(&comp.kind, MathKind::Glyph(glyph) if glyph.stretch.get().is_explicit(Axis::X)));
    let mut base = Value::Null;
    if stretch {
        // The first pass invokes Typst's layout_scripts: attachment measurement
        // sets the base's shared stretch reference. Reuse it to export only the
        // base frame. No SVG/source reverse mapping and no reimplemented policy.
        let _ = typst_layout::editor_math_frame(&mut engine, &item, script_styles).map_err(diagnostics)?;
        let frame = typst_layout::editor_math_frame(&mut engine, &script.base, script_styles).map_err(diagnostics)?;
        let width = frame.width().to_pt(); let height = frame.height().to_pt();
        let page = typst_layout::Page { frame, bleed: Sides::splat(Abs::zero()), fill: Smart::Custom(None), numbering: None, supplement: Content::empty(), number: 1 };
        base = json!({"svg":typst_svg::svg(&page, &Default::default()),"width":width,"height":height});
    }
    Ok(json!({"engine":"Typst math IR 59b5999","upper":upper,"lower":lower,"stretch":stretch,"base":base}))
}
fn main() {
    let result = (|| {
        let mut body = String::new();
        io::stdin().take(256 * 1024 + 1).read_to_string(&mut body).map_err(|e| e.to_string())?;
        if body.len() > 256 * 1024 { return Err("请求过大".into()); }
        resolve(serde_json::from_str(&body).map_err(|e| e.to_string())?)
    })();
    println!("{}", result.unwrap_or_else(|error| json!({"error":error})));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn query(expression: &str, display: bool) -> Value {
        resolve(Request { expression: expression.into(), definitions: String::new(), display }).unwrap()
    }
    #[test]
    fn typst_decides_both_defaults_and_explicit_overrides() {
        assert_eq!(query("sum_1^2", true)["upper"], "limits");
        assert_eq!(query("sum_1^2", false)["upper"], "scripts");
        assert_eq!(query("lim_1", true)["lower"], "limits");
        assert_eq!(query("x_1^2", true)["lower"], "scripts");
        assert_eq!(query("scripts(sum)_1^2", true)["lower"], "scripts");
        assert_eq!(query("limits(A)_1^2", false)["lower"], "limits");
    }
    #[test]
    fn lim_branch_ignores_document_blank_lines_but_keeps_its_math_styles() {
        for definitions in ["", "\n\n", "#let unrelated = 1\n\n"] {
            for display in [true, false] {
                let result = resolve(Request { expression: "lim_(x -> oo)".into(), definitions: definitions.into(), display }).unwrap();
                assert_eq!(result["lower"], if display { "limits" } else { "scripts" });
            }
        }
    }
    #[test]
    fn empty_slots_still_get_a_position_and_definitions_are_evaluated() {
        assert_eq!(query("sum_()", true)["lower"], "limits");
        let custom = resolve(Request { expression: "my_1".into(), definitions: "#let my = math.op(\"my\", limits: true)".into(), display: true }).unwrap();
        assert_eq!(custom["lower"], "limits");
        assert!(resolve(Request { expression: "unknown_name_1".into(), definitions: String::new(), display: true }).is_err());
    }
    #[test]
    fn stretch_uses_attachment_measurements_and_keeps_only_the_base_svg() {
        let short = query("stretch(arrow.r)^(\"a\")", true);
        let long = query("stretch(arrow.r)^(\"a much longer label\")", true);
        assert_eq!(long["upper"], "limits");
        assert_eq!(long["stretch"], true);
        assert!(long["base"]["width"].as_f64().unwrap() > short["base"]["width"].as_f64().unwrap());
        assert!(long["base"]["svg"].as_str().unwrap().contains("<svg"));
    }
}
