//! Disposable macro instances. Plans never write back into the editing document.
use crate::{math::{Kind, MathData}, typst};
use serde_json::{Value, json};
use typst_syntax::{Source, SyntaxKind, SyntaxNode};

pub struct Plan { pub key: String, pub request: Value }

pub fn plans(source: &str) -> Vec<Plan> {
    fn visit(node: &SyntaxNode, at: usize, closers: &str, code: bool, source: &str, out: &mut Vec<Plan>) {
        let end = at + node.len();
        if node.kind() == SyntaxKind::LetBinding && node.errors_and_warnings().0.is_empty() {
            let prefix = &source[..end];
            let registry = typst::macro_registry(prefix);
            if let Some(def) = registry.entries.last().filter(|d| d.expandable) {
                if !typst::warmup_within_limit(def) {
                    out.push(Plan { key:typst::warmup_key(def), request:json!({"warmup_error":"模板展开超过编辑器预算，保留普通调用"}) });
                    return;
                }
                let expression = if def.function { format!("#{}({})", def.name, vec!["\"\""; def.params.len()].join(", ")) } else { format!("#{}",def.name) };
                let equation = format!("$ {expression} $");
                let start = prefix.len() + 1 + usize::from(code);
                let body = if code { format!("[{equation}]") } else { equation.clone() };
                let projection = format!("{prefix}\n{body}\n{closers}");
                let mut raw = vec![];
                collect_raw(&def.template, def, &registry, &mut raw, &mut std::collections::HashSet::new());
                out.push(Plan { key:typst::warmup_key(def), request:json!({"source":projection,"raw":raw,"formulas":[{"id":"0","start":start,"end":start+equation.len()}]}) });
            }
        }
        let closers = match node.kind() {
            SyntaxKind::ContentBlock => format!("]{closers}"),
            SyntaxKind::CodeBlock => format!("}}{closers}"),
            SyntaxKind::Args | SyntaxKind::Parenthesized => format!("){closers}"),
            _ => closers.to_owned(),
        };
        let code = match node.kind() { SyntaxKind::CodeBlock | SyntaxKind::Args => true, SyntaxKind::ContentBlock => false, _ => code };
        let mut offset = at;
        for child in node.children() { visit(child, offset, &closers, code, source, out); offset += child.len(); }
    }
    let mut out = vec![];
    visit(Source::detached(source).root(), 0, "", false, source, &mut out);
    out
}

fn collect_raw(data: &MathData, def: &typst::MacroDefinition, registry: &typst::MacroRegistry, out: &mut Vec<Value>, visited: &mut std::collections::HashSet<usize>) {
    for atom in data {
        if let Kind::TemplateCall { definition } = atom.kind {
            let callee = &registry.entries[definition];
            // Visit each definition once. One labelled source range can produce
            // multiple occurrence SVGs when a template calls its dependency twice.
            if visited.insert(definition) { collect_raw(&callee.template, callee, registry, out, visited); }
        }
        if let Kind::Raw { source } = &atom.kind {
            for (start,end) in definition_raw_ranges(def, source) {
                let id = format!("{start}:{end}");
                if !out.iter().any(|r|r["id"]==id) { out.push(json!({"id":id,"start":start,"end":end})); }
            }
        }
        for cell in &atom.cells { collect_raw(cell, def, registry, out, visited); }
    }
}
pub fn definition_raw_ranges(def: &typst::MacroDefinition, text: &str) -> Vec<(usize,usize)> {
    raw_ranges(&typst::warmup_key(def),0,text).into_iter().filter(|(start,_)|*start>=def.definition_start).collect()
}
pub fn raw_ranges(source: &str, offset: usize, text: &str) -> Vec<(usize,usize)> {
    fn visit(node: &SyntaxNode, at: usize, text: &str, out: &mut Vec<(usize,usize)>) {
        if node.full_text().as_str()==text { out.push((at,at+node.len())); return; }
        let mut pos = at;
        let children: Vec<_> = node.children().collect();
        for (i,child) in children.iter().enumerate() {
            if child.kind()==SyntaxKind::Hash && children.get(i+1).is_some_and(|next|format!("#{}",next.full_text())==text) {
                out.push((pos,pos+text.len()));
            } else { visit(child,pos,text,out); }
            pos += child.len();
        }
    }
    let mut out = vec![]; visit(Source::detached(source).root(),offset,text,&mut out); out
}
