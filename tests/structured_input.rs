// SPDX-License-Identifier: GPL-2.0-or-later
use visual_typst_core::{Action,Editor,math::*,typst};
fn key(e:&mut Editor,k:&str){e.apply(Action::Key{key:k.into(),shift:false,ctrl:false}).unwrap();}
fn input(e:&mut Editor,s:&str){e.apply(Action::Input{text:s.into()}).unwrap();}
fn command(s:&str)->Editor{let mut e=Editor::default();input(&mut e,&format!("\\{s}"));key(&mut e,"Enter");assert!(e.pending().is_none(),"{s}: {}",e.message);e}
fn load(s:&str)->Editor{let mut e=Editor::default();e.apply(Action::Import{source:s.into()}).unwrap();e}
fn raw(a:&MathAtom,s:&str){assert!(matches!(&a.kind,Kind::Raw{source} if source==s),"{a:?}");}

#[test]
fn nested_raw_keeps_its_editable_fraction_and_source() {
    let mut e=command("frac(dif x, 2 pi)");
    assert!(matches!(e.root[0].kind,Kind::Frac));raw(&e.root[0].cells[0][0],"dif");
    assert!(matches!(e.root[0].cells[1][1].kind,Kind::Symbol{..}));
    assert_eq!(e.root,load("frac(dif x, 2 pi)").root);
    key(&mut e,"Home");key(&mut e,"ArrowRight");key(&mut e,"ArrowRight");
    key(&mut e,"Backspace");assert_eq!(typst::write_cell(&e.root),"frac(x, 2 pi)");
    e.apply(Action::Undo).unwrap();assert_eq!(typst::write_cell(&e.root),"frac(dif x, 2 pi)");
}

#[test]
fn compiler_owns_symbols_operators_shorthands_and_escapes() {
    for s in ["dif","sum","integral","arrow.r","sin","lr","text"] {
        raw(&command(s).root[0],s);
    }
    for s in ["alpha","pi","Omega"] { assert!(matches!(command(s).root[0].kind,Kind::Symbol{..})); }
    for s in ["times","dot","div",">=",r"\/",r"\\"] {
        let e=command(s);
        assert!(matches!(&e.root[0].kind,Kind::Symbol{name,glyph} if name==s && Some(glyph.as_str())==symbol(s)));
        assert_eq!(typst::write_cell(&e.root),s);
        assert_eq!(e.root,load(s).root);
    }
    for s in ["x","123"] { assert!(command(s).root.iter().all(|a|matches!(a.kind,Kind::Char{..}))); }
    for s in ["cancel( x/y  + dif x )", "lr((x), size: #150%)", "text(\"hello\")"] {
        raw(&command(s).root[0],s);raw(&load(s).root[0],s);
    }
    let mut e=Editor::default();input(&mut e,">=");assert_eq!(e.root.len(),2);assert!(e.root.iter().all(|a|matches!(a.kind,Kind::Char{..})));
    assert_eq!(typst::write_cell(&e.root),">=");
}

#[test]
fn ordinary_input_keeps_char_nodes_and_only_maps_single_character_display() {
    let mut e=Editor::default();input(&mut e,"<=");let previous=e.root.clone();input(&mut e,"+");
    assert_eq!(e.root.len(),3);assert_eq!(&e.root[..2],previous.as_slice());
    assert!(e.root.iter().all(|a|matches!(a.kind,Kind::Char{..})));
    assert_eq!(typst::write_cell(&e.root),"<=+");
    key(&mut e,"Backspace");assert_eq!(e.root,previous);
    let mut e=command("<=");let symbol=e.root[0].clone();input(&mut e,"+");
    assert_eq!(e.root[0],symbol);assert!(matches!(e.root[1].kind,Kind::Char{value:'+'}));
    let mut e=Editor::default();input(&mut e,"times");assert_eq!(e.root.len(),5);
    assert!(e.root.iter().all(|a|matches!(a.kind,Kind::Char{..})));
    input(&mut e,"\\times");key(&mut e,"Enter");assert!(matches!(&e.root[5].kind,Kind::Symbol{name,..} if name=="times"));
    let mut e=Editor::default();input(&mut e,"-*");
    let root=e.root.clone();let response=e.response();
    let chars:Vec<_>=response.view.children.iter().filter(|v|v.kind=="char").collect();
    assert_eq!(chars[0].text,"-");assert_eq!(chars[0].display_glyph.as_deref(),Some("−"));
    assert_eq!(chars[1].text,"*");assert_eq!(chars[1].display_glyph.as_deref(),Some("∗"));
    assert_eq!(e.root,root);assert_eq!(typst::write_cell(&e.root),"-*");
}

#[test]
fn definitions_keep_source_and_supported_calls_expand_only_in_view() {
    let definitions = "#let twice(x) = $ #x  + #x $\n#let number = 2\n";
    let mut e = load(&format!("{definitions}$ twice(a) + twice(b) $"));
    assert_eq!(e.definitions,definitions);
    assert!(matches!(&e.root[0].kind, Kind::MacroCall { name, .. } if name == "twice"));
    assert_eq!(typst::write_atom(&e.root[2]), "twice(b)");
    assert_eq!(e.root[0].cells.len(), 1);
    key(&mut e,"End");input(&mut e,"\\times");
    assert!(e.command_context().unwrap().source.starts_with(definitions));
    key(&mut e,"Enter");assert_eq!(e.definitions,definitions);
    let source=typst::write_document(&e.root,&e.definitions);
    assert!(source.starts_with(definitions));assert_eq!(load(&source).root,e.root);
    assert!(typst::parse_document("$ x $\n#let x = 1").is_err());
}

#[test]
fn fraction_slash_uses_typst_precedence_and_keeps_nested_fallbacks() {
    let e=command("(dif x)/ (2 pi)");assert!(matches!(e.root[0].kind,Kind::Frac));
    assert_eq!(typst::write_cell(&e.root),"frac(dif x, 2 pi)");
    assert!(matches!(command("/").root[0].kind,Kind::Frac));
    let mut e=Editor::default();input(&mut e,"x/2");
    assert_eq!(typst::write_cell(&e.root),"frac(x, 2)");
    assert_eq!(e.cursor.slices[0].cell,1);
}

#[test]
fn alignment_preserves_empty_columns_ragged_rows_and_linebreaks() {
    let src="a &= 1 && \"given\" \\\nbb &= 2 & \"why\"";
    let e=command(src);
    assert!(matches!(&e.root[0].kind,Kind::Aligned{columns:4,row_lengths} if row_lengths==&vec![4,3]));
    assert!(e.root[0].cells[2].is_empty());assert!(e.root[0].cells[7].is_empty());
    let serialized=typst::write_cell(&e.root);
    assert_eq!(serialized,"a & = 1 &  & \"given\" \\\nbb & = 2 & \"why\"");
    assert_eq!(load(&serialized).root,e.root);
    assert_eq!(load(src).root,e.root);
    assert!(matches!(command("a \\ b").root[0].kind,Kind::Aligned{columns:1,..}));
    assert_eq!(command("\\").root[0].cells.len(),2);
    assert_eq!(command("&").root[0].cells.len(),2);
    assert_eq!(command("a \\ \\ b").root[0].cells.len(),3);
}

#[test]
fn markers_only_split_their_own_math_level() {
    let e=command("frac(a & b \\ c & d, 2)");
    assert!(matches!(e.root[0].kind,Kind::Frac));
    assert!(matches!(e.root[0].cells[0][0].kind,Kind::Aligned{columns:2,..}));
    let e=command(r#""a & b \\ c" + \&"#);
    assert!(matches!(e.root[0].kind,Kind::Text));raw(e.root.last().unwrap(),r"\&");
    assert!(!e.root.iter().any(|a|matches!(a.kind,Kind::Aligned{..})));
}

#[test]
fn alignment_navigation_and_growth_keep_row_column_positions() {
    let mut e=load("a & b \\ c & d");
    key(&mut e,"ArrowRight");assert_eq!(e.cursor.slices[0].cell,0);
    key(&mut e,"ArrowDown");assert_eq!(e.cursor.slices[0].cell,2);
    key(&mut e,"Tab");assert_eq!(e.cursor.slices[0].cell,3);
    key(&mut e,"ArrowUp");assert_eq!(e.cursor.slices[0].cell,1);
    e.apply(Action::AddColumn).unwrap();assert_eq!(e.root[0].cells.len(),6);
    e.apply(Action::AddRow).unwrap();assert_eq!(e.root[0].cells.len(),9);
    let s=typst::write_cell(&e.root);assert_eq!(load(&s).root,e.root);
    e.apply(Action::Undo).unwrap();assert_eq!(e.root[0].cells.len(),6);
}

#[test]
fn paste_opens_draft_and_copy_preserves_typst_source() {
    let mut e=load("frac(dif x, 2 pi)");
    e.apply(Action::Key{key:"a".into(),ctrl:true,shift:false}).unwrap();
    let selected=e.response().selected_source;assert_eq!(selected,"frac(dif x, 2 pi)");
    e.apply(Action::Paste{text:"sqrt(x)".into()}).unwrap();
    assert_eq!(e.pending(),Some("sqrt(x)"));key(&mut e,"Escape");
    assert_eq!(typst::write_cell(&e.root),selected);
    key(&mut e,"End");e.apply(Action::Paste{text:selected}).unwrap();
    assert!(e.pending().is_some());key(&mut e,"Enter");assert_eq!(e.root.len(),2);
    e.apply(Action::Paste{text:"$ 1/2 $".into()}).unwrap();key(&mut e,"Enter");
    assert!(matches!(e.root[2].kind,Kind::Frac));
    e.apply(Action::Paste{text:"frac(".into()}).unwrap();key(&mut e,"Enter");assert!(e.pending().is_some());
}

#[test]
fn string_sugar_survives_without_a_text_command() {
    let mut e=Editor::default();input(&mut e,r#""a >= b & c""#);
    assert!(matches!(e.root[0].kind,Kind::Text));assert!(!e.string_mode());
    key(&mut e,"Home");key(&mut e,"ArrowRight");
    e.apply(Action::Key{key:"a".into(),ctrl:true,shift:false}).unwrap();
    assert_eq!(e.response().selected_source,r#""a >= b & c""#);
    e.apply(Action::Paste{text:r#""hello""#.into()}).unwrap();assert_eq!(e.pending(),Some(r#""hello""#));
    key(&mut e,"Enter");assert_eq!(typst::write_cell(&e.root),r#""hello""#);
    raw(&command("text").root[0],"text");raw(&command("lr").root[0],"lr");
}

#[test]
fn shift_clicking_into_a_nest_clamps_the_selection_to_the_cell() {
    let mut e=load("frac(1, 2) + x");
    let end=Cursor{slices:vec![],pos:e.root.len(),occurrence:String::new()};
    e.apply(Action::Click{cursor:end,shift:false}).unwrap();
    // The anchor names a position in the outer cell, the cursor names an atom
    // inside the fraction. Widening the outer end by one used to exceed the cell.
    e.apply(Action::Click{cursor:Cursor{slices:vec![CursorSlice{atom:0,cell:0}],pos:0,occurrence:String::new()},shift:true}).unwrap();
    assert!(valid(&e.root,&e.cursor),"{:?}",e.cursor);
    assert!(e.cursor.slices.is_empty());
    assert_eq!(e.cursor.pos,e.root.len());
    assert_eq!(e.selection(),Some((0,e.root.len())));
    // A rejected selection must never leave a cursor the next keystroke cannot use.
    key(&mut e,"ArrowRight");input(&mut e,"y");
    assert_eq!(typst::write_cell(&e.root),"frac(1, 2) + x y");
}


