//! One in-memory document compile, then source-range → labelled frame → SVG.
use super::*;
use std::collections::HashMap;
use typst::layout::{Abs, Frame, FrameItem, Point, Sides};
use typst::foundations::Smart;
use typst::syntax::{SyntaxKind, SyntaxNode};

#[derive(Deserialize)]
pub struct RawRange { pub id: String, pub start: usize, pub end: usize }
#[derive(Deserialize)]
pub struct RenderRequest { #[serde(default)] pub preview: bool, #[serde(default)] pub pdf:bool, #[serde(default)] pub overlays: HashMap<String,String>, #[serde(default="default_path")] pub path: String, pub source: String, pub raw: Vec<RawRange>, #[serde(default)] pub formulas: Vec<RawRange>, #[serde(default)] pub preview_hashes:Vec<String> }

fn valid_range(node: &SyntaxNode, offset: usize, start: usize, end: usize, math: bool) -> bool {
    let math = math || node.kind() == SyntaxKind::Math;
    if math && offset == start && offset + node.len() == end { return true; }
    let mut pos = offset;
    let children: Vec<_> = node.children().collect();
    for (i,child) in children.iter().enumerate() {
        if math && pos==start && child.kind()==SyntaxKind::Hash
            && children.get(i+1).is_some_and(|next|pos+child.len()+next.len()==end) { return true; }
        if pos <= start && end <= pos+child.len() && valid_range(child,pos,start,end,math) { return true; }
        pos += child.len();
    }
    false
}

pub fn render(req: RenderRequest, world: &mut FormulaWorld) -> Result<Value, String> {
    world.overlays=req.overlays.clone(); world.time=typst_kit::datetime::Time::system();
    if req.pdf { return pdf(&req,world); }
    if req.preview { return preview(&req,world); }
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
    // Keep the document's actual text size. The browser maps Typst points to
    // editor pixels with editor-size / article-size.
    source.insert_str(0,"#set text(font: \"New Computer Modern Math\")\n");
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
                let mapped = group.label.as_ref().and_then(|label| {
                    let label = label.resolve();
                    let (identity, size) = label.rsplit_once(":base-font-pt:")?;
                    let size = size.parse::<f64>().ok().filter(|s|s.is_finite() && *s > 0.0)?;
                    Some((*labels.get(identity)?, size))
                });
                if let Some((r, environment_font_size_pt)) = mapped {
                    if !formula_labels.is_empty() && active.is_empty() { continue; }
                    let id = active.last().map_or_else(||r.id.clone(),|(_,formula)|format!("{}:{formula}",r.id));
                    let occurrence = counts.entry(id.clone()).or_insert(0);
                    let mut output = group.frame.clone(); output.transform(group.transform);
                    let width = output.width().to_pt(); let height = output.height().to_pt(); let baseline = output.baseline().to_pt();
                    let svg_page = typst_layout::Page { frame:output, bleed:Sides::splat(Abs::zero()), fill:Smart::Custom(None), numbering:None, supplement:Content::empty(), number:1 };
                    items.push(json!({"id":format!("{id}:{}",*occurrence),"start":r.start,"end":r.end,"page":page,
                        "x":(at.x+pos.x).to_pt(),"y":(at.y+pos.y).to_pt(),"width":width,"height":height,"baseline":baseline,
                        // Compatibility names requested by the editor protocol:
                        // these three fields are ratios, not absolute pt sizes.
                        "base_font_size_pt":width/environment_font_size_pt,
                        "base_font_height_pt":height/environment_font_size_pt,
                        "base_font_baseline_pt":baseline/environment_font_size_pt,
                        "environment_font_size_pt":environment_font_size_pt,
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
fn pdf(req:&RenderRequest,world:&mut FormulaWorld)->Result<Value,String> {
    use base64::Engine as _;
    let id=source_id(&req.path)?;
    if world.source.id()!=id {world.source=Source::new(id,req.source.clone());} else {world.source.replace(&req.source);}
    let result=typst::compile::<typst_layout::PagedDocument>(world);
    let document=result.output.map_err(diagnostics)?;
    let bytes=typst_pdf::pdf(&document,&Default::default()).map_err(diagnostics)?;
    Ok(json!({"pdf":base64::engine::general_purpose::STANDARD.encode(bytes),"pages":document.pages().len(),"warnings":result.warnings.iter().map(|w|w.message.as_str()).collect::<Vec<_>>()}))
}

pub fn world() -> Result<FormulaWorld,String> {
    Ok(FormulaWorld {library:LazyHash::new(Library::default()),fonts:font_store(!cfg!(test)),source:Source::detached(String::new()),overlays:HashMap::new(),time:typst_kit::datetime::Time::system()})
}
fn preview(req:&RenderRequest,world:&mut FormulaWorld)->Result<Value,String> {
    let id=source_id(&req.path)?;
    if world.source.id()!=id {world.source=Source::new(id,req.source.clone());} else {world.source.replace(&req.source);}
    // Compile the exact document, without the structural editor's 24pt style or labels.
    let result=typst::compile::<typst_layout::PagedDocument>(world);
    let document=result.output.map_err(diagnostics)?;
    fn mapping(frame:&Frame, transform:typst::layout::Transform, source:&Source, output:&mut Vec<Value>) {
        for (pos,item) in frame.items() {
            match item {
                FrameItem::Group(group)=>mapping(&group.frame,transform.pre_concat(typst::layout::Transform::translate(pos.x,pos.y)).pre_concat(group.transform),source,output),
                FrameItem::Text(text)=>{
                    let mut x=pos.x;
                    for glyph in &text.glyphs {
                        if glyph.span.0.id()==Some(source.id()) {
                            if let Some(node)=source.find(glyph.span.0) {
                                let start=(node.offset()+usize::from(glyph.span.1)).min(node.range().end);
                                let point=Point::new(x,pos.y).transform(transform);
                                let color=match &text.fill { typst::visualize::Paint::Solid(color)=>Some(color.to_hex().to_string()), _=>None };
                                let end=(start+glyph.range().len()).min(node.range().end);
                                output.push(json!({"start":start,"end":end,"x":point.x.to_pt(),"y":point.y.to_pt(),"color":color}));
                            }
                        }
                        x+=glyph.x_advance.at(text.size);
                    }
                },
                _=>{}
            }
        }
    }
    let pages=document.pages().iter().enumerate().map(|(index,p)|{
        let hash=format!("{:032x}",typst::utils::hash128(p));
        let unchanged=req.preview_hashes.get(index)==Some(&hash);
        let mut positions=vec![];
        if !unchanged {mapping(&p.frame,typst::layout::Transform::identity(),&world.source,&mut positions);}
        json!({"svg":if unchanged{Value::Null}else{json!(typst_svg::svg(p,&Default::default()))},"width":p.frame.width().to_pt(),"height":p.frame.height().to_pt(),"mapping":if unchanged{Value::Null}else{json!(positions)},"hash":hash})
    }).collect::<Vec<_>>();
    Ok(json!({"pages":pages,"warnings":result.warnings.iter().map(|w|w.message.as_str()).collect::<Vec<_>>()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_svg_width_does_not_depend_on_environment_size() {
        let mut world=world().unwrap();
        let small=compile(&mut world,"#set text(size: 12pt)\n$cancel(x)$",&["cancel(x)"]);
        let large=compile(&mut world,"#set text(size: 36pt)\n$cancel(x)$",&["cancel(x)"]);
        let a=&small["items"][0];let b=&large["items"][0];
        assert_eq!(a["environment_font_size_pt"],12.0);
        assert_eq!(b["environment_font_size_pt"],36.0);
        for field in ["base_font_size_pt","base_font_height_pt","base_font_baseline_pt"]{
            assert!((a[field].as_f64().unwrap()-b[field].as_f64().unwrap()).abs()<1e-6,"{field}");
        }
        assert!((a["base_font_size_pt"].as_f64().unwrap()-a["width"].as_f64().unwrap()/12.0).abs()<1e-6);
    }
    #[test]
    fn raw_in_nested_scripts_keeps_base_size_and_its_smaller_svg() {
        let mut world=world().unwrap();
        let mut widths=vec![];
        for expression in ["cancel(x)","a^(cancel(x))","a^(b^(cancel(x)))"]{
            let result=compile(&mut world,&format!("#set text(size: 20pt)\n${expression}$"),&["cancel(x)"]);
            let item=&result["items"][0];assert_eq!(item["environment_font_size_pt"],20.0);
            widths.push(item["base_font_size_pt"].as_f64().unwrap());
        }
        assert!(widths[0]>widths[1]&&widths[1]>widths[2],"{widths:?}");
    }
    #[test]
    fn repeated_macro_raw_reports_each_rendered_environment() {
        let mut world=world().unwrap();
        let source="#let f() = $cancel(x)$\n#set text(size:12pt)\n#f()\n#set text(size:36pt)\n#f()";
        let result=compile(&mut world,source,&["cancel(x)"]);
        let items=result["items"].as_array().unwrap();assert_eq!(items.len(),2);
        assert_eq!(items[0]["environment_font_size_pt"],12.0);assert_eq!(items[1]["environment_font_size_pt"],36.0);
        assert_ne!(items[0]["id"],items[1]["id"]);
        assert!((items[0]["base_font_size_pt"].as_f64().unwrap()-items[1]["base_font_size_pt"].as_f64().unwrap()).abs()<1e-6);
    }
    #[test]
    fn local_environment_size_is_resolved_but_styles_inside_raw_remain_visible() {
        let mut world=world().unwrap();
        let local=compile(&mut world,"#set text(size:12pt)\n#text(size:2em)[$cancel(x)$]",&["cancel(x)"]);
        assert_eq!(local["items"][0]["environment_font_size_pt"],24.0);
        let source="#let big = text(size:24pt)[$cancel(x)$]\n#set text(size:12pt)\n$big$";
        let inner=compile(&mut world,source,&["big"]);
        assert_eq!(inner["items"][0]["environment_font_size_pt"],12.0);
        let small=compile(&mut world,"#set text(size:12pt)\n$cancel(x)$",&["cancel(x)"]);
        assert!(inner["items"][0]["base_font_size_pt"].as_f64().unwrap()>1.9*small["items"][0]["base_font_size_pt"].as_f64().unwrap());
    }
    #[test]
    fn preview_preserves_page_settings_and_compiles_unsaved_imports() {
        let mut world=world().unwrap();
        let source="#set page(width: 100pt, height: 200pt, margin: 8pt)\n#import \"draft.typ\": value\n#value\n#pagebreak()\n#datetime.today().display()";
        let request=RenderRequest {preview:true,pdf:false,overlays:HashMap::from([("draft.typ".into(),"#let value = [Unsaved document]".into())]),path:"main.typ".into(),source:source.into(),raw:vec![],formulas:vec![],preview_hashes:vec![]};
        let reply=render(request,&mut world).unwrap();
        assert_eq!(reply["pages"].as_array().unwrap().len(),2);
        assert_eq!(reply["pages"][0]["width"],100.0);assert_eq!(reply["pages"][0]["height"],200.0);
        assert_eq!(world.source.text(),source,"preview must not inject math editing styles");
        assert!(reply["pages"][0]["svg"].as_str().unwrap().contains("<svg"));
    }
    #[test]
    fn preview_hashes_skip_unchanged_svg_and_mapping_export() {
        let mut world=world().unwrap();let source="First page\n#pagebreak()\nSecond page";
        let request=RenderRequest{preview:true,pdf:false,overlays:Default::default(),path:"main.typ".into(),source:source.into(),raw:vec![],formulas:vec![],preview_hashes:vec![]};
        let first=render(request,&mut world).unwrap();
        let hashes=first["pages"].as_array().unwrap().iter().map(|page|page["hash"].as_str().unwrap().to_owned()).collect();
        let second=render(RenderRequest{preview:true,pdf:false,overlays:Default::default(),path:"main.typ".into(),source:source.into(),raw:vec![],formulas:vec![],preview_hashes:hashes},&mut world).unwrap();
        assert!(second["pages"].as_array().unwrap().iter().all(|page|page["svg"].is_null()&&page["mapping"].is_null()));
    }
    #[test]
    fn pdf_export_is_native_typst_pdf() {
        use base64::Engine as _;
        let result=render(RenderRequest{preview:false,pdf:true,overlays:Default::default(),path:"main.typ".into(),source:"= PDF\n\nHello $x^2$".into(),raw:vec![],formulas:vec![],preview_hashes:vec![]},&mut world().unwrap()).unwrap();
        let bytes=base64::engine::general_purpose::STANDARD.decode(result["pdf"].as_str().unwrap()).unwrap();
        assert!(bytes.starts_with(b"%PDF"));assert_eq!(result["pages"],1);
    }
    #[test]
    fn relative_imports_use_the_active_file_directory() {
        let source="#import \"defs.typ\": twice\n$twice(x)$";
        let start=source.find("twice(x)").unwrap();
        let reply=render(RenderRequest {preview:false,pdf:false,overlays:Default::default(),path:"tests/fixtures/sub/main.typ".into(),source:source.into(),raw:vec![RawRange{id:"imported".into(),start,end:start+8}],formulas:vec![],preview_hashes:vec![]},&mut world().unwrap()).unwrap();
        assert!(reply["items"][0]["svg"].as_str().unwrap().contains("<path"));
    }
    fn compile(world: &mut FormulaWorld, source: &str, targets: &[&str]) -> Value {
        let raw = targets.iter().enumerate().map(|(i,text)| {
            let start = source.rfind(text).unwrap(); RawRange { id:i.to_string(),start,end:start+text.len() }
        }).collect();
        render(RenderRequest {preview:false,pdf:false,overlays:Default::default(), path:"main.typ".into(), source:source.into(),raw,formulas:vec![],preview_hashes:vec![] },world).unwrap()
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
        assert!(render(RenderRequest {preview:false,pdf:false,overlays:Default::default(),path:"main.typ".into(),source:"$undefined(x)$".into(),raw:vec![],formulas:vec![],preview_hashes:vec![]},&mut world).is_err());
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
            world.source.replace(&format!("#set text(size: 11pt)\n$ stretch(arrow.r)^(\"{label}\") $"));
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
