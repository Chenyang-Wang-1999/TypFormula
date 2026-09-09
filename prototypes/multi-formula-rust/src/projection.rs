//! A UI projection of existing Typst syntax, not a second language AST.
//! Raw nodes carry only an origin ID. Text below is a display hint for known
//! literals/symbols; serialization always reads or patches the original Source.
use std::{collections::HashMap, ops::Range, sync::LazyLock};
use serde::Serialize;
use typst::{Library, LibraryExt, foundations::Value};
use typst_syntax::{ast::{self, AstNode}, SyntaxNode};
use crate::{Document, NodeId, macros::EnvironmentId};

static LIBRARY: LazyLock<Library> = LazyLock::new(Library::default);
const MAX_NODES: usize = 4096;
const MAX_DEPTH: usize = 40;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionKind { Row, Text, Symbol, Raw, Fraction, Root, Script, Delimiter, Matrix, Decoration, Macro }

#[derive(Clone, Debug, Serialize)]
pub struct Projection {
    pub kind: ProjectionKind,
    pub origin: NodeId,
    pub cache_id: u64,
    pub range: Range<usize>,
    pub text: Option<String>,
    pub slots: Vec<Slot>,
    pub columns: usize,
    pub editable: bool,
    /// Quoted strings stay upright; mathematical letters use math glyphs.
    pub literal: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Slot {
    pub role: String,
    pub origin: NodeId,
    pub range: Range<usize>,
    /// An empty string is the valid Typst spelling of an editor hole. Typing
    /// into it replaces the quoted placeholder rather than editing a string.
    pub hole: bool,
    pub editable: bool,
    pub children: Vec<Projection>,
}

impl Projection {
    pub fn raw(doc: &Document, node: &SyntaxNode) -> Self {
        Self { kind: ProjectionKind::Raw, origin: doc.id_for(node), cache_id: doc.node(doc.id_for(node)).unwrap().cache_id,
            range: doc.range_of(node), text: None, slots: vec![], columns: 0, editable: true, literal: false }
    }
    pub fn origins(&self, out: &mut Vec<NodeId>) {
        if self.kind == ProjectionKind::Raw { out.push(self.origin); }
        // Attachment geometry is read from the same completed layout.
        if self.kind == ProjectionKind::Script {
            out.push(self.origin);
            out.extend(self.slots.iter().map(|s| s.origin));
        }
        for slot in &self.slots { for node in &slot.children { node.origins(out); } }
    }
}

pub fn formula(doc: &Document, equation: ast::Equation, env: EnvironmentId) -> Projection {
    let mut builder = Builder { doc, remaining: MAX_NODES };
    let mut root = Projection::raw(doc, equation.to_untyped());
    root.kind = ProjectionKind::Row;
    if equation.to_untyped().diagnosis().errors {
        // Retain a local repair surface and every other recoverable formula.
        let range = root.range.clone();
        root.slots = vec![Slot { role: "body".into(), origin: root.origin, range,
            hole: false, editable: true, children: vec![] }];
    } else {
        root.slots = vec![builder.slot("body", equation.body().to_untyped(), env, &HashMap::new(), false, 0)];
    }
    root
}

struct Builder<'a> { doc: &'a Document, remaining: usize }

impl Builder<'_> {
    fn slot(&mut self, role: &str, node: &SyntaxNode, env: EnvironmentId,
            substitutions: &HashMap<String, Projection>, template: bool, depth: usize) -> Slot {
        let hole = node.cast::<ast::Str>().is_some_and(|s| s.get().is_empty()) ||
            node.cast::<ast::Math>().is_some_and(|m| {
                let expressions: Vec<_> = m.exprs().collect();
                expressions.len() == 1 && matches!(expressions[0], ast::Expr::Str(s) if s.get().is_empty())
            });
        let mut range = self.doc.range_of(node);
        if let Some(math) = node.cast::<ast::Math>() {
            let spelling = &self.doc.source().text()[range.clone()];
            if math.was_deparenthesized() && spelling.starts_with('(') && spelling.ends_with(')') {
                range.start += 1;
                range.end -= 1;
            }
            let spelling = &self.doc.source().text()[range.clone()];
            let leading = spelling.len() - spelling.trim_start().len();
            let trailing = spelling.len() - spelling.trim_end().len();
            range.start += leading;
            range.end = range.end.saturating_sub(trailing).max(range.start);
        }
        let children = if hole { vec![] }
            else if let Some(math) = node.cast::<ast::Math>() {
                math.exprs().map(|e| self.expression(e.to_untyped(), env, substitutions, template, depth + 1)).collect()
            } else { vec![self.expression(node, env, substitutions, template, depth + 1)] };
        Slot { role: role.into(), origin: self.doc.id_for(node), range, hole, editable: !template, children }
    }

    fn expression(&mut self, node: &SyntaxNode, env: EnvironmentId,
                  substitutions: &HashMap<String, Projection>, template: bool, depth: usize) -> Projection {
        let spelling = self.doc.text(self.doc.id_for(node)).unwrap();
        if let Some(replacement) = substitutions.get(spelling) {
            fn size(node: &Projection) -> usize {
                1 + node.slots.iter().flat_map(|s| &s.children).map(size).sum::<usize>()
            }
            let count = size(replacement);
            if count <= self.remaining { self.remaining -= count; return replacement.clone(); }
            let mut raw = Projection::raw(self.doc, node);
            raw.editable = false;
            return raw;
        }
        let mut out = Projection::raw(self.doc, node);
        out.editable = !template;
        if self.remaining == 0 || depth > MAX_DEPTH { return out; }
        self.remaining -= 1;
        let Some(expr) = node.cast::<ast::Expr>() else { return out; };
        let mut slots: Vec<(&str, &SyntaxNode)> = vec![];
        match expr {
            ast::Expr::Equation(eq) => {
                out.kind = ProjectionKind::Row;
                slots.push(("body", eq.body().to_untyped()));
            }
            ast::Expr::Math(_) => {
                out.kind = ProjectionKind::Row;
                slots.push(("body", node));
            }
            ast::Expr::MathFrac(frac) => {
                out.kind = ProjectionKind::Fraction;
                slots.extend([("numerator", frac.num().to_untyped()), ("denominator", frac.denom().to_untyped())]);
            }
            ast::Expr::MathAttach(attach) if attach.primes().is_none() => {
                out.kind = ProjectionKind::Script;
                slots.push(("base", attach.base().to_untyped()));
                if let Some(top) = attach.top() { slots.push(("upper", top.to_untyped())); }
                if let Some(bottom) = attach.bottom() { slots.push(("lower", bottom.to_untyped())); }
            }
            ast::Expr::MathRoot(root) => {
                out.kind = ProjectionKind::Root;
                out.text = Some(root.index().map(|i| i.to_string()).unwrap_or_default());
                slots.push(("radicand", root.radicand().to_untyped()));
            }
            ast::Expr::MathDelimited(delimiter) => {
                out.kind = ProjectionKind::Delimiter;
                // The original delimiter tokens remain available for writing.
                out.text = Some(format!("{}{}", spelling.chars().next().unwrap_or('('), spelling.chars().last().unwrap_or(')')));
                let mut slot = self.slot("body", delimiter.body().to_untyped(), env, substitutions, template, depth + 1);
                let open = self.doc.id_for(delimiter.open().to_untyped());
                let close = self.doc.id_for(delimiter.close().to_untyped());
                slot.children.retain(|p| p.origin != open && p.origin != close);
                slot.range = self.doc.range_of(delimiter.open().to_untyped()).end..self.doc.range_of(delimiter.close().to_untyped()).start;
                out.slots.push(slot);
                return out;
            }
            ast::Expr::MathCall(call) => {
                let name = self.doc.text(self.doc.id_for(call.callee().to_untyped())).unwrap();
                let mut args = vec![];
                let mut widths = vec![];
                let mut width = 0;
                for item in call.args().arg_items() {
                    let ast::Arg::Pos(arg) = item.arg else { return out; };
                    args.push(arg.to_untyped());
                    width += 1;
                    if item.ends_in_semicolon { widths.push(width); width = 0; }
                }
                if width > 0 { widths.push(width); }
                if let Some(definition) = self.doc.macros.visible(env, name) {
                    if !definition.function || !definition.expandable || definition.parameters.len() != args.len() { return out; }
                    let captured = definition.captured;
                    let body = definition.body.unwrap();
                    let parameters = definition.parameters.clone();
                    let mut bound = HashMap::new();
                    for (parameter, arg) in parameters.into_iter().zip(args) {
                        let mut group = Projection::raw(self.doc, arg);
                        group.kind = ProjectionKind::Row;
                        group.slots = vec![self.slot("argument", arg, env, substitutions, template, depth + 1)];
                        bound.insert(parameter, group);
                    }
                    let body = self.doc.syntax(body).unwrap();
                    out.kind = ProjectionKind::Macro;
                    out.text = Some(name.into());
                    let expanded = self.expression(body, captured, &bound, true, depth + 1);
                    out.slots = vec![Slot { role: "expansion".into(), origin: out.origin, range: out.range.clone(), hole: false,
                        editable: false, children: vec![expanded] }];
                    return out;
                }
                // An import or dynamic assignment can shadow an apparent builtin.
                if self.doc.macros.uncertain(env) { return out; }
                match (name, args.len()) {
                    ("frac", 2) => { out.kind = ProjectionKind::Fraction; slots.extend([("numerator", args[0]), ("denominator", args[1])]); }
                    ("sqrt", 1) => { out.kind = ProjectionKind::Root; slots.push(("radicand", args[0])); }
                    ("root", 2) => { out.kind = ProjectionKind::Root; slots.extend([("index", args[0]), ("radicand", args[1])]); }
                    ("mat", n) if n > 0 && widths.iter().all(|w| *w == widths[0]) => {
                        out.kind = ProjectionKind::Matrix;
                        out.columns = widths[0];
                        slots.extend(args.into_iter().map(|arg| ("cell", arg)));
                    }
                    ("abs", 1) | ("norm", 1) => {
                        out.kind = ProjectionKind::Delimiter;
                        out.text = Some(if name == "abs" { "||" } else { "‖‖" }.into());
                        slots.push(("body", args[0]));
                    }
                    ("overline" | "underline" | "hat" | "vec", 1) => {
                        out.kind = ProjectionKind::Decoration; out.text = Some(name.into()); slots.push(("body", args[0]));
                    }
                    _ => return out,
                }
            }
            ast::Expr::MathIdent(_) | ast::Expr::Ident(_) => {
                if let Some(definition) = self.doc.macros.visible(env, spelling) {
                    if definition.function || !definition.expandable { return out; }
                    let body = self.doc.syntax(definition.body.unwrap()).unwrap();
                    let expanded = self.expression(body, definition.captured, &HashMap::new(), true, depth + 1);
                    out.kind = ProjectionKind::Macro; out.text = Some(spelling.into());
                    out.slots = vec![Slot { role: "expansion".into(), origin: out.origin, range: out.range.clone(), hole: false,
                        editable: false, children: vec![expanded] }];
                    return out;
                }
                if !self.doc.macros.uncertain(env) {
                    if let Some(binding) = LIBRARY.math.scope().get(spelling) {
                        if let Value::Symbol(symbol) = binding.read() {
                            out.kind = ProjectionKind::Symbol;
                            out.text = Some(symbol.get().into());
                        }
                    }
                }
            }
            ast::Expr::Str(text) => { out.kind = ProjectionKind::Text; out.literal = true; out.text = Some(text.get().into()); }
            ast::Expr::MathText(_) | ast::Expr::Int(_) | ast::Expr::Float(_) => {
                out.kind = ProjectionKind::Text; out.text = Some(spelling.into());
            }
            ast::Expr::MathShorthand(_) | ast::Expr::Escape(_) => {
                // Let the engine resolve symbol aliases; never guess a glyph.
                return out;
            }
            _ => return out,
        }
        out.slots = slots.into_iter().map(|(role, child)| self.slot(role, child, env, substitutions, template, depth + 1)).collect();
        out
    }
}
