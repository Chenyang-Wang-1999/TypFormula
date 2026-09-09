//! Lexical index only. Typst, not this index, executes bindings and show rules.
use serde::Serialize;
use typst_syntax::{ast::{self, AstNode}, SyntaxKind, SyntaxNode};
use crate::{Document, NodeId};

pub type EnvironmentId = usize;

#[derive(Clone, Debug, Serialize)]
pub struct Environment {
    pub parent: Option<EnvironmentId>,
    pub scope: Option<NodeId>,
    pub binding: Option<usize>,
    pub opaque: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Definition {
    pub node: NodeId,
    pub cache_id: u64,
    pub range: std::ops::Range<usize>,
    pub names: Vec<String>,
    pub body: Option<NodeId>,
    pub parameters: Vec<String>,
    pub function: bool,
    pub expandable: bool,
    pub captured: EnvironmentId,
}

#[derive(Default, Debug, Serialize)]
pub struct MacroIndex {
    pub environments: Vec<Environment>,
    pub definitions: Vec<Definition>,
}

impl MacroIndex {
    pub fn scan(document: &Document) -> (Self, Vec<(NodeId, EnvironmentId)>) {
        let mut index = Self { environments: vec![Environment {
            parent: None, scope: None, binding: None, opaque: false,
        }], definitions: vec![] };
        let mut formulas = vec![];
        index.walk(document, document.source().root(), 0, &mut formulas);
        (index, formulas)
    }

    fn push(&mut self, parent: EnvironmentId, scope: Option<NodeId>, binding: Option<usize>, opaque: bool) -> EnvironmentId {
        let id = self.environments.len();
        self.environments.push(Environment { parent: Some(parent), scope, binding, opaque });
        id
    }

    pub fn visible(&self, mut env: EnvironmentId, name: &str) -> Option<&Definition> {
        loop {
            let current = &self.environments[env];
            if current.opaque { return None; }
            if let Some(i) = current.binding {
                let definition = &self.definitions[i];
                if definition.names.iter().any(|n| n == name) { return Some(definition); }
            }
            env = current.parent?;
        }
    }

    pub fn uncertain(&self, mut env: EnvironmentId) -> bool {
        loop {
            let current = &self.environments[env];
            if current.opaque { return true; }
            let Some(parent) = current.parent else { return false; };
            env = parent;
        }
    }

    pub fn visible_definitions(&self, mut env: EnvironmentId) -> Vec<&Definition> {
        let mut seen = std::collections::HashSet::new();
        let mut definitions = vec![];
        loop {
            let current = &self.environments[env];
            if current.opaque { break; }
            if let Some(i) = current.binding {
                let definition = &self.definitions[i];
                let fresh = definition.names.iter().any(|name| !seen.contains(name));
                seen.extend(definition.names.iter().cloned());
                if fresh { definitions.push(definition); }
            }
            let Some(parent) = current.parent else { break; };
            env = parent;
        }
        definitions.reverse();
        definitions
    }

    fn walk(&mut self, doc: &Document, node: &SyntaxNode, env: EnvironmentId,
            formulas: &mut Vec<(NodeId, EnvironmentId)>) -> EnvironmentId {
        match node.kind() {
            SyntaxKind::Equation => { formulas.push((doc.id_for(node), env)); env }
            SyntaxKind::LetBinding => {
                // Don't call typed accessors on an incomplete binding, or
                // expand older definitions through a potentially shadowing let.
                if node.diagnosis().errors { return self.push(env, None, None, true); }
                let binding = node.cast::<ast::LetBinding>().unwrap();
                let names = binding.kind().bindings().iter().map(|n| n.as_str().to_owned()).collect();
                let mut body = binding.init();
                let mut parameters = vec![];
                let mut function = false;
                let mut expandable = true;
                if let Some(ast::Expr::Closure(closure)) = body {
                    function = true;
                    for param in closure.params().children() {
                        match param {
                            ast::Param::Pos(ast::Pattern::Normal(ast::Expr::Ident(name))) => parameters.push(name.as_str().to_owned()),
                            _ => expandable = false,
                        }
                    }
                    body = Some(closure.body());
                }
                // Executable blocks are opaque; direct math/value bodies are
                // projected without evaluating Typst or copying their source.
                expandable &= body.is_some_and(|b| matches!(b,
                    ast::Expr::Equation(_) | ast::Expr::Math(_) | ast::Expr::Int(_) |
                    ast::Expr::Float(_) | ast::Expr::Str(_) | ast::Expr::MathIdent(_)));
                let definition = self.definitions.len();
                self.definitions.push(Definition { node: doc.id_for(node), cache_id: doc.node(doc.id_for(node)).unwrap().cache_id, range: doc.range_of(node), names,
                    body: body.map(|b| doc.id_for(b.to_untyped())), parameters,
                    function, expandable, captured: env });
                self.push(env, None, Some(definition), false)
            }
            SyntaxKind::ContentBlock | SyntaxKind::CodeBlock => {
                let child_env = self.push(env, Some(doc.id_for(node)), None, false);
                self.children(doc, node, child_env, formulas);
                fn dynamic(node: &SyntaxNode) -> bool {
                    matches!(node.kind(), SyntaxKind::Binary | SyntaxKind::DestructAssignment) || node.children().any(dynamic)
                }
                if dynamic(node) { self.push(env, None, None, true) } else { env }
            }
            SyntaxKind::ModuleImport | SyntaxKind::ModuleInclude => self.push(env, None, None, true),
            SyntaxKind::Error => self.push(env, None, None, true),
            SyntaxKind::Conditional | SyntaxKind::ForLoop | SyntaxKind::WhileLoop => {
                let uncertain = self.push(env, Some(doc.id_for(node)), None, true);
                self.children(doc, node, uncertain, formulas);
                self.push(env, None, None, true)
            }
            SyntaxKind::DestructAssignment | SyntaxKind::Binary => {
                self.children(doc, node, env, formulas);
                self.push(env, None, None, true)
            }
            _ => self.children(doc, node, env, formulas),
        }
    }

    fn children(&mut self, doc: &Document, node: &SyntaxNode, mut env: EnvironmentId,
                formulas: &mut Vec<(NodeId, EnvironmentId)>) -> EnvironmentId {
        for child in node.children() { env = self.walk(doc, child, env, formulas); }
        env
    }
}
