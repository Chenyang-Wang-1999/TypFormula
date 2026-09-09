// SPDX-License-Identifier: GPL-2.0-or-later
// Typst replaces MathParser/TeXMathStream at import and command confirmation.
// Structural editing and draft keystrokes do not reparse the formula.
use crate::math::*;
use typst_syntax::{Source, SyntaxKind, SyntaxNode, ast::{self, AstNode}};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc};

pub const PROJECTION_LIMIT: usize = 4096;
const SIZE_CAP: usize = PROJECTION_LIMIT + 1;
#[derive(Clone, Default)]
struct TemplateSize {
    fixed: usize,
    params: Vec<usize>,
    depth: usize,
}
#[derive(Clone, serde::Serialize)]
pub struct MacroDefinition {
    pub source: String,
    pub name: String,
    pub params: Vec<String>,
    pub expandable: bool,
    pub shadowed: bool,
    pub reason: String,
    #[serde(skip)]
    pub template: Rc<MathData>,
    #[serde(skip)]
    pub context: Rc<String>,
    #[serde(skip)]
    pub function: bool,
    #[serde(skip)]
    names: Vec<String>,
    #[serde(skip)]
    input: String,
    #[serde(skip)]
    size: TemplateSize,
}
// The name table describes the current scope. TemplateCall edges instead point
// to immutable definition versions, including ones subsequently shadowed.
#[derive(Clone, Copy)]
enum Binding { Expandable(usize), Opaque(usize) }
#[derive(Default)]
pub struct MacroRegistry {
    pub entries: Vec<MacroDefinition>,
    names: HashMap<String, Binding>,
}
impl MacroRegistry {
    pub fn is_bound(&self, name: &str) -> bool { self.names.contains_key(name) }
    pub fn get(&self, name: &str) -> Option<&MacroDefinition> {
        let (Binding::Expandable(index) | Binding::Opaque(index)) = self.names.get(name)?;
        self.entries.get(*index)
    }
    fn expandable(&self, name: &str) -> Option<(usize, &MacroDefinition)> {
        let Binding::Expandable(index) = self.names.get(name)? else { return None; };
        Some((*index, &self.entries[*index]))
    }
    fn register(&mut self, definition: MacroDefinition) {
        let index = self.entries.len();
        let binding = if definition.expandable { Binding::Expandable(index) } else { Binding::Opaque(index) };
        for name in &definition.names { self.names.insert(name.clone(), binding); }
        self.entries.push(definition);
    }
}
thread_local! {
    // Retain one definition set. Prefix templates are reused on source edits;
    // removed suffixes are freed once no caller holds the previous registry.
    static MACROS: RefCell<Option<(String, Rc<MacroRegistry>)>> = const { RefCell::new(None) };
}
pub fn macro_registry(definitions: &str) -> Rc<MacroRegistry> {
    MACROS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((key, registry)) = cache.as_ref() {
            if key == definitions { return registry.clone(); }
        }
        let previous = cache.as_ref().map(|(_, registry)| registry.as_ref());
        let registry = Rc::new(analyze_macros(definitions, previous));
        *cache = Some((definitions.to_string(), registry.clone()));
        registry
    })
}
struct ParseContext<'a> {
    params: &'a [String],
    locals: &'a HashSet<String>,
    registry: &'a MacroRegistry,
    template: bool,
}
impl ParseContext<'_> {
    fn is_bound(&self, name: &str) -> bool { self.locals.contains(name) || self.registry.is_bound(name) }
    fn resolve(&self, name: &str) -> Option<(usize, &MacroDefinition)> {
        if self.locals.contains(name) { None } else { self.registry.expandable(name) }
    }
    fn call(&self, definition: usize, def: &MacroDefinition, cells: Vec<MathData>) -> MathAtom {
        let kind = if self.template { Kind::TemplateCall { definition } }
            else { Kind::MacroCall { name: def.name.clone(), function: def.function } };
        MathAtom { kind, cells }
    }
}
fn capped_add(a: usize, b: usize) -> usize { a.saturating_add(b).min(SIZE_CAP) }
fn capped_mul(a: usize, b: usize) -> usize { a.saturating_mul(b).min(SIZE_CAP) }
// A template's output size is an affine function of its argument sizes.
// Compose those functions once at registration instead of traversing a shared
// dependency graph repeatedly (which could itself take exponential time).
fn template_size(data: &MathData, registry: &MacroRegistry, arity: usize) -> TemplateSize {
    let mut out = TemplateSize { fixed: 1, params: vec![0; arity], depth: 0 };
    for atom in data {
        if let Kind::Parameter { index } = atom.kind { out.params[index] = capped_add(out.params[index], 1); continue; }
        if let Kind::TemplateCall { definition } = atom.kind {
            let callee = &registry.entries[definition].size;
            out.fixed = capped_add(out.fixed, callee.fixed);
            out.depth = out.depth.max(callee.depth.saturating_add(1));
            for (i, cell) in atom.cells.iter().enumerate() {
                let arg = template_size(cell, registry, arity);
                out.fixed = capped_add(out.fixed, capped_mul(callee.params[i], arg.fixed));
                for (weight, part) in out.params.iter_mut().zip(arg.params) { *weight = capped_add(*weight, capped_mul(callee.params[i], part)); }
                out.depth = out.depth.max(callee.depth.saturating_add(arg.depth).saturating_add(1));
            }
        } else {
            out.fixed = capped_add(out.fixed, 1);
            for cell in &atom.cells {
                let part = template_size(cell, registry, arity);
                out.fixed = capped_add(out.fixed, part.fixed);
                for (weight, part) in out.params.iter_mut().zip(part.params) { *weight = capped_add(*weight, part); }
                out.depth = out.depth.max(part.depth);
            }
        }
    }
    out
}
pub fn projection_size(atom: &MathAtom, registry: &MacroRegistry, limit: usize) -> usize {
    fn size(atom: &MathAtom, registry: &MacroRegistry, limit: usize, depth: usize) -> usize {
        if depth > 64 { return limit + 1; }
        if let Kind::MacroCall { name, .. } = &atom.kind {
            if let Some((_, def)) = registry.expandable(name) {
                if depth.saturating_add(def.size.depth) > 64 { return limit + 1; }
                let mut out = def.size.fixed;
                for (weight, cell) in def.size.params.iter().zip(&atom.cells) {
                    let cost = cell.iter().fold(1usize, |n,a| n.saturating_add(size(a, registry, limit, depth+1)).min(limit+1));
                    out = out.saturating_add(weight.saturating_mul(cost)).min(limit+1);
                }
                return out;
            }
        }
        atom.cells.iter().flatten().fold(1usize, |n,a| n.saturating_add(size(a, registry, limit, depth+1)).min(limit+1))
    }
    size(atom, registry, limit, 0)
}
fn parameter(node: &SyntaxNode, params: &[String]) -> Option<usize> {
    if !matches!(node.kind(), SyntaxKind::Ident | SyntaxKind::MathIdent) { return None; }
    params.iter().position(|p| p == node.full_text().as_str())
}
fn has_reference(node: &SyntaxNode, params: &[String]) -> bool {
    parameter(node, params).is_some() || node.children().any(|n| has_reference(n, params))
}
pub fn validate_definitions(text: &str) -> Result<(), String> {
    let source = Source::detached(text.to_string());
    if let Some(error) = source.root().errors_and_warnings().0.first() { return Err(error.message.to_string()); }
    if source.root().children().any(|n| !matches!(n.kind(), SyntaxKind::LetBinding | SyntaxKind::Space | SyntaxKind::Parbreak | SyntaxKind::Hash | SyntaxKind::LineComment | SyntaxKind::BlockComment)) {
        return Err("宏列表只支持 #let 定义；公式请在原公式框中编辑。".into());
    }
    Ok(())
}
fn analyze_macros(text: &str, previous: Option<&MacroRegistry>) -> MacroRegistry {
    let source = Source::detached(text.to_string());
    let mut registry = MacroRegistry::default();
    let mut offset = 0;
    let mut start = 0;
    let mut reuse_prefix = true;
    for node in source.root().children() {
        offset += node.len();
        let Some(binding) = node.cast::<ast::LetBinding>() else { continue; };
        let context_end = start;
        let input = &text[start..offset];
        start = offset;
        if reuse_prefix {
            if let Some(old) = previous.and_then(|r| r.entries.get(registry.entries.len())).filter(|d| d.input == input) {
                let mut def = old.clone();
                def.source = input.to_string(); // exclude the old trailing trivia
                def.shadowed = false;
                registry.register(def);
                continue;
            }
            reuse_prefix = false;
        }
        let names: Vec<String> = binding.kind().bindings().iter().map(|n| n.as_str().to_string()).collect();
        let mut def = MacroDefinition { source: input.to_string(), input: input.to_string(), name: names.join(", "), names,
            params: vec![], expandable: false, shadowed: false, reason: "仅支持直接返回数学公式的位置参数函数或公式常量".into(),
            template: Rc::new(vec![]), context: Rc::new(text[..context_end].to_string()), function: false, size: TemplateSize::default() };
        let mut body = binding.init();
        let mut supported = def.names.len() == 1;
        let mut locals = HashSet::new();
        if let Some(ast::Expr::Closure(closure)) = body {
            def.function = true;
            // Named closures bind themselves; do not accidentally resolve a
            // recursive call to an older same-name macro. Other lets evaluate
            // their initializer in the preceding scope.
            if let Some(name) = closure.name() { locals.insert(name.as_str().to_string()); }
            for param in closure.params().children() {
                if let ast::Param::Pos(ast::Pattern::Normal(ast::Expr::Ident(ident))) = param {
                    def.params.push(ident.as_str().to_string());
                } else { supported = false; }
            }
            if def.params.iter().collect::<HashSet<_>>().len() != def.params.len() { supported = false; }
            locals.extend(def.params.iter().cloned());
            body = Some(closure.body());
        }
        if supported && let Some(ast::Expr::Equation(eq)) = body {
            let ctx = ParseContext { params: &def.params, locals: &locals, registry: &registry, template: true };
            let template = parse_cell(eq.body().to_untyped(), &ctx);
            let mut used = vec![false; def.params.len()];
            fn check(data: &MathData, params: &[String], used: &mut [bool]) -> bool {
                data.iter().all(|atom| {
                    if let Kind::Parameter { index } = atom.kind { used[index] = true; }
                    if let Kind::Raw { source } = &atom.kind {
                        let parsed = Source::detached(format!("$ {source} $"));
                        if has_reference(parsed.root(), params) { return false; }
                    }
                    // Every registered callee exposes every parameter, so its
                    // argument templates preserve the caller's parameter uses.
                    atom.cells.iter().all(|c| check(c, params, used))
                })
            }
            if !check(&template, &def.params, &mut used) {
                def.reason = "参数引用位于 Raw 中，无法在原位编辑".into();
            } else if used.contains(&false) {
                def.reason = "存在未显示的参数，无法提供全部参数输入框".into();
            } else {
                def.expandable = true;
                def.reason = "所有参数均可在结构槽位中编辑".into();
                def.size = template_size(&template, &registry, def.params.len());
                def.template = Rc::new(template);
            }
        }
        registry.register(def);
    }
    if let Some(last) = registry.entries.last_mut() { last.source.push_str(&text[start..]); }
    for (index, def) in registry.entries.iter_mut().enumerate() {
        def.shadowed = def.names.iter().all(|name| match registry.names.get(name) {
            Some(Binding::Expandable(current) | Binding::Opaque(current)) => *current != index,
            None => true,
        });
    }
    registry
}

pub struct Parsed { pub root: MathData, pub definitions: String, pub display: bool }

pub fn parse_command(text: &str, definitions: &str) -> Result<Parsed, String> {
    let source = Source::detached(text.to_string());
    let document = source.root().children().any(|n| matches!(n.kind(), SyntaxKind::Equation | SyntaxKind::LetBinding));
    let body = if document { text.to_string() } else { format!("$ {text} $") };
    let separator = if definitions.is_empty() || definitions.ends_with('\n') { "" } else { "\n" };
    parse_document(&format!("{definitions}{separator}{body}"))
}

pub fn parse_document(text: &str) -> Result<Parsed, String> {
    let initial = Source::detached(text.to_owned());
    let document = initial.root().children().any(|n| matches!(n.kind(), SyntaxKind::Equation | SyntaxKind::LetBinding));
    let source = if document { initial } else { Source::detached(format!("$ {text} $")) };
    let (errors, _) = source.root().errors_and_warnings();
    if let Some(error) = errors.first() { return Err(error.message.to_string()); }
    let mut root = None;
    let mut display = true;
    let mut definitions = String::new();
    for node in source.root().children() {
        if let Some(eq) = node.cast::<ast::Equation>() {
            if root.is_some() { return Err("此原型一次编辑一个顶层公式；请只导入目标公式和所需函数定义。".into()); }
            let registry = macro_registry(&definitions);
            let ctx = ParseContext { params: &[], locals: &HashSet::new(), registry: &registry, template: false };
            root = Some(parse_cell(eq.body().to_untyped(), &ctx));
            display = eq.block();
        } else if node.kind() == SyntaxKind::LetBinding {
            if root.is_some() { return Err("请将 #let 定义放在公式之前。".into()); }
            // Keep code opaque. Only Typst evaluates functions and variables.
            definitions.push_str(&node.full_text());
        } else if !matches!(node.kind(), SyntaxKind::Space | SyntaxKind::Parbreak | SyntaxKind::Hash | SyntaxKind::LineComment | SyntaxKind::BlockComment) {
            return Err("这是独立公式原型，导入内容仅支持函数定义和一个公式。".into());
        } else if root.is_none() { definitions.push_str(&node.full_text());
        }
    }
    if definitions.trim().is_empty() { definitions.clear(); }
    Ok(Parsed { root: root.unwrap_or_default(), definitions, display })
}

fn parse_cell(node: &SyntaxNode, ctx: &ParseContext) -> MathData {
    if node.kind() != SyntaxKind::Math { return parse_atom(node, ctx); }
    let mut children: Vec<_> = node.children().collect();
    if children.first().is_some_and(|n| n.kind() == SyntaxKind::LeftParen) && children.last().is_some_and(|n| n.kind() == SyntaxKind::RightParen) {
        children.remove(0); children.pop();
    }
    // Split only syntax nodes at this math level: quoted/escaped markers and
    // markers in nested calls must not split the enclosing formula.
    if children.iter().any(|n| matches!(n.kind(), SyntaxKind::Linebreak | SyntaxKind::MathAlignPoint)) {
        let mut rows: Vec<Vec<MathData>> = vec![vec![]];
        let mut segment = vec![];
        for child in children {
            if matches!(child.kind(), SyntaxKind::Linebreak | SyntaxKind::MathAlignPoint) {
                rows.last_mut().unwrap().push(parse_nodes(&segment, ctx));
                segment.clear();
                if child.kind() == SyntaxKind::Linebreak { rows.push(vec![]); }
            } else { segment.push(child); }
        }
        rows.last_mut().unwrap().push(parse_nodes(&segment, ctx));
        let columns = rows.iter().map(Vec::len).max().unwrap_or(1);
        let row_lengths = rows.iter().map(Vec::len).collect();
        let cells = rows.into_iter().flat_map(|mut row| { row.resize(columns, vec![]); row }).collect();
        return vec![MathAtom { kind: Kind::Aligned { columns, row_lengths }, cells }];
    }
    parse_nodes(&children, ctx)
}
fn parse_nodes(nodes: &[&SyntaxNode], ctx: &ParseContext) -> MathData {
    let mut result = vec![];
    let mut children = nodes.iter().copied();
    while let Some(child) = children.next() {
        if child.kind() == SyntaxKind::Hash {
            if let Some(next) = children.next() {
                if let Some(index) = parameter(next, ctx.params) {
                    result.push(MathAtom { kind: Kind::Parameter { index }, cells: vec![] });
                } else { result.push(MathAtom::raw(format!("#{}", next.full_text()))); }
            }
        } else if !matches!(child.kind(), SyntaxKind::Space | SyntaxKind::Parbreak | SyntaxKind::LineComment | SyntaxKind::BlockComment) {
            result.extend(parse_atom(child, ctx));
        }
    }
    result
}
fn parse_atom(node: &SyntaxNode, ctx: &ParseContext) -> MathData {
    if let Some(index) = parameter(node, ctx.params) {
        return vec![MathAtom { kind: Kind::Parameter { index }, cells: vec![] }];
    }
    if node.kind() == SyntaxKind::MathIdent && ctx.is_bound(node.full_text().as_str()) {
        if let Some((index, def)) = ctx.resolve(node.full_text().as_str()).filter(|(_,d)| !d.function) {
            return vec![ctx.call(index, def, vec![])];
        }
        return vec![MathAtom::raw(node.full_text())];
    }
    let atom = match node.cast::<ast::Expr>() {
        Some(ast::Expr::Math(_)) => return parse_cell(node, ctx),
        Some(ast::Expr::Str(s)) => MathAtom { kind: Kind::Text, cells: vec![s.get().chars().map(MathAtom::character).collect()] },
        Some(ast::Expr::MathFrac(f)) => MathAtom { kind: Kind::Frac, cells: vec![parse_cell(f.num().to_untyped(), ctx), parse_cell(f.denom().to_untyped(), ctx)] },
        Some(ast::Expr::MathAttach(a)) if a.primes().is_none() => {
            let mut script = MathAtom { kind: Kind::Script { cell_1_is_up: a.top().is_some() }, cells: vec![parse_cell(a.base().to_untyped(), ctx)] };
            if let Some(top) = a.top() { script.cells.push(parse_cell(top.to_untyped(), ctx)); }
            if let Some(bottom) = a.bottom() { script.cells.push(parse_cell(bottom.to_untyped(), ctx)); }
            script
        }
        Some(ast::Expr::MathRoot(r)) => {
            let body = parse_cell(r.radicand().to_untyped(), ctx);
            if let Some(i) = r.index() { MathAtom { kind: Kind::Root, cells: vec![body, i.to_string().chars().map(MathAtom::character).collect()] } }
            else { MathAtom { kind: Kind::Sqrt, cells: vec![body] } }
        }
        Some(ast::Expr::MathDelimited(d)) => {
            let raw = node.full_text();
            let mut body = parse_cell(d.body().to_untyped(), ctx);
            let left = raw.chars().next().unwrap_or('(').to_string();
            let right = raw.chars().last().unwrap_or(')').to_string();
            // Typst includes the delimiters in MathDelimited::body.
            if body.first().is_some_and(|a| write_atom(a) == left) { body.remove(0); }
            if body.last().is_some_and(|a| write_atom(a) == right) { body.pop(); }
            MathAtom { kind: Kind::Delim { left, right }, cells: vec![body] }
        }
        Some(ast::Expr::MathCall(call)) => {
            let name = call.callee().to_untyped().full_text().to_string();
            if ctx.is_bound(&name) {
                if let Some((index, def)) = ctx.resolve(&name).filter(|(_,d)| d.function) {
                    let args: Option<Vec<_>> = call.args().arg_items().map(|item| match item.arg {
                        ast::Arg::Pos(expr) if !item.ends_in_semicolon => Some(parse_cell(expr.to_untyped(), ctx)),
                        _ => None,
                    }).collect();
                    if let Some(cells) = args.filter(|args| args.len() == def.params.len()) {
                        return vec![ctx.call(index, def, cells)];
                    }
                }
                return vec![MathAtom::raw(node.full_text())];
            }
            if !COMMANDS.contains(&name.as_str()) { return vec![MathAtom::from_source(node.full_text())]; }
            let mut args = vec![];
            let mut widths = vec![];
            let mut width = 0;
            for item in call.args().arg_items() {
                let ast::Arg::Pos(expr) = item.arg else { return vec![MathAtom::from_source(node.full_text())]; };
                let mut arg = parse_cell(expr.to_untyped(), ctx);
                if arg.len() == 1 && matches!(arg[0].kind, Kind::Text) && arg[0].cells[0].is_empty() { arg.clear(); }
                args.push(arg); width += 1;
                if item.ends_in_semicolon { widths.push(width); width = 0; }
            }
            if width > 0 { widths.push(width); }
            let kind = match (name.as_str(), args.len()) {
                ("frac", 2) => Kind::Frac, ("sqrt", 1) => Kind::Sqrt, ("root", 2) => Kind::Root,
                ("abs", 1) => Kind::Delim { left: "|".into(), right: "|".into() },
                ("norm", 1) => Kind::Delim { left: "‖".into(), right: "‖".into() },
                ("overline" | "underline" | "hat" | "vec", 1) => Kind::Decoration { name },
                ("mat", n) if n > 0 && widths.iter().all(|w| *w == widths[0]) => Kind::Grid { columns: widths[0] },
                _ => return vec![MathAtom::from_source(node.full_text())],
            };
            if matches!(kind, Kind::Root) { args.swap(0, 1); }
            MathAtom { kind, cells: args }
        }
        Some(ast::Expr::MathShorthand(_)) | Some(ast::Expr::Escape(_)) => MathAtom::from_source(node.full_text()),
        Some(ast::Expr::Linebreak(_)) | Some(ast::Expr::MathAlignPoint(_)) => return parse_marker(node),
        _ => {
            let raw = node.full_text().to_string();
            if let Some(glyph) = symbol(&raw) { MathAtom { kind: Kind::Symbol { name: raw, glyph: glyph.into() }, cells: vec![] } }
            else if !raw.is_empty() && raw.chars().all(is_operator) { MathAtom::from_source(raw) }
            else if matches!(node.kind(), SyntaxKind::MathText | SyntaxKind::Text | SyntaxKind::Int | SyntaxKind::Float) {
                return raw.chars().map(MathAtom::character).collect();
            } else { MathAtom::from_source(raw) }
        }
    };
    if matches!(atom.kind, Kind::Text) && atom.cells[0].is_empty() { vec![] } else { vec![atom] }
}

fn parse_marker(node: &SyntaxNode) -> MathData {
    // A single marker can also be the complete expression in an argument.
    let columns = if node.kind() == SyntaxKind::MathAlignPoint { 2 } else { 1 };
    let row_lengths = if columns == 2 { vec![2] } else { vec![1, 1] };
    vec![MathAtom { kind: Kind::Aligned { columns, row_lengths }, cells: vec![vec![], vec![]] }]
}

pub fn write_cell(data: &MathData) -> String {
    if data.is_empty() { return "\"\"".into(); }
    let mut out = String::new();
    let mut previous_digit = false;
    let mut previous_operator = false;
    for atom in data {
        let digit = matches!(atom.kind, Kind::Char { value } if value.is_ascii_digit() || value == '.');
        let operator = matches!(atom.kind,Kind::Char {value} if !value.is_alphanumeric() && !value.is_whitespace());
        if !out.is_empty() && !(digit && previous_digit || operator && previous_operator) { out.push(' '); }
        out.push_str(&write_atom(atom)); previous_digit = digit; previous_operator = operator;
    }
    out
}
pub fn write_atom(atom: &MathAtom) -> String {
    let c = |idx| write_cell(&atom.cells[idx]);
    match &atom.kind {
        Kind::Char { value } => value.to_string(),
        Kind::Symbol { name, .. } | Kind::Raw { source: name } => name.clone(),
        Kind::Unknown { name, .. } => name.clone(),
        Kind::MacroCall { name, function } => if *function { format!("{name}({})", atom.cells.iter().map(write_cell).collect::<Vec<_>>().join(", ")) } else { name.clone() },
        Kind::Parameter { index } => format!("#parameter{index}"),
        Kind::TemplateCall { .. } => unreachable!("template edges never belong to the editable source tree"),
        Kind::Text => serde_json::to_string(&atom.cells[0].iter().map(|a| if let Kind::Char { value } = a.kind { value.to_string() } else { write_atom(a) }).collect::<String>()).unwrap(),
        Kind::Frac => format!("frac({}, {})", c(0), c(1)),
        Kind::Sqrt => format!("sqrt({})", c(0)),
        Kind::Root => format!("root({}, {})", c(1), c(0)),
        Kind::Script { .. } => {
            let base = c(0);
            let mut result = if atom.cells[0].len() > 1 { format!("({base})") } else { base };
            if let Some(i) = atom.script_idx(false) { result.push_str(&format!("_({})", c(i))); }
            if let Some(i) = atom.script_idx(true) { result.push_str(&format!("^({})", c(i))); }
            result
        }
        Kind::Delim { left, right } if left == "|" && right == "|" => format!("abs({})", c(0)),
        Kind::Delim { left, right } if left == "‖" && right == "‖" => format!("norm({})", c(0)),
        Kind::Delim { left, right } => format!("{left}{}{right}", c(0)),
        Kind::Grid { columns } => format!("mat({})", atom.cells.chunks(*columns).map(|row| row.iter().map(write_cell).collect::<Vec<_>>().join(", ")).collect::<Vec<_>>().join("; ")),
        Kind::Aligned { columns, row_lengths } => atom.cells.chunks(*columns).enumerate().map(|(r, row)| {
            let used = row.iter().rposition(|c| !c.is_empty()).map_or(1, |i| i+1).max(row_lengths[r]);
            row[..used].iter().map(|c| if c.is_empty() { String::new() } else { write_cell(c) }).collect::<Vec<_>>().join(" & ")
        }).collect::<Vec<_>>().join(" \\\n"),
        Kind::Decoration { name } => format!("{name}({})", atom.cells.iter().map(write_cell).collect::<Vec<_>>().join(", ")),
    }
}
pub fn write_document(data: &MathData, definitions: &str) -> String {
    write_document_mode(data, definitions, true)
}
pub fn write_document_mode(data: &MathData, definitions: &str, display: bool) -> String {
    let separator = if definitions.is_empty() || definitions.ends_with('\n') { "" } else { "\n" };
    let space = if display { " " } else { "" };
    format!("{definitions}{separator}${space}{}{space}$", write_cell(data))
}
