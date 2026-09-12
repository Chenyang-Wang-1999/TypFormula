// SPDX-License-Identifier: GPL-2.0-or-later
// Typst replaces MathParser/TeXMathStream at import and command confirmation.
// Structural editing and draft keystrokes do not reparse the formula.
use crate::math::*;
use crate::slots::{self, Write};
use crate::view::{self, ViewTemplate};
use unicode_segmentation::UnicodeSegmentation;
use typst_syntax::{Source, SyntaxKind, SyntaxNode, ast::{self, AstNode}};
use std::{collections::{HashMap, HashSet}, sync::{Arc, Mutex, OnceLock}};

pub const PROJECTION_LIMIT: usize = 4096;
const SIZE_CAP: usize = PROJECTION_LIMIT + 1;
/// The table an empty `mat()` is given, since its source says nothing about shape.
/// Kept here rather than in `cursor` because the parser is what builds it.
const NEW_TABLE_COLUMNS: usize = 2;
const NEW_TABLE_CELLS: usize = NEW_TABLE_COLUMNS * NEW_TABLE_COLUMNS;
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
    /// What the body expands to, **stored as a display tree**.
    ///
    /// Not as the atoms it was parsed from: those are dropped once this is built. Two
    /// representations of one template would drift, and the display tree is the one
    /// every reader wants — a call site is this material with its arguments bound into
    /// the holes, and a hole is a *variant* here (`view::ViewTemplate::Hole`), so the
    /// frontend cannot be handed one however the caller is written.
    #[serde(skip)]
    pub template: Arc<ViewTemplate>,
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
    let tree=Source::detached(source);
    let mut out = vec![]; visit(tree.root(), offset, text, &mut out);
    if out.is_empty() && text.contains('(') {
        // Structured calls are canonically spelled by the writer. Preserve the
        // original range even when it used a/b or different argument whitespace.
        fn canonical(text:&str)->Option<String> {
            parse_document(&format!("$ {text} $")).ok().map(|p|write_cell(&p.root))
        }
        fn calls(node:&SyntaxNode,at:usize,want:&str,out:&mut Vec<(usize,usize)>) {
            if node.kind()==SyntaxKind::MathCall && canonical(&node.full_text()).as_deref()==Some(want) {
                out.push((at,at+node.len()));return;
            }
            let mut pos=at;for child in node.children(){calls(child,pos,want,out);pos+=child.len();}
        }
        if let Some(want)=canonical(text) {calls(tree.root(),offset,&want,&mut out);}
    }
    out
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
        if let Kind::Parameter { index, .. } = atom.kind { out.params[index] = capped_add(out.params[index], 1); continue; }
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
            template: Arc::new(ViewTemplate::empty()), context: Arc::new(text[..context_end].to_string()), function: false, definition_start: at, size: TemplateSize::default() };
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
                    if let Kind::Parameter { index, .. } = atom.kind { used[index] = true; }
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
                // The atoms stop here: what is kept is the display tree they project
                // to, built with no session because registration has none.
                def.template = Arc::new(view::template_tree(&template, &registry));
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
///
/// A draft is bare expression text, not a document, so `$ {text} $` is how the
/// equation is addressed: `parse_formula` then takes the equation node and nothing
/// else, which is what makes the result a formula and not a paragraph of loose
/// atoms. Every command path goes through here.
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

/// Parse a command draft as a formula.
///
/// The draft is *expression* text, not a document, so it is always addressed as the
/// inside of an equation: `parse_formula` takes the equation node and nothing else,
/// which is what makes the answer a formula rather than a paragraph of loose atoms.
///
/// A draft that *is* a document — the author pasted one — is parsed as one, and the
/// caller keeps the definitions it carries. `Source::detached` over the raw text is
/// what tells the two apart: `$ x $` is a document and `x` is not.
pub fn parse_command(text: &str, definitions: &str) -> Result<Parsed, String> {
    let document = Source::detached(text.to_string()).root().children()
        .any(|n| matches!(n.kind(), SyntaxKind::Equation | SyntaxKind::LetBinding));
    if !document { return parse_formula(&format!("$ {text} $"), definitions); }
    let separator = if definitions.is_empty() || definitions.ends_with('\n') { "" } else { "\n" };
    if Source::detached(text.to_string()).root().children().any(|n| n.kind() == SyntaxKind::LetBinding) {
        parse_document(&format!("{definitions}{separator}{text}"))
    } else { parse_document(text) }
}

/// Parse a command name as the call it spells, filling in what only the registry
/// knows: a macro's own argument count, or `()` for a built-in.
///
/// Used when a command is typed with nothing after it, so the parser builds the node
/// the command means instead of an identifier with that name. Which call it spells is
/// still the parser's answer — this only supplies the arguments nobody typed.
///
/// A name the registry defines wins over a built-in of the same name, exactly as it
/// does in a document. That case cannot go through the parser at all: the arguments
/// do not exist yet, so there is nothing for the macro branch to match on, and the
/// node is built here with one empty cell per parameter — the same shape the call
/// would have once its arguments were typed.
pub fn parse_command_invocation(text: &str, definitions: &str) -> Result<Parsed, String> {
    let registry = macro_registry(definitions);
    if let Some((_, def)) = registry.get(text).filter(|def| def.expandable).map(|def| (0usize, def)) {
        if !def.params.is_empty() {
            let atom = MathAtom {
                kind: Kind::MacroCall { name: def.name.clone(), function: true },
                cells: vec![vec![]; def.params.len()],
            };
            return Ok(Parsed { root: vec![atom], definitions: definitions.to_owned(), display: true });
        }
    }
    parse_formula(&format!("$ {text}() $"), definitions)
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
                    result.push(MathAtom { kind: Kind::Parameter { index, name: ctx.params[index].clone() }, cells: vec![] });
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
        return vec![MathAtom { kind: Kind::Parameter { index, name: ctx.params[index].clone() }, cells: vec![] }];
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
        // `√x`/`∛x` are *syntax* rather than a call, but they mean the two shapes the
        // commands `sqrt`/`root` mean — and neither shape carries data of its own, so
        // the node is the call either way. That keeps `√x` and `sqrt(x)` **one** node
        // instead of two, and the written form is unchanged: `√x` has always been
        // written `sqrt(x)` (the shape's spelling is what decides, not the source).
        Some(ast::Expr::MathRoot(r)) => {
            let radicand = parse_cell(r.radicand().to_untyped(), ctx);
            // A radical's cells are `[index, radicand]`, the order Typst's own
            // `root(index, radicand)` takes and the order the two are read in.
            //
            // The degree is built as the `Number` that `root(3, x)` parses to, not from
            // its characters: `MathRoot::index` hands back a literal `u8`, and spelling
            // it out as a bare `Char` gave `∛x` and `root(3, x)` two different trees for
            // the same maths — which `round_trip.rs` caught the moment anything covered
            // this branch at all.
            let (name, cells) = match r.index() {
                Some(degree) => ("root", vec![vec![MathAtom::number(&degree.to_string())], radicand]),
                None => ("sqrt", vec![radicand]),
            };
            MathAtom { kind: Kind::MacroCall { name: name.into(), function: true }, cells }
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
            // A name the command file does not know is not a *command* — but it is still
            // a call, and its arguments are still parseable. So it becomes a `MacroCall`
            // all the same: the node stores the name and its arguments, and the frontend
            // draws the call's own source as one image while the caret is outside and
            // swaps to the name plus those argument slots once it enters
            // (`view::MACRO_COLLAPSED`, the `raw_macro` arrangement). That is the whole
            // difference from a configured name: the *shape* is unknown, so there is no
            // slot schema to borrow, and no template to expand.
            if !candidate_names().contains(&name.as_str()) {
                let Some(args) = positional_args(call, ctx) else {
                    return vec![MathAtom::from_source(node.full_text())];
                };
                return vec![MathAtom { kind: Kind::MacroCall { name, function: true }, cells: args }];
            }
            let mut args = vec![];
            // Cells per **row**, in source order: a comma keeps the count, a
            // semicolon ends a row. So `mat(1, 2, 3)` is one row of three and
            // `mat(1; 2; 3)` is three rows of one — the *columns* of the table are
            // the width of its first row, which is why every row has to agree before
            // this can be a table at all.
            //
            // The two loops below read the same argument list for two different
            // questions: this one wants the **rows** (so it watches the semicolons),
            // and `positional_args` wants the cells. A table is the only shape that
            // asks about rows, which is why it cannot go through that helper whole.
            let mut cells_per_row = vec![];
            let mut width = 0;
            for item in call.args().arg_items() {
                let ast::Arg::Pos(expr) = item.arg else { return vec![MathAtom::from_source(node.full_text())]; };
                let mut arg = parse_cell(expr.to_untyped(), ctx);
                if arg.len() == 1 && matches!(arg[0].kind, Kind::Text) && arg[0].cells[0].is_empty() { arg.clear(); }
                args.push(arg); width += 1;
                if item.ends_in_semicolon { cells_per_row.push(width); width = 0; }
            }
            if width > 0 { cells_per_row.push(width); }
            // A table's rows come from how the argument list is punctuated, and *which*
            // punctuation counts is the command file's answer — `mat` splits on
            // semicolons, while `vec` and `cases` take one argument per row however the
            // arguments are separated. Everything else about the table (its delimiters,
            // whether it is centred) follows from that same name.
            if let Some(rows) = slots::command_spec(&name).and_then(|spec| spec.rows) {
                let row_lengths = match rows {
                    // Read the original widths first, then pad each short row.
                    "mat" if !args.is_empty() => cells_per_row,
                    // One argument per row, so every row is one cell wide.
                    "mat" => { args = vec![vec![]; NEW_TABLE_CELLS]; vec![NEW_TABLE_COLUMNS; NEW_TABLE_COLUMNS] }
                    _ => vec![1; args.len()],
                };
                let columns = row_lengths.iter().copied().max().unwrap_or(1);
                // Each row is padded to `columns` **before** the rows are flattened, which
                // is what makes `chunks(columns)` give the rows back. Padding once at the
                // end instead would misalign every row after a short one: `mat(a; b, c)`
                // would flatten to `[a, b, c, _]` and be read back as rows `[a, b]` and
                // `[c, _]`. `Multiline` stores its rows the same way, for the same reason.
                let mut cells: Vec<MathData> = vec![];
                let mut at = 0;
                for &length in &row_lengths {
                    let mut row = args[at..at + length].to_vec();
                    at += length;
                    row.resize(columns, vec![]);
                    cells.extend(row);
                }
                // Padding is a real editable cell in a matrix. All rows must
                // expose and serialize it, otherwise typing there is discarded.
                let row_lengths = vec![columns; row_lengths.len()];
                return vec![MathAtom { kind: Kind::Table { columns, row_lengths, name: name.clone() }, cells }];
            }
            // The name is what says whether the editor has a shape for this call at
            // all — `slots::configured_shape` answers that, and the shape itself is
            // looked up again whenever the node is drawn or navigated.
            //
            // Nothing else about the argument list depends on the shape: the cells
            // are stored in the order they are written, which is why `root(3, x)`
            // keeps `3` first. (It used to be stored reversed, with a swap here
            // driven by the write template's `{1}` — see `slots::ROOT`.)
            if slots::configured_shape(&name).is_none() {
                return vec![MathAtom::from_source(node.full_text())];
            }
            // The node stores the **call**, not the shape: slots, view and navigation are
            // looked up from `config/commands.json` when it is drawn or edited, and
            // never kept in it. That is what lets the file decide what is structured —
            // and what keeps a node from holding a shape the file no longer declares.
            //
            // Which shape that is, and whether it still applies, is asked in
            // `MathAtom::command_shape` rather than decided here: a font variant depends on
            // its *body*, and the body changes as the node is edited.
            MathAtom { kind: Kind::MacroCall { name, function: true }, cells: args }
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
                // One atom per *grapheme cluster*, which is what the lexer put in
                // this node and what `GlyphItem` holds. Splitting by scalar here
                // would let `write_cell` put a separator inside a character, and
                // `e` + a combining accent would come back as `e ́`.
                return raw.graphemes(true).map(MathAtom::glyph).collect();
            } else { MathAtom::from_source(raw) }
        }
    };
    if matches!(atom.kind, Kind::Text) && atom.cells[0].is_empty() { vec![] } else { vec![atom] }
}

/// The positional arguments of a call, or `None` when it has any other kind of one.
///
/// A named argument (`delim: #none`), a spread or a trailing semicolon cannot be
/// represented by a plain cell list, so a call that has one stays source text. This is
/// what keeps `mat(x, delim: #none)` verbatim while `cancel(a + b)` is structured.
fn positional_args(call: ast::MathCall, ctx: &ParseContext) -> Option<Vec<MathData>> {
    let mut args = vec![];
    for item in call.args().arg_items() {
        let ast::Arg::Pos(expr) = item.arg else { return None };
        if item.ends_in_semicolon { return None; }
        let mut arg = parse_cell(expr.to_untyped(), ctx);
        // An empty text run is how the source spells an empty cell, and the parser
        // turns it back into one; anywhere else it is a cell with nothing in it.
        if arg.len() == 1 && matches!(arg[0].kind, Kind::Text) && arg[0].cells[0].is_empty() { arg.clear(); }
        args.push(arg);
    }
    Some(args)
}

/// Whether a body is a run of characters, which is the only shape a font variant has.
///
/// A variant is applied by substituting codepoints, so it needs characters to substitute.
/// Everything else — a fraction, a radical, an attachment, an accent, a table, a picture
/// (`Raw`) or another call the editor cannot shape (`raw_macro`) — has no single glyph run,
/// and the engine refuses to answer for it (measured).
///
/// A **nested variant** does qualify: `bold(upright(a))` resolves to one glyph (`𝐚`). Such
/// a body is a `MacroCall` whose shape is a `Style`, so it has to be asked through
/// `command_shape` rather than matched on `Kind` — both a variant and an unshapeable call
/// are `MacroCall` in the tree, and matching on the kind would reject the nested variant
/// along with the rest.
pub fn has_glyph_run(data: &[MathData]) -> bool {
    !data.is_empty() && data.iter().all(|cell| cell.iter().all(|atom| match &atom.kind {
        Kind::Char { .. } | Kind::Symbol { .. } | Kind::Number | Kind::Text => true,
        Kind::MacroCall { .. } => {
            atom.command_shape().is_some_and(|shape| shape.is_font_variant()) && has_glyph_run(&atom.cells)
        }
        _ => false,
    }))
}

/// The command names the parser may meet as an ordinary call.
///
/// `∛x`/`∜x` are a different syntax node (`MathRoot`, handled separately), but the
/// written commands are ordinary calls: `root(3, x)` is a `MathCall` like any other,
/// which is what lets the parser look every command up in the one table.
fn candidate_names() -> Vec<&'static str> {
    slots::command_names()
}

fn parse_marker(node: &SyntaxNode) -> MathData {    // A single marker can also be the complete expression in an argument.
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
/// Fills a `slots::Write::Template`: `{0}`, `{1}`… are cell indices.
///
/// There used to be a `{name}` placeholder for the shapes that spelled themselves
/// `{name}({0})` — `hat`, `bold` and the rest. Those are `MacroCall`s now and spell
/// themselves through `Write::Named`, and the one template left (`frac`) has no name
/// to substitute, so the placeholder and its `unreachable!` arm are gone. A template
/// that reached for it is caught by `slots.rs`'s `a_template_only_names_placeholders_that_exist`.
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
        match key.parse::<usize>().ok().and_then(|index| atom.cells.get(index)) {
            Some(cell) => out.push_str(&write_cell(cell)),
            // Keep an invented placeholder visible rather than dropping it,
            // so the round-trip test fails loudly instead of silently
            // writing a truncated node into the document.
            None => { debug_assert!(false, "模板占位符 {{{key}}} 没有对应的格子"); out.push_str(&rest[open..open + close + 1]); }
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
    match atom.grammar().write {
        Write::OwnText => match &atom.kind {
            Kind::Char { text } => text.clone(),
            Kind::Symbol { name, .. } | Kind::Raw { source: name } | Kind::Unknown { name, .. } => name.clone(),
            other => unreachable!("{other:?} 由自己的文本拼写，但它没有文本"),
        },
        Write::Marker => match &atom.kind {
            // A hole spells itself the way the **definition** writes it, because
            // that is the only scope a hole exists in: a template fragment that
            // reaches this function (a font variant's expression, a call the editor
            // cannot shape) has to be a fragment the definition's own text contains,
            // so `definitions`/`origin` can locate it. It used to be
            // `#parameter{index}` — a name invented here, which then leaked onto the
            // wire as if it were source (`docs/editing-model.md` §6.2).
            Kind::Parameter { name, .. } => format!("#{name}"),
            other => unreachable!("{other:?} 声明为占位拼写"),
        },
        Write::TemplateOnly => unreachable!("template edges never belong to the editable source tree"),
        Write::Quoted => serde_json::to_string(&atom.cells[0].iter().map(|a| if let Kind::Char { text } = &a.kind { text.clone() } else { write_atom(a) }).collect::<String>()).unwrap(),
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
            Kind::Table { columns, row_lengths, name } => {
                // Every matrix cell is real, including the empty cells used to
                // pad short input rows. Serialize the complete rectangle.
                //
                // An empty cell writes as *nothing* — `write_cell` answers a quoted empty
                // string for one, which is how a text run spells it, and `mat(a, ; c, d)`
                // is not `mat(a, "", c, d)`. The one exception is the **last cell of the
                // last row**: nothing follows it, so writing it bare would leave a
                // trailing comma and no argument, and `mat(, ; , )` would read back as
                // rows of two and *one*. An empty text run is the spelling that survives,
                // because the parser turns one back into an empty cell.
                let rows = slots::command_spec(name).and_then(|spec| spec.rows);
                let separator = if rows == Some("mat") { "; " } else { ", " };
                let last_row = row_lengths.len().saturating_sub(1);
                let body = atom.cells.chunks(*columns).enumerate().map(|(r, row)| {
                    let used = row.len();
                    row[..used].iter().enumerate().map(|(i, cell)| {
                        if !cell.is_empty() { write_cell(cell) }
                        else if r == last_row && i + 1 == used { "\"\"".to_string() }
                        else { String::new() }
                    }).collect::<Vec<_>>().join(", ")
                }).collect::<Vec<_>>().join(separator);
                format!("{name}({body})")
            }
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
