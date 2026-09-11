// SPDX-License-Identifier: GPL-2.0-or-later
use visual_typst_core::{Action,Editor,math::*,typst};
fn key(e:&mut Editor,k:&str){e.apply(Action::Key{key:k.into(),shift:false,ctrl:false}).unwrap();}
fn input(e:&mut Editor,s:&str){e.apply(Action::Input{text:s.into()}).unwrap();}
fn command(s:&str)->Editor{let mut e=Editor::default();input(&mut e,&format!("\\{s}"));key(&mut e,"Enter");assert!(e.pending().is_none(),"{s}: {}",e.message);e}
fn load(s:&str)->Editor{let mut e=Editor::default();e.apply(Action::Import{source:s.into()}).unwrap();e}
fn raw(a:&MathAtom,s:&str){assert!(matches!(&a.kind,Kind::Raw{source} if source==s),"{a:?}");}
/// The spelling of one `Number`: the characters of its single cell.
fn run(a:&MathAtom)->String{
    assert!(matches!(a.kind,Kind::Number),"{a:?}");
    a.cells[0].iter().map(|c|if let Kind::Char{text}=&c.kind {text.clone()} else {format!("{c:?}")}).collect()
}

#[test]
fn nested_raw_keeps_its_editable_fraction_and_source() {
    let mut e=command("frac(dif x, 2 pi)");
    assert!(matches!(e.root[0].kind,Kind::Fraction));raw(&e.root[0].cells[0][0],"dif");
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
    for s in ["x"] { assert!(command(s).root.iter().all(|a|matches!(a.kind,Kind::Char{..}))); }
    // A run of digits is one `Number` holding them in one cell, which is how both
    // the lexer and the engine treat it: one token, one `NumberItem`.
    assert_eq!(command("123").root.len(),1);
    assert_eq!(run(&command("123").root[0]),"123");
    for s in ["cancel( x/y  + dif x )", "lr((x), size: #150%)", "text(\"hello\")"] {
        raw(&command(s).root[0],s);raw(&load(s).root[0],s);
    }
    let mut e=Editor::default();input(&mut e,">=");assert_eq!(e.root.len(),2);assert!(e.root.iter().all(|a|matches!(a.kind,Kind::Char{..})));
    // One separator: joined, `>=` would lex as a single shorthand token.
    assert_eq!(typst::write_cell(&e.root),"> =");
}

#[test]
fn ordinary_input_keeps_char_nodes_and_only_maps_single_character_display() {
    let mut e=Editor::default();input(&mut e,"<=");let previous=e.root.clone();input(&mut e,"+");
    assert_eq!(e.root.len(),3);assert_eq!(&e.root[..2],previous.as_slice());
    assert!(e.root.iter().all(|a|matches!(a.kind,Kind::Char{..})));
    assert_eq!(typst::write_cell(&e.root),"< = +");
    key(&mut e,"Backspace");assert_eq!(e.root,previous);
    let mut e=command("<=");let symbol=e.root[0].clone();input(&mut e,"+");
    assert_eq!(e.root[0],symbol);assert!(matches!(&e.root[1].kind,Kind::Char{text} if text=="+"));
    let mut e=Editor::default();input(&mut e,"times");assert_eq!(e.root.len(),5);
    assert!(e.root.iter().all(|a|matches!(a.kind,Kind::Char{..})));
    input(&mut e,"\\times");key(&mut e,"Enter");assert!(matches!(&e.root[5].kind,Kind::Symbol{name,..} if name=="times"));
    let mut e=Editor::default();input(&mut e,"-*");
    let root=e.root.clone();let response=e.response();
    let chars:Vec<_>=response.view.children.iter().filter(|v|v.kind=="char").collect();
    assert_eq!(chars[0].text,"-");assert_eq!(chars[0].display_glyph.as_deref(),Some("−"));
    assert_eq!(chars[1].text,"*");assert_eq!(chars[1].display_glyph.as_deref(),Some("∗"));
    assert_eq!(e.root,root);assert_eq!(typst::write_cell(&e.root),"- *");
}

#[test]
fn a_separator_keeps_typed_characters_from_becoming_one_shorthand() {
    // Without the separator, `- >` would be written as `->` and re-read as the
    // arrow shorthand, so the two typed characters would come back as one Raw.
    // The same holds for `||` (`‖`), `...` (`…`), `:=` (`≔`) and `<=`.
    for (typed,written) in [("->","- >"),("||","| |"),("...",". . ."),(":=",": ="),("<=","< =")] {
        let mut e=Editor::default();input(&mut e,typed);
        assert!(e.root.iter().all(|a|matches!(a.kind,Kind::Char{..})),"{typed}: {:?}",e.root);
        assert_eq!(typst::write_cell(&e.root),written,"{typed}");
        let reparsed=load(&written);
        assert_eq!(reparsed.root.len(),e.root.len(),"{written} must stay separate atoms");
        assert_eq!(typst::write_cell(&reparsed.root),written,"{written} must be stable");
    }
    // A run keeps its digits in one cell, so the writer puts no separator inside it
    // (`Write::Run`); only the separator between atoms is left. A dot typed in
    // normal mode is an ordinary character and gets that separator, while a dot read
    // from source joins its digits, because the lexer keeps `12.5` in one token.
    //
    // The two spellings render identically -- measured with the real adapter,
    // `1.5` and `1 . 5` are both 30.672 x 16.512pt at 24pt -- so neither is a
    // loss, but a typed decimal and a written one are not the same tree.
    for (typed,written) in [("12.5","12 . 5"),(".5",". 5"),("1..2","1 . . 2")] {
        let mut e=Editor::default();input(&mut e,typed);
        assert_eq!(typst::write_cell(&e.root),written,"{typed}");
        assert_eq!(load(&written).root,e.root,"{written} must read back as the tree that wrote it");
    }
    for source in ["12.5","123.456","0.5"] {
        assert_eq!(load(source).root.len(),1,"{source}");
        assert_eq!(run(&load(source).root[0]),source);
    }
    // The rule is the engine's, so it is ASCII digits only. `²3` is one lexer
    // token (`char::is_numeric` is true for `²`) but `resolve_text` rejects it and
    // the engine makes a `Text` of it, so it must not become a `Number` here.
    assert!(load("²3").root.iter().all(|a|!matches!(a.kind,Kind::Number{..})),"{:?}",load("²3").root);
}

#[test]
fn a_typed_digit_joins_the_number_run_beside_it() {
    // `123` then `4` is one run, in the tree and therefore in the source.
    let mut e=Editor::default();input(&mut e,"123");input(&mut e,"4");
    assert_eq!(e.root.len(),1);assert_eq!(run(&e.root[0]),"1234");
    assert_eq!(typst::write_cell(&e.root),"1234");
    // A decimal that came from source keeps its dot when a digit is appended.
    let mut e=load("123.456");key(&mut e,"End");input(&mut e,"7");
    assert_eq!(typst::write_cell(&e.root),"123.4567");
    // Command mode hands the draft to the parser, so a dot belongs to the run.
    assert_eq!(typst::write_cell(&command("123.456").root),"123.456");
    // A dot typed in normal mode never extends the run.
    let mut e=Editor::default();input(&mut e,"123");input(&mut e,".");input(&mut e,"456");
    assert_eq!(typst::write_cell(&e.root),"123 . 456");
}

#[test]
fn the_caret_goes_inside_a_number_run() {
    // A run is a container, so the caret reaches *between* its digits -- which a
    // leaf could not express at all: there was no way to put a `9` after the `2`
    // of `1234`. Entering takes one move (which lands before the first digit), so
    // three moves put the caret between the `2` and the `3`.
    let mut e=load("1234");
    for _ in 0..3 { key(&mut e,"ArrowRight"); }
    assert_eq!(e.cursor.slices.len(),1,"the caret is inside the run");
    assert_eq!(e.cursor.pos,2);
    input(&mut e,"9");
    assert_eq!(typst::write_cell(&e.root),"12934","the digit goes in at the caret");
    assert_eq!(e.cursor.pos,3,"and the caret follows it");
    // A run on either side of the caret takes the digit, and the caret stays inside
    // it: typed before `456`, `9` then `8` reads `98456`, not `89456`.
    let mut e=load("456");input(&mut e,"9");input(&mut e,"8");
    assert_eq!(e.root.len(),1);assert_eq!(run(&e.root[0]),"98456");
    assert_eq!(typst::write_cell(&e.root),"98456");
    // A run is not a trap: left/right walk out of it, unlike a text run, whose
    // boundaries swallow the arrows.
    let mut e=load("12");key(&mut e,"ArrowRight");key(&mut e,"ArrowLeft");
    assert!(e.cursor.slices.is_empty(),"left leaves the run:{:?}",e.cursor);
    // Anything that is not a digit ends the run at the caret instead of landing in
    // it, and keeps its usual meaning there.
    let mut e=load("1234");for _ in 0..3 { key(&mut e,"ArrowRight"); }input(&mut e,"x");
    assert_eq!(typst::write_cell(&e.root),"12 x 34");
    assert_eq!(e.root.iter().map(|a|a.kind.clone()).collect::<Vec<_>>(),
               vec![Kind::Number,Kind::Char{text:"x".into()},Kind::Number]);
    assert_eq!(e.cursor.pos,2,"the caret stays where the user put it");
    // A character that opens a structure keeps its own rule at the split point.
    // `/` takes the atom on its left as the numerator and moves the caret into the
    // empty denominator -- the same thing it does between any two atoms -- so the
    // right half of the run stays after the fraction.
    let mut e=load("1234");for _ in 0..3 { key(&mut e,"ArrowRight"); }input(&mut e,"/");
    assert_eq!(typst::write_cell(&e.root),r#"frac(12, "") 34"#);
    assert_eq!(e.cursor.slices.last().unwrap().cell,1,"the caret is the denominator");
    input(&mut e,"7");
    assert_eq!(typst::write_cell(&e.root),r#"frac(12, 7) 34"#);
    // Deleting the last digit removes the run rather than leaving an empty one,
    // which would be written as a stray separator. The spaces matter: `x1y` is one
    // identifier to the lexer, so the run has to stand alone between two atoms.
    let mut e=load("x 1 y");for _ in 0..3 { key(&mut e,"ArrowRight"); }key(&mut e,"Backspace");
    assert_eq!(typst::write_cell(&e.root),"x y");
    assert!(e.root.iter().all(|a|!matches!(a.kind,Kind::Number)),"{:?}",e.root);
    // Backspace at the *start* of the run is the ordinary cell rule (LyX's
    // pullArg): the container dissolves and keeps its content, so the digit becomes
    // a loose character. It renders identically (`1 2 3` measured the same as `123`)
    // and the next parse of the formula forms the run again.
    let mut e=load("x 1 y");key(&mut e,"ArrowRight");key(&mut e,"ArrowRight");key(&mut e,"Backspace");
    assert_eq!(typst::write_cell(&e.root),"x 1 y");
    assert!(e.root.iter().all(|a|!matches!(a.kind,Kind::Number)),"{:?}",e.root);
}

#[test]
fn one_character_is_one_grapheme_cluster() {
    // A reader's "character" is not a Unicode scalar: `é` can be `e` plus a
    // combining accent, and an emoji can be several scalars joined by ZWJ. The
    // lexer keeps one cluster in one token and `GlyphItem` holds one cluster, so
    // the editor does too.
    //
    // Before this, the parse split by scalar: `e` and its accent became two
    // `Char`s, the writer put its separator between them, and one keystroke turned
    // `é` into `e`, a space, the typed letter and a floating accent -- measured as
    // `0x65 0x20 0x7a 0x20 0x301` for `é` plus `z`.
    for cluster in ["e\u{301}", "\u{1f44d}\u{1f3fd}", "\u{1f468}\u{200d}\u{1f469}"] {
        let mut e=load(&format!("${cluster}$"));
        assert_eq!(e.root.len(),1,"{cluster}: one node");
        assert_eq!(e.root[0].kind,Kind::Char{text:cluster.into()},"{cluster}");
        // Editing must not split it: a keystroke rewrites the formula in the
        // writer's own spelling, which is where a separator would appear.
        key(&mut e,"End");input(&mut e,"z");
        let written=typst::write_cell(&e.root);
        assert_eq!(written,format!("{cluster} z"),"{cluster}: 字形簇不能被拆开");
        assert_eq!(load(&written).root,e.root,"{cluster}: 写出后必须读回同一棵树");
    }
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
    let e=command("(dif x)/ (2 pi)");assert!(matches!(e.root[0].kind,Kind::Fraction));
    assert_eq!(typst::write_cell(&e.root),"frac(dif x, 2 pi)");
    assert!(matches!(command("/").root[0].kind,Kind::Fraction));
    let mut e=Editor::default();input(&mut e,"x/2");
    assert_eq!(typst::write_cell(&e.root),"frac(x, 2)");
    assert_eq!(e.cursor.slices[0].cell,1);
}

#[test]
fn alignment_preserves_empty_columns_ragged_rows_and_linebreaks() {
    let src="a &= 1 && \"given\" \\\nbb &= 2 & \"why\"";
    let e=command(src);
    assert!(matches!(&e.root[0].kind,Kind::Multiline{columns:4,row_lengths} if row_lengths==&vec![4,3]));
    assert!(e.root[0].cells[2].is_empty());assert!(e.root[0].cells[7].is_empty());
    let serialized=typst::write_cell(&e.root);
    assert_eq!(serialized,"a & = 1 &  & \"given\" \\\nbb & = 2 & \"why\"");
    assert_eq!(load(&serialized).root,e.root);
    assert_eq!(load(src).root,e.root);
    assert!(matches!(command("a \\ b").root[0].kind,Kind::Multiline{columns:1,..}));
    assert_eq!(command("\\").root[0].cells.len(),2);
    assert_eq!(command("&").root[0].cells.len(),2);
    assert_eq!(command("a \\ \\ b").root[0].cells.len(),3);
}

#[test]
fn markers_only_split_their_own_math_level() {
    let e=command("frac(a & b \\ c & d, 2)");
    assert!(matches!(e.root[0].kind,Kind::Fraction));
    assert!(matches!(e.root[0].cells[0][0].kind,Kind::Multiline{columns:2,..}));
    let e=command(r#""a & b \\ c" + \&"#);
    assert!(matches!(e.root[0].kind,Kind::Text));raw(e.root.last().unwrap(),r"\&");
    assert!(!e.root.iter().any(|a|matches!(a.kind,Kind::Multiline{..})));
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
    assert!(matches!(e.root[2].kind,Kind::Fraction));
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


