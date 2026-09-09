//! One in-memory document compile, then source-range → labelled frame → SVG.
use super::*;
use std::collections::HashMap;
use typst::layout::{Abs, Frame, FrameItem, Point, Sides};
use typst::foundations::Smart;
use typst::syntax::{SyntaxKind, SyntaxNode};

#[derive(Deserialize)]
pub struct RawRange { pub id: String, pub start: usize, pub end: usize }
#[derive(Deserialize)]
pub struct RenderRequest { #[serde(default="default_path")] pub path: String, pub source: String, pub raw: Vec<RawRange>, #[serde(default)] pub formulas: Vec<RawRange> }

fn valid_range(node: &SyntaxNode, offset: usize, start: usize, end: usize, math: bool) -> bool {
    let math = math || node.kind() == SyntaxKind::Math;
    if math && offset == start && offset + node.len() == end { return true; }
    let mut pos = offset;
    for child in node.children() {
        if pos <= start && end <= pos+child.len() && valid_range(child,pos,start,end,math) { return true; }
        pos += child.len();
    }
    false
}

pub fn render(req: RenderRequest, world: &mut FormulaWorld) -> Result<Value, String> {
    if req.raw.len() > 4096 { return Err("Raw 块过多".into()); }
    let original = Source::detached(req.source.clone());
    let mut ranges: Vec<_> = req.raw.iter().enumerate().collect(); ranges.sort_by_key(|(_,r)| r.start);
    let mut end = 0;
    for (_,r) in &ranges {
        if r.start < end || r.start >= r.end || r.end > req.source.len()
            || !req.source.is_char_boundary(r.start) || !req.source.is_char_boundary(r.end)
            || !valid_range(original.root(),0,r.start,r.end,false) { return Err("Raw 源码区间无效或重叠".into()); }
        end = r.end;
    }
    let mut source = req.source.clone();
    // Labelled equations are transparent source markers in math IR. Their body
    // keeps native glyph/stretch/attachment semantics until final frame export.
    let mut labels = HashMap::new(); let mut edits = vec![]; let mut formula_labels = HashMap::new();
    for (i,r) in req.formulas.iter().enumerate() {
        if r.start >= r.end || req.source.get(r.start..r.end).is_none_or(|s| !s.starts_with('$') || !s.ends_with('$')) { return Err("公式源码区间无效".into()); }
        let label = format!("visual-typst-formula-{i}"); formula_labels.insert(label.clone(), r.id.clone());
        edits.push((r.end,r.end,format!("#label(\"{label}\")")));
    }
    for (i,r) in ranges.into_iter().rev() {
        let label = format!("visual-typst-raw-{i}"); labels.insert(label.clone(),r);
        edits.push((r.start,r.end,format!("#[${}$<{label}>]",&req.source[r.start..r.end])));
    }
    edits.sort_by_key(|(start,end,_)|(*start,*end));
    for (start,end,replacement) in edits.into_iter().rev() { source.replace_range(start..end,&replacement); }
    source.insert_str(0,"#set text(font: \"New Computer Modern Math\", size: 24pt)\n");
    let id=source_id(&req.path)?;
    if world.source.id()!=id { world.source=Source::new(id,source); } else { world.source.replace(&source); }
    let result = typst::compile::<typst_layout::PagedDocument>(world);
    let document = result.output.map_err(diagnostics)?;
    let mut items = vec![]; let mut counts = HashMap::new();
    fn collect(frame: &Frame, page: usize, at: Point, labels: &HashMap<String,&RawRange>, formula_labels: &HashMap<String,String>, active: &mut Vec<(typst::introspection::Location,String)>, counts: &mut HashMap<String,usize>, items: &mut Vec<Value>) {
        for (pos,item) in frame.items() {
            match item {
                FrameItem::Tag(typst::introspection::Tag::Start(content,..)) => {
                    if let Some(id) = content.label().and_then(|label|formula_labels.get(&*label.resolve())) { active.push((content.location().unwrap(),id.clone())); }
                }
                FrameItem::Tag(typst::introspection::Tag::End(location,..)) => { if active.last().is_some_and(|(l,_)|l==location) { active.pop(); } }
                _ => {}
            }
            if let FrameItem::Group(group) = item {
                if let Some(r) = group.label.as_ref().and_then(|l| labels.get(&*l.resolve())) {
                    if !formula_labels.is_empty() && active.is_empty() { continue; }
                    let id = active.last().map_or_else(||r.id.clone(),|(_,formula)|format!("{}:{formula}",r.id));
                    let occurrence = counts.entry(id.clone()).or_insert(0);
                    let mut output = group.frame.clone(); output.transform(group.transform);
                    let width = output.width().to_pt(); let height = output.height().to_pt(); let baseline = output.baseline().to_pt();
                    let svg_page = typst_layout::Page { frame:output, bleed:Sides::splat(Abs::zero()), fill:Smart::Custom(None), numbering:None, supplement:Content::empty(), number:1 };
                    items.push(json!({"id":format!("{id}:{}",*occurrence),"start":r.start,"end":r.end,"page":page,
                        "x":(at.x+pos.x).to_pt(),"y":(at.y+pos.y).to_pt(),"width":width,"height":height,"baseline":baseline,
                        "svg":typst_svg::svg(&svg_page,&Default::default())}));
                    *occurrence += 1;
                } else { collect(&group.frame,page,at+*pos,labels,formula_labels,active,counts,items); }
            }
        }
    }
    let mut active = vec![];
    for (page,output) in document.pages().iter().enumerate() { collect(&output.frame,page+1,Point::zero(),&labels,&formula_labels,&mut active,&mut counts,&mut items); }
    Ok(json!({"engine":"Typst in-memory source mapping","items":items,"pages":document.pages().len(),
        "warnings":result.warnings.iter().map(|w|w.message.as_str()).collect::<Vec<_>>()}))
}

pub fn world() -> Result<FormulaWorld,String> {
    let font = Font::new(Bytes::new(include_bytes!("../../web/fonts/NewCMMath-Regular.otf").as_slice()),0).ok_or("无法读取公式字体")?;
    Ok(FormulaWorld { library:LazyHash::new(Library::default()),book:LazyHash::new(FontBook::from_fonts([&font])),font,source:Source::detached(String::new()) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relative_imports_use_the_active_file_directory() {
        let source="#import \"defs.typ\": twice\n$twice(x)$";
        let start=source.find("twice(x)").unwrap();
        let reply=render(RenderRequest {path:"tests/fixtures/sub/main.typ".into(),source:source.into(),raw:vec![RawRange{id:"imported".into(),start,end:start+8}],formulas:vec![]},&mut world().unwrap()).unwrap();
        assert!(reply["items"][0]["svg"].as_str().unwrap().contains("<path"));
    }
    fn compile(world: &mut FormulaWorld, source: &str, targets: &[&str]) -> Value {
        let raw = targets.iter().enumerate().map(|(i,text)| {
            let start = source.rfind(text).unwrap(); RawRange { id:i.to_string(),start,end:start+text.len() }
        }).collect();
        render(RenderRequest { path:"main.typ".into(), source:source.into(),raw,formulas:vec![] },world).unwrap()
    }
    #[test]
    fn whole_document_maps_decorations_macros_pages_and_empty_output() {
        let mut world = world().unwrap();
        let result = compile(&mut world,"#let marked(x) = $cancel(#x)$\n#let nothing = []\nBefore $marked(x)$ after.\n#pagebreak()\n$ nothing + cancel(y) $", &["marked(x)","nothing","cancel(y)"]);
        assert_eq!(result["pages"],2);
        assert_eq!(result["items"].as_array().unwrap().len(),3);
        assert!(result["items"][2]["svg"].as_str().unwrap().contains("<path"));
        assert_eq!(result["items"][2]["page"],2);
        assert!(result["items"][1]["svg"].as_str().unwrap().contains("<svg"));
    }
    #[test]
    fn mapped_raw_inherits_script_size_and_scoped_context() {
        let mut world = world().unwrap();
        let normal = compile(&mut world,"$ cancel(x) $", &["cancel(x)"]);
        let script = compile(&mut world,"$ a^(cancel(x)) $", &["cancel(x)"]);
        assert!(script["items"][0]["width"].as_f64().unwrap() < normal["items"][0]["width"].as_f64().unwrap());
        let first = compile(&mut world,"#let f(x) = $cancel(#x)$\n$ f(x) $", &["f(x)"]);
        assert_eq!(first["items"].as_array().unwrap().len(),1);
    }
    #[test]
    fn warm_world_recompiles_styles_and_recovers_after_errors() {
        let mut world = world().unwrap();
        let small = compile(&mut world,"#set text(size: 12pt)\n$cancel(x)$", &["cancel(x)"]);
        let large = compile(&mut world,"#set text(size: 36pt)\n$cancel(x)$", &["cancel(x)"]);
        assert!(large["items"][0]["width"].as_f64().unwrap() > 2.9*small["items"][0]["width"].as_f64().unwrap());
        assert!(render(RenderRequest {path:"main.typ".into(),source:"$undefined(x)$".into(),raw:vec![],formulas:vec![]},&mut world).is_err());
        assert_eq!(compile(&mut world,"$dif$", &["dif"])["items"].as_array().unwrap().len(),1);
    }
    #[test]
    fn stretch_document_probe() {
        let mut world = world().unwrap();
        let short = compile(&mut world,"$ stretch(arrow.r)^(\"a\") $", &["stretch(arrow.r)"]);
        let long = compile(&mut world,"$ stretch(arrow.r)^(\"a much longer label\") $", &["stretch(arrow.r)"]);
        let short_width = short["items"][0]["width"].as_f64().unwrap();
        let long_width = long["items"][0]["width"].as_f64().unwrap();
        eprintln!("document-mapped stretch: short={short_width:.4}pt, long={long_width:.4}pt");
        fn arrow_width(frame: &Frame) -> f64 {
            frame.items().map(|(_,item)| match item {
                FrameItem::Group(group) => arrow_width(&group.frame),
                FrameItem::Text(text) if text.text.contains('→') => text.width().to_pt(),
                _ => 0.0,
            }).fold(0.0,f64::max)
        }
        let mut direct = vec![];
        for label in ["a","a much longer label"] {
            world.source.replace(&format!("#set text(size: 24pt)\n$ stretch(arrow.r)^(\"{label}\") $"));
            let document = typst::compile::<typst_layout::PagedDocument>(&world).output.unwrap();
            direct.push(arrow_width(&document.pages()[0].frame));
        }
        eprintln!("unwrapped document arrow: short={:.4}pt, long={:.4}pt",direct[0],direct[1]);
        assert!(direct[1] > direct[0], "control: native Typst stretch should grow");
        assert!(long_width > short_width, "stretch should grow with its attachment");
        assert!((short_width-direct[0]).abs() < 1e-6);
        assert!((long_width-direct[1]).abs() < 1e-6, "mapped SVG must use the native stretched glyph");
    }
}
