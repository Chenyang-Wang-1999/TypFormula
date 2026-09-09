use std::{collections::HashMap, ops::Range, sync::atomic::{AtomicU64, Ordering}};
use serde::{Deserialize, Serialize};
use typst_syntax::{ast::{self, AstNode}, Source, Span, SyntaxKind, SyntaxNode};
use crate::{macros::{EnvironmentId, MacroIndex}, projection::{self, Projection}};

static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);

/// Serializable reference to a node in one immutable document revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct NodeId { pub document: u64, pub revision: u64, pub index: u32 }

#[derive(Clone, Debug, Serialize)]
pub struct NodeRecord {
    pub id: NodeId,
    /// Editor/cache identity, distinct from the revision-checked syntax ID.
    pub cache_id: u64,
    pub range: Range<usize>,
    #[serde(skip)]
    kind: SyntaxKind,
    #[serde(skip)]
    pub span: Span,
}

#[derive(Clone, Debug, Serialize)]
pub struct Formula {
    pub node: NodeId,
    pub valid: bool,
    pub body: Range<usize>,
    pub display: bool,
    pub environment: EnvironmentId,
    pub projection: Projection,
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic { pub message: String, pub range: Option<Range<usize>> }

#[derive(Debug)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for Error {}

/// Owns the only editable Source. Both the compiler and projection use this
/// tree. A Source clone is a reference-counted snapshot, not another parse.
pub struct Document {
    pub(crate) source: Source,
    token: u64,
    revision: u64,
    next_cache_id: u64,
    nodes: Vec<NodeRecord>,
    by_span: HashMap<Span, NodeId>,
    pub macros: MacroIndex,
    pub formulas: Vec<Formula>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Document {
    pub fn parse(text: impl Into<String>) -> Self { Self::from_source(Source::detached(text.into())) }

    pub fn from_source(source: Source) -> Self {
        let mut document = Self {
            source, token: NEXT_DOCUMENT.fetch_add(1, Ordering::Relaxed), revision: 0, next_cache_id: 1,
            nodes: vec![], by_span: HashMap::new(), macros: MacroIndex::default(),
            formulas: vec![], diagnostics: vec![],
        };
        document.reindex(HashMap::new());
        document
    }

    pub fn revision(&self) -> u64 { self.revision }
    pub fn source(&self) -> &Source { &self.source }
    pub fn nodes(&self) -> &[NodeRecord] { &self.nodes }

    pub fn node(&self, id: NodeId) -> Result<&NodeRecord, Error> {
        if id.document != self.token || id.revision != self.revision {
            return Err(Error("节点属于另一个文档或旧版本，请刷新投影".into()));
        }
        self.nodes.get(id.index as usize).ok_or_else(|| Error("未知源码节点".into()))
    }

    pub fn syntax(&self, id: NodeId) -> Result<&SyntaxNode, Error> {
        let span = self.node(id)?.span;
        self.source.find(span).map(|linked| linked.get()).ok_or_else(|| Error("节点已不存在".into()))
    }

    pub fn text(&self, id: NodeId) -> Result<&str, Error> {
        Ok(&self.source.text()[self.node(id)?.range.clone()])
    }
    pub fn id_for(&self, node: &SyntaxNode) -> NodeId { self.by_span[&node.span()] }
    pub fn range_of(&self, node: &SyntaxNode) -> Range<usize> {
        self.nodes[self.id_for(node).index as usize].range.clone()
    }

    pub fn replace(&mut self, text: &str) -> bool {
        if self.source.text() == text { return false; }
        self.source.replace(text);
        self.revision += 1;
        self.reindex(HashMap::new());
        true
    }

    pub fn edit(&mut self, revision: u64, range: Range<usize>, text: &str) -> Result<(), Error> {
        if revision != self.revision { return Err(Error("文档已改变，不能提交旧版本的编辑".into())); }
        if range.start > range.end || self.source.text().get(range.clone()).is_none() {
            return Err(Error("编辑范围不在 UTF-8 字符边界上".into()));
        }
        // Preserve identities only for unaffected nodes or enclosing nodes.
        // Never match by spelling: identical formulas are different instances.
        let delta = text.len() as isize - range.len() as isize;
        let mut identities = HashMap::new();
        for node in &self.nodes {
            let old = &node.range;
            let mapped = if old.end <= range.start { Some(old.clone()) }
                else if old.start >= range.end { Some(old.start.checked_add_signed(delta).unwrap()..old.end.checked_add_signed(delta).unwrap()) }
                else if old.start <= range.start && old.end >= range.end {
                    Some(old.start..old.end.checked_add_signed(delta).unwrap())
                } else { None };
            if let Some(mapped) = mapped {
                identities.insert((mapped.start, mapped.end, node.kind), node.cache_id);
            }
        }
        self.source.edit(range, text);
        self.revision += 1;
        self.reindex(identities);
        Ok(())
    }

    fn reindex(&mut self, mut identities: HashMap<(usize, usize, SyntaxKind), u64>) {
        self.nodes.clear();
        self.by_span.clear();
        fn visit(node: &SyntaxNode, start: usize, document: u64, revision: u64,
                 nodes: &mut Vec<NodeRecord>, by_span: &mut HashMap<Span, NodeId>,
                 identities: &mut HashMap<(usize, usize, SyntaxKind), u64>, next: &mut u64) {
            let id = NodeId { document, revision, index: nodes.len() as u32 };
            let previous = identities.remove(&(start, start + node.len(), node.kind())).or_else(|| {
                // Error recovery can temporarily widen an equation to absorb
                // following text. Keep its repair editor and last good SVG,
                // but never invent syntax identities for the absorbed nodes.
                if node.kind() != SyntaxKind::Equation || !node.diagnosis().errors { return None; }
                let key = identities.keys().find(|(a, _, kind)| *a == start && *kind == SyntaxKind::Equation).copied()?;
                identities.remove(&key)
            });
            let cache_id = previous.unwrap_or_else(|| {
                let id = *next; *next += 1; id
            });
            nodes.push(NodeRecord { id, cache_id, kind: node.kind(), range: start..start + node.len(), span: node.span() });
            by_span.insert(node.span(), id);
            let mut offset = start;
            for child in node.children() {
                visit(child, offset, document, revision, nodes, by_span, identities, next);
                offset += child.len();
            }
        }
        visit(self.source.root(), 0, self.token, self.revision, &mut self.nodes, &mut self.by_span, &mut identities, &mut self.next_cache_id);
        self.diagnostics = self.source.root().errors_and_warnings().0.into_iter().map(|error| Diagnostic {
            message: error.message.to_string(), range: None,
        }).collect();
        let (macros, formulas) = MacroIndex::scan(self);
        self.macros = macros;
        self.formulas = formulas.into_iter().map(|(node, environment)| {
            let equation = self.syntax(node).unwrap().cast::<ast::Equation>().unwrap();
            let valid = !equation.to_untyped().diagnosis().errors;
            let projection = projection::formula(self, equation, environment);
            Formula { node, valid, body: projection.slots[0].range.clone(),
                display: equation.block(), environment,
                projection }
        }).collect();
    }
}
