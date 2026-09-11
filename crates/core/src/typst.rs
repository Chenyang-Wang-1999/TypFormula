// SPDX-License-Identifier: GPL-2.0-or-later
// Typst replaces MathParser/TeXMathStream at import and command confirmation.
// Structural editing and draft keystrokes do not reparse the formula.
use crate::math::*;
use crate::slots::Write;
use typst_syntax::{Source, SyntaxKind, SyntaxNode, ast::{self, AstNode}};
use std::{collections::{HashMap, HashSet}, sync::{Arc, Mutex, OnceLock}};

pub const PROJECTION_LIMIT: usize = 4096;
const SIZE_CAP: usize = PROJECTION_LIMIT + 1;
// Definition-prefix text kept in the registry cache. A document with F formulas
// asks for F distinct prefixes, so the budget has to cover a whole document for
// the second pass over it (font change, split, LSP reclassification, reopening a
// file) to be cheap; entries also hold their own context strings, so the real
// footprint is a small multiple of this.
const CACHE_TEXT_LIMIT: usize = 8 * 1024 * 1024;
const CACHE_ENTRY_LIMIT: usize = 4096;
// Consecutive misses after which the cache stops inserting. A document whose
// definition prefixes do not fit is analysed in full every pass either way; not
// inserting keeps that pass at the cost of the analysis alone, instead of paying
// for an entry that is evicted before it is ever used again.
const CACHE_SATURATION_MISSES: u32 = 64;
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
    pub template: Arc<MathData>,
    #[serde(skip)]
    pub context: Arc<String>,
    #[serde(skip)]
    pub function: bool,
    #[serde(skip)]
    pub definition_start: usize,
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
enum Binding { Expandable(usize), Opaque(usize), Hidden }
#[derive(Default)]
pub struct MacroRegistry {
    pub entries: Vec<MacroDefinition>,
    names: HashMap<String, Binding>,
}
impl MacroRegistry {
    pub fn is_bound(&self, name: &str) -> bool { self.names.contains_key(name) }
    pub fn get(&self, name: &str) -> Option<&MacroDefinition> {
        let (Binding::Expandable(index) | Binding::Opaque(index)) = self.names.get(name)? else { return None; };
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
// Registries are keyed by the definition prefix they were analyzed from and are
// shared process-wide. The HTTP and stdio servers answer each request on its own
// thread, so a thread-local cache is empty for every request; the desktop is
// single-threaded and keeps reusing the entries it already built.
static MACROS: Mutex<MacroCache> = Mutex::new(MacroCache::new());
static EMPTY_MACROS: OnceLock<Arc<MacroRegistry>> = OnceLock::new();
fn empty_macros() -> Arc<MacroRegistry> { EMPTY_MACROS.get_or_init(|| Arc::new(MacroRegistry::default())).clone() }
struct MacroCache { entries: Vec<CacheEntry>, text: usize, misses: u32, saturated: bool }
// Lookups compare a hash and a length instead of whole prefixes: a document of
// N formulas asks for N prefixes of growing size, and comparing each of them
// against every cached entry costs more than the analysis it saves.
struct CacheEntry { key: String, hash: u64, len: usize, registry: Arc<MacroRegistry> }
fn key_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() { hash ^= *byte as u64; hash = hash.wrapping_mul(0x100_0000_01b3); }
    hash
}
impl MacroCache {
    const fn new() -> Self { Self { entries: Vec::new(), text: 0, misses: 0, saturated: false } }
    // Only a matching hash and length is verified as a whole string, so a
    // collision cannot return a registry for different definitions.
    fn find(&self, definitions: &str, hash: u64) -> Option<usize> {
        self.entries.iter().position(|entry| entry.hash == hash && entry.len == definitions.len() && entry.key == definitions)
    }
    fn retain(&mut self, definitions: &str, hash: u64, registry: &Arc<MacroRegistry>) {
        // Either the working set fits and every entry is kept, or it does not and
        // the cache stops for good. A hit must not clear that verdict: a document
        // whose prefixes do not fit still hits the few entries near the end of the
        // previous pass, and resuming insertion there only makes the pass slower.
        if self.text + definitions.len() > CACHE_TEXT_LIMIT || self.entries.len() >= CACHE_ENTRY_LIMIT {
            self.misses = self.misses.saturating_add(1);
            if self.misses >= CACHE_SATURATION_MISSES {
                // Holding entries that will never be reused only adds allocation
                // pressure to the analysis, which then runs slower than without a
                // cache at all, so drop them.
                self.entries.clear();
                self.text = 0;
                self.saturated = true;
            }
            if self.saturated { return; }
        }
        self.entries.push(CacheEntry { key: definitions.to_string(), hash, len: definitions.len(), registry: registry.clone() });
        self.text += definitions.len();
        while self.entries.len() > 1 && (self.text > CACHE_TEXT_LIMIT || self.entries.len() > CACHE_ENTRY_LIMIT) {
            self.text = self.text.saturating_sub(self.entries.remove(0).len);
        }
    }
}
// A poisoned lock still holds usable data: the caches are plain maps of derived
// values, so recover instead of propagating a panic from an unrelated thread.
fn poison_free<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> { lock.lock().unwrap_or_else(|error| error.into_inner()) }
// The document prefix that ends where this definition ends. A fragment inside the
// definition body sits at an offset in exactly this text, so that offset — not a
// private rendering instance — is how its image is asked for.
pub fn definition_prefix(def: &MacroDefinition) -> String { format!("{}{}", def.context, def.input) }
// The offsets in the document at which a definition's own text spells `text`.
// A fragment inside a macro template is rendered from its call site, but the
// source range it is asked for is the one in the definition, so one definition
// that writes the same fragment twice is disambiguated by ordinal, not by text.
pub fn definition_raw_ranges(def: &MacroDefinition, text: &str) -> Vec<(usize, usize)> {
    raw_ranges(&definition_prefix(def), 0, text).into_iter().filter(|(start, _)| *start >= def.definition_start).collect()
}
pub fn raw_ranges(source: &str, offset: usize, text: &str) -> Vec<(usize, usize)> {
    fn visit(node: &SyntaxNode, at: usize, text: &str, out: &mut Vec<(usize, usize)>) {
        if node.full_text().as_str() == text { out.push((at, at + node.len())); return; }
        let mut pos = at;
        let children: Vec<_> = node.children().collect();
        for (i, child) in children.iter().enumerate() {
            if child.kind() == SyntaxKind::Hash && children.get(i + 1).is_some_and(|next| format!("#{}", next.full_text()) == text) {
                out.push((pos, pos + text.len()));
            } else { visit(child, pos, text, out); }
            pos += child.len();
        }
    }
    let mut out = vec![]; visit(Source::detached(source).root(), offset, text, &mut out); out
}

// A prefix ends at the formula being edited. Traverse only its open lexical
// ancestors: completed sibling blocks must never export their local bindings.
enum ScopeBinding<'a> { Let(&'a SyntaxNode, usize), Hidden(Vec<String>) }
fn scope_bindings<'a>(node: &'a SyntaxNode, offset: usize, out: &mut Vec<ScopeBinding<'a>>) {
    if matches!(node.kind(), SyntaxKind::ContentBlock | SyntaxKind::CodeBlock)
        && node.children().last().is_some_and(|n| matches!(n.kind(), SyntaxKind::RightBracket | SyntaxKind::RightBrace)) { return; }
    if let Some(binding) = node.cast::<ast::LetBinding>() {
        if node.errors_and_warnings().0.is_empty() { out.push(ScopeBinding::Let(node, offset)); return; }
        // A formula inside a function definition sees its parameters, not an
        // older same-name binding or the unfinished function as an expansion.
        if let Some(ast::Expr::Closure(closure)) = binding.init() {
            let mut names = Vec::new();
            if let Some(name) = closure.name() { names.push(name.as_str().to_owned()); }
            for param in closure.params().children() {
                match param {
                    ast::Param::Pos(pattern) => names.extend(pattern.bindings().iter().map(|n| n.as_str().to_owned())),
                    ast::Param::Named(named) => names.push(named.name().as_str().to_owned()),
                    ast::Param::Spread(spread) => { if let Some(name) = spread.sink_ident() { names.push(name.as_str().to_owned()); } }
                }
            }
            out.push(ScopeBinding::Hidden(names));
        }
    }
    if node.kind() == SyntaxKind::Equation && node.errors_and_warnings().0.is_empty() { return; }
    let mut at = offset;
    for child in node.children() { scope_bindings(child, at, out); at += child.len(); }
}
pub fn macro_registry(definitions: &str) -> Arc<MacroRegistry> {
    // The desktop passes the full lexical prefix. Most documents and most
    // early formulas contain no bindings at all; avoid parsing that markup a
    // second time merely to construct an empty registry.
    if !definitions.contains("let") { return empty_macros(); }
    let hash = key_hash(definitions);
    let mut cache = poison_free(&MACROS);
    if let Some(index) = cache.find(definitions, hash) {
        let entry = cache.entries.swap_remove(index);
        let registry = entry.registry.clone();
        cache.entries.push(entry);
        cache.misses = 0;
        return registry;
    }
    // Reuse the most recently analyzed prefix: a definition analyzed from a
    // prefix stays valid for every extension of it, and its templates are shared
    // instead of rebuilt. Documents ask for prefixes in growing order, so the most
    // recent entry is the longest usable prefix; when an edit lands inside the
    // definitions themselves it is simply a candidate whose earlier definitions
    // analyze_macros still reuses, one definition at a time.
    let previous = cache.entries.last().map(|entry| entry.registry.clone());
    let registry = Arc::new(analyze_macros(definitions, previous.as_deref()));
    cache.retain(definitions, hash, &registry);
    registry
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
// Clone a definition for reuse without copying its source text: the reused
// definition carries only its own text, and `old.source` holds the whole trailing
// prefix, which on a large document is far bigger than every other field.
fn reused_definition(old: &MacroDefinition, input: &str) -> MacroDefinition {
    MacroDefinition {
        source: input.to_string(), input: input.to_string(),
        name: old.name.clone(), names: old.names.clone(), params: old.params.clone(),
        expandable: old.expandable, shadowed: false, reason: old.reason.clone(),
        template: old.template.clone(), context: old.context.clone(), function: old.function,
        definition_start: old.definition_start, size: old.size.clone(),
    }
}
fn analyze_macros(text: &str, previous: Option<&MacroRegistry>) -> MacroRegistry {
    let source = Source::detached(text.to_string());
    let mut registry = MacroRegistry::default();
    let mut start = 0;
    let mut reuse_prefix = true;
    let mut bindings = vec![];
    scope_bindings(source.root(), 0, &mut bindings);
    for item in bindings {
        let ScopeBinding::Let(node, at) = item else {
            if let ScopeBinding::Hidden(names) = item { for name in names { registry.names.insert(name, Binding::Hidden); } }
            reuse_prefix = false; continue;
        };
        let binding = node.cast::<ast::LetBinding>().unwrap();
        let offset = at + node.len();
        let context_end = start;
        let input = &text[start..offset];
        start = offset;
        if reuse_prefix {
            if let Some(old) = previous.and_then(|r| r.entries.get(registry.entries.len())).filter(|d| d.input == input) {
                registry.register(reused_definition(old, input));
                continue;
            }
            reuse_prefix = false;
        }
        let names: Vec<String> = binding.kind().bindings().iter().map(|n| n.as_str().to_string()).collect();
        let mut def = MacroDefinition { source: input.to_string(), input: input.to_string(), name: names.join(", "), names,
            params: vec![], expandable: false, shadowed: false, reason: "仅支持直接返回数学公式的位置参数函数或公式常量".into(),
            template: Arc::new(vec![]), context: Arc::new(text[..context_end].to_string()), function: false, definition_start: at, size: TemplateSize::default() };
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
                def.template = Arc::new(template);
            }
        }
        registry.register(def);
    }
    if let Some(last) = registry.entries.last_mut() { last.source.push_str(&text[start..]); }
    for (index, def) in registry.entries.iter_mut().enumerate() {
        def.shadowed = def.names.iter().all(|name| match registry.names.get(name) {
            Some(Binding::Expandable(current) | Binding::Opaque(current)) => *current != index,
            Some(Binding::Hidden) | None => true,
        });
    }
    registry
}

pub struct Parsed { pub root: MathData, pub definitions: String, pub display: bool }

/// Parse a single equation against the lexical document prefix without treating
/// surrounding markup as part of the editable math tree.
pub fn parse_formula(text: &str, context: &str) -> Result<Parsed, String> {
    let source = Source::detached(text.to_owned());
    let eq = source.root().children().find(|n| n.kind()==SyntaxKind::Equation).ok_or("缺少公式")?;
    parse_formula_node(eq,context)
}

/// Build the structural model from an equation already owned by the document
/// syntax tree. This preserves Typst's incremental AST and avoids reparsing the
/// equation string for every static projection during import.
pub fn parse_formula_node(node:&SyntaxNode,context:&str)->Result<Parsed,String> {
    let (errors, _) = node.errors_and_warnings();
    if let Some(error) = errors.first() { return Err(error.message.to_string()); }
    let eq=node.cast::<ast::Equation>().ok_or("缺少公式")?;
    let registry = macro_registry(context);
    let ctx = ParseContext { params: &[], locals: &HashSet::new(), registry: &registry, template: false };
    Ok(Parsed { root: parse_cell(eq.body().to_untyped(), &ctx), definitions: context.to_owned(), display: eq.block() })
}

pub fn parse_command(text: &str, definitions: &str) -> Result<Parsed, String> {
    let source = Source::detached(text.to_string());
    let document = source.root().children().any(|n| matches!(n.kind(), SyntaxKind::Equation | SyntaxKind::LetBinding));
    let body = if document { text.to_string() } else { format!("$ {text} $") };
    let separator = if definitions.is_empty() || definitions.ends_with('\n') { "" } else { "\n" };
    if !document { return parse_formula(&body, definitions); }
    if source.root().children().any(|n| n.kind() == SyntaxKind::LetBinding) {
        parse_document(&format!("{definitions}{separator}{body}"))
    } else { parse_formula(&body, definitions) }
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
        return vec![MathAtom { kind: Kind::Multiline { columns, row_lengths }, cells }];
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
        Some(ast::Expr::MathFrac(f)) => MathAtom { kind: Kind::Fraction, cells: vec![parse_cell(f.num().to_untyped(), ctx), parse_cell(f.denom().to_untyped(), ctx)] },
        Some(ast::Expr::MathAttach(a)) if a.primes().is_none() => {
            // Storage is always `[base, upper, lower]`, whatever the source
            // spells: an attachment the source omits stays an empty cell.
            let mut script = MathAtom { kind: Kind::Scripts, cells: vec![parse_cell(a.base().to_untyped(), ctx), vec![], vec![]] };
            if let Some(top) = a.top() { script.cells[script_cell(true)] = parse_cell(top.to_untyped(), ctx); }
            if let Some(bottom) = a.bottom() { script.cells[script_cell(false)] = parse_cell(bottom.to_untyped(), ctx); }
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
            MathAtom { kind: Kind::Fenced { left, right }, cells: vec![body] }
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
                ("frac", 2) => Kind::Fraction, ("sqrt", 1) => Kind::Sqrt, ("root", 2) => Kind::Root,
                ("abs", 1) => Kind::Fenced { left: "|".into(), right: "|".into() },
                ("norm", 1) => Kind::Fenced { left: "‖".into(), right: "‖".into() },
                ("overline" | "underline" | "hat" | "vec", 1) => Kind::Decoration { name },
                ("mat", n) if n > 0 && widths.iter().all(|w| *w == widths[0]) => Kind::Table { columns: widths[0] },
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
                // The lexer keeps a numeric run in one node and one grapheme
                // cluster in the others, so a run becomes one atom here. Which
                // runs count is the engine's rule (`math::is_number`) and not the
                // lexer's: `²3` is one token but resolves to a `Text`.
                if is_number(&raw) { return vec![MathAtom::number(&raw)]; }
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
    vec![MathAtom { kind: Kind::Multiline { columns, row_lengths }, cells: vec![vec![], vec![]] }]
}

pub fn write_cell(data: &MathData) -> String {
    if data.is_empty() { return "\"\"".into(); }
    let mut out = String::new();
    // A separator goes between every two atoms: the source is re-parsed whenever
    // the document is analyzed, so characters that would lex as one token have to
    // be kept apart. Two letters would become one identifier (`xy`), two operator
    // characters one Typst shorthand -- `->` is an arrow, `||` is `‖`, `...` is
    // `…` -- turning typed characters into a single uneditable fragment.
    //
    // Digits used to be the one exception, because a digit run was several `Char`
    // atoms that had to stay lexed as one number. A run is one `Kind::Number` now,
    // so there is nothing left to hold together and the exception is gone;
    // `tests/structured_input.rs` keeps the separator rule pinned.
    for atom in data {
        if !out.is_empty() { out.push(' '); }
        out.push_str(&write_atom(atom));
    }
    out
}
/// Fills a `slots::Write::Template`: `{0}`, `{1}`… are cell indices and
/// `{name}` is the node's stored name.
///
/// One pass rather than successive replacement: a cell's own source can contain
/// braces (a text run is written as a quoted string), so substituting cell 0
/// first would let its text be expanded as though it were part of the template.
fn fill_template(template: &str, atom: &MathAtom) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else { out.push_str(&rest[open..]); return out };
        let key = &rest[open + 1..open + close];
        match key {
            "name" => match &atom.kind {
                Kind::MacroCall { name, .. } | Kind::Decoration { name } => out.push_str(name),
                other => unreachable!("模板用了 {{name}}，但 {other:?} 没有名字"),
            },
            other => match other.parse::<usize>().ok().and_then(|index| atom.cells.get(index)) {
                Some(cell) => out.push_str(&write_cell(cell)),
                // Keep an invented placeholder visible rather than dropping it,
                // so the round-trip test fails loudly instead of silently
                // writing a truncated node into the document.
                None => { debug_assert!(false, "模板占位符 {{{other}}} 没有对应的格子"); out.push_str(&rest[open..open + close + 1]); }
            },
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}
/// A node's Typst spelling, chosen by its declaration (`slots::Write`).
///
/// The `unreachable!` arms are declaration-versus-kind mismatches: the
/// declaration says which shape a kind has, and `slots.rs`'s tests assert that
/// every kind agrees with its own declaration. A loud stop is deliberate here —
/// this function feeds the authoritative source, so writing a truncated node
/// would corrupt the document, which is worse than stopping.
pub fn write_atom(atom: &MathAtom) -> String {
    let cell = |index: usize| atom.cells.get(index).map(write_cell).unwrap_or_default();
    let joined = |cells: &[MathData]| cells.iter().map(write_cell).collect::<Vec<_>>().join(", ");
    match atom.decl().write {
        Write::OwnText => match &atom.kind {
            Kind::Char { value } => value.to_string(),
            Kind::Symbol { name, .. } | Kind::Raw { source: name } | Kind::Unknown { name, .. } => name.clone(),
            other => unreachable!("{other:?} 由自己的文本拼写，但它没有文本"),
        },
        Write::Marker => match &atom.kind {
            Kind::Parameter { index } => format!("#parameter{index}"),
            other => unreachable!("{other:?} 声明为占位拼写"),
        },
        Write::TemplateOnly => unreachable!("template edges never belong to the editable source tree"),
        Write::Quoted => serde_json::to_string(&atom.cells[0].iter().map(|a| if let Kind::Char { value } = a.kind { value.to_string() } else { write_atom(a) }).collect::<String>()).unwrap(),
        Write::Run => {
            let run: String = atom.cells[0].iter().map(write_atom).collect();
            // A run is digits and at most one dot by construction; if an edit ever
            // left something else in the cell, the separator that would have to
            // come back is the safe spelling, so say so loudly here instead.
            debug_assert!(is_number(&run), "数字串的格子里出现了非数字：{run:?}");
            run
        }
        Write::Template(template) => fill_template(template, atom),
        Write::Named => match &atom.kind {
            Kind::MacroCall { name, function } => if *function { format!("{name}({})", joined(&atom.cells)) } else { name.clone() },
            other => unreachable!("{other:?} 声明为具名调用"),
        },
        Write::Attach => {
            let base = cell(0);
            let mut result = if atom.cells[0].len() > 1 { format!("({base})") } else { base };
            if let Some(i) = atom.script_idx(false) { result.push_str(&format!("_({})", cell(i))); }
            if let Some(i) = atom.script_idx(true) { result.push_str(&format!("^({})", cell(i))); }
            result
        }
        Write::Delimited => match &atom.kind {
            Kind::Fenced { left, right } if left == "|" && right == "|" => format!("abs({})", cell(0)),
            Kind::Fenced { left, right } if left == "‖" && right == "‖" => format!("norm({})", cell(0)),
            Kind::Fenced { left, right } => format!("{left}{}{right}", cell(0)),
            other => unreachable!("{other:?} 声明为定界包裹"),
        },
        Write::Matrix => match &atom.kind {
            Kind::Table { columns } => format!("mat({})", atom.cells.chunks(*columns).map(|row| joined(row)).collect::<Vec<_>>().join("; ")),
            other => unreachable!("{other:?} 声明为矩阵"),
        },
        Write::Rows => match &atom.kind {
            Kind::Multiline { columns, row_lengths } => atom.cells.chunks(*columns).enumerate().map(|(r, row)| {
                let used = row.iter().rposition(|c| !c.is_empty()).map_or(1, |i| i+1).max(row_lengths[r]);
                row[..used].iter().map(|c| if c.is_empty() { String::new() } else { write_cell(c) }).collect::<Vec<_>>().join(" & ")
            }).collect::<Vec<_>>().join(" \\\n"),
            other => unreachable!("{other:?} 声明为对齐公式"),
        },
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
