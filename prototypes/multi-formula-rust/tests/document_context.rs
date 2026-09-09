use multi_formula_context::{Document, Projection, ProjectionKind as Kind};

fn descendants(node: &Projection) -> Vec<&Projection> {
    let mut result = vec![node];
    for slot in &node.slots {
        for child in &slot.children { result.extend(descendants(child)); }
    }
    result
}

fn binding_body(document: &Document, formula: usize, name: &str) -> String {
    let definition = document.macros.visible(document.formulas[formula].environment, name).unwrap();
    document.text(definition.body.unwrap()).unwrap().to_owned()
}

#[test]
fn formulas_share_document_syntax_and_ordered_environments() {
    let document = Document::parse("$before$\n#let value = 1\n$value$\n#let value = 2\n$value$");
    assert!(document.diagnostics.is_empty());
    assert_eq!(document.formulas.len(), 3);
    assert!(document.macros.visible(document.formulas[0].environment, "value").is_none());
    assert_eq!(binding_body(&document, 1, "value"), "1");
    assert_eq!(binding_body(&document, 2, "value"), "2");
    for formula in &document.formulas {
        let record = document.node(formula.node).unwrap();
        let compiler_node = document.source().find(record.span).unwrap();
        assert!(std::ptr::eq(document.syntax(formula.node).unwrap(), compiler_node.get()));
        assert_eq!(formula.projection.origin, formula.node);
    }
}

#[test]
fn lexical_scope_branches_do_not_leak() {
    let document = Document::parse(
        "#let value = 1\n#[ $value$ #let value = 2\n$value$ ]\n$value$",
    );
    assert!(document.diagnostics.is_empty());
    assert_eq!(document.formulas.len(), 3);
    assert_eq!(binding_body(&document, 0, "value"), "1");
    assert_eq!(binding_body(&document, 1, "value"), "2");
    assert_eq!(binding_body(&document, 2, "value"), "1");
    assert!(document.macros.environments.iter().any(|env| env.scope.is_some()));
}

#[test]
fn function_body_is_not_a_separate_document_formula() {
    let document = Document::parse("#let twice(x) = $#x + #x$\n$twice(a)$");
    assert!(document.diagnostics.is_empty());
    assert_eq!(document.formulas.len(), 1);
    let nodes = descendants(&document.formulas[0].projection);
    assert!(nodes.iter().any(|p| p.kind == Kind::Macro));
    let args: Vec<_> = nodes.iter().filter(|p| p.kind == Kind::Text && p.text.as_deref() == Some("a")).collect();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0].origin, args[1].origin);
    assert!(args.iter().all(|p| p.editable));
    assert!(nodes.iter().any(|p| p.text.as_deref() == Some("+") && !p.editable));
}

#[test]
fn closures_expand_in_their_captured_environment() {
    let document = Document::parse("#let value = 1\n#let keep() = $value$\n#let value = 2\n$keep()$");
    assert!(document.diagnostics.is_empty());
    let nodes = descendants(&document.formulas[0].projection);
    assert!(nodes.iter().any(|p| p.kind == Kind::Text && p.text.as_deref() == Some("1")));
    assert!(!nodes.iter().any(|p| p.kind == Kind::Text && p.text.as_deref() == Some("2")));
}

#[test]
fn builtin_shadowing_and_imports_fall_back_without_guessing() {
    let shadowed = Document::parse("#let frac(x, y) = { x + y }\n$frac(a, b)$");
    let nodes = descendants(&shadowed.formulas[0].projection);
    assert!(nodes.iter().any(|p| p.kind == Kind::Raw));
    assert!(!nodes.iter().any(|p| p.kind == Kind::Fraction));
    let imported = Document::parse("#import \"other.typ\": *\n#let local = 1\n$frac(a, b) + local$");
    assert_eq!(imported.macros.definitions.len(), 1);
    assert_eq!(binding_body(&imported, 0, "local"), "1");
    assert!(!descendants(&imported.formulas[0].projection).iter().any(|p| p.kind == Kind::Fraction));
}

#[test]
fn both_fraction_spellings_project_without_roundtrip_serialization() {
    let document = Document::parse("$a/b$ and $frac(a, b)$");
    assert_eq!(document.formulas.len(), 2);
    let fractions: Vec<_> = document.formulas.iter().flat_map(|f| descendants(&f.projection))
        .filter(|p| p.kind == Kind::Fraction).collect();
    assert_eq!(fractions.len(), 2);
    assert_ne!(fractions[0].origin, fractions[1].origin);
    assert_eq!(document.text(fractions[0].origin).unwrap(), "a/b");
    assert_eq!(document.text(fractions[1].origin).unwrap(), "frac(a, b)");
}

#[test]
fn raw_stores_origin_not_a_copy_of_source() {
    let document = Document::parse("$cancel(x)$");
    let raw = descendants(&document.formulas[0].projection).into_iter().find(|p| p.kind == Kind::Raw).unwrap();
    assert!(raw.text.is_none());
    assert!(raw.slots.is_empty());
    assert_eq!(document.text(raw.origin).unwrap(), "cancel(x)");
    assert_eq!(&document.source().text()[raw.range.clone()], "cancel(x)");
}

#[test]
fn holes_preserve_display_delimiters_and_unrelated_source() {
    let mut document = Document::parse("正文 // 保留注释\n$ \"\" $\n尾部");
    let formula = &document.formulas[0];
    assert!(formula.display);
    assert!(formula.projection.slots[0].hole);
    let range = formula.body.clone();
    assert_eq!(&document.source().text()[range.clone()], "\"\"");
    document.edit(document.revision(), range, "frac(\"\", \"\")").unwrap();
    assert_eq!(document.source().text(), "正文 // 保留注释\n$ frac(\"\", \"\") $\n尾部");
    assert!(document.formulas[0].display);
    let fraction = descendants(&document.formulas[0].projection).into_iter().find(|p| p.kind == Kind::Fraction).unwrap();
    assert!(fraction.slots.iter().all(|s| s.hole));
    let range = fraction.slots[0].range.clone();
    document.edit(document.revision(), range, "x").unwrap();
    assert_eq!(document.source().text(), "正文 // 保留注释\n$ frac(x, \"\") $\n尾部");
}

#[test]
fn edits_reject_foreign_or_stale_ids_and_invalid_utf8_offsets() {
    let mut first = Document::parse("中文 $x$");
    let second = Document::parse("$x$");
    let old = first.formulas[0].node;
    assert!(first.node(second.formulas[0].node).is_err());
    assert!(first.edit(first.revision(), 1..2, "").is_err());
    let revision = first.revision();
    first.edit(revision, 0..0, "新增").unwrap();
    assert!(first.node(old).is_err());
    assert!(first.edit(revision, 0..0, "旧修改").is_err());
}

#[test]
fn invalid_draft_is_preserved_then_can_be_repaired() {
    let mut document = Document::parse("$x$");
    document.replace("$frac(");
    assert!(!document.diagnostics.is_empty());
    assert_eq!(document.source().text(), "$frac(");
    assert!(document.formulas.iter().all(|formula| !formula.valid));
    document.replace("$frac(x, y)$");
    assert!(document.diagnostics.is_empty());
    assert_eq!(document.formulas.len(), 1);
}

#[test]
fn document_clone_for_compiler_is_an_immutable_revision_snapshot() {
    let mut document = Document::parse("$a$");
    let snapshot = document.source().clone();
    document.edit(0, 1..2, "b").unwrap();
    assert_eq!(snapshot.text(), "$a$");
    assert_eq!(document.source().text(), "$b$");
}

#[test]
fn dynamic_assignment_disables_unsafe_static_expansion() {
    let document = Document::parse("#let value = 1\n#{ value = 2 }\n$value$");
    assert!(document.macros.uncertain(document.formulas[0].environment));
    assert!(document.macros.visible(document.formulas[0].environment, "value").is_none());
}

#[test]
fn cache_identity_survives_offsets_and_edits_but_is_not_a_syntax_id() {
    let mut document = Document::parse("$x$ and $x$");
    let first = document.formulas[0].projection.cache_id;
    let second = document.formulas[1].projection.cache_id;
    let old_node = document.formulas[1].node;
    assert_ne!(first, second);
    document.edit(0, 0..0, "中文 ").unwrap();
    assert_eq!(document.formulas[0].projection.cache_id, first);
    assert_eq!(document.formulas[1].projection.cache_id, second);
    assert!(document.node(old_node).is_err());
    let body = document.formulas[0].body.clone();
    document.edit(document.revision(), body, "frac(a, b)").unwrap();
    assert_eq!(document.formulas[0].projection.cache_id, first);
    assert_eq!(document.formulas[1].projection.cache_id, second);
    let range = document.formulas[0].projection.range.clone();
    document.edit(document.revision(), range, "").unwrap();
    assert_eq!(document.formulas.len(), 1);
    assert_eq!(document.formulas[0].projection.cache_id, second);
}

#[test]
fn malformed_binding_does_not_clear_recoverable_formulas() {
    let document = Document::parse("$x$\n#let broken = ;\n$y$");
    assert!(!document.diagnostics.is_empty());
    assert!(document.formulas.iter().any(|f| f.valid && document.text(f.node).unwrap() == "$x$"));
    assert!(document.formulas.iter().any(|f| f.valid && document.text(f.node).unwrap() == "$y$"));
}

#[test]
fn quoted_text_retains_its_semantic_style() {
    let document = Document::parse("$x + \"x\"$");
    let leaves: Vec<_> = descendants(&document.formulas[0].projection).into_iter()
        .filter(|p| p.text.as_deref() == Some("x")).collect();
    assert!(leaves.iter().any(|p| p.literal));
    assert!(leaves.iter().any(|p| !p.literal));
}
