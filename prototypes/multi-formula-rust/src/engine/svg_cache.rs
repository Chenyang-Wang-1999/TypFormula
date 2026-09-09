//! Frozen editor SVGs. A source revision does not evict the last good image.
//! Only creation / an explicit refresh request publishes a new cached image.
use std::collections::{HashMap, HashSet};
use serde_json::{Value, json};
use crate::{Document, NodeId, Projection, ProjectionKind};
use super::Snapshot;

#[derive(Clone)]
pub(super) struct Target {
    pub key: String,
    pub node: NodeId,
    pub container: NodeId,
    pub slots: Vec<(String, NodeId)>,
}

struct Entry {
    target: Target,
    pending: bool,
    epoch: u64,
    value: Option<Value>,
    error: Option<String>,
    version: u64,
}

#[derive(Default)]
pub(super) struct SvgCache {
    pub(super) generation: u64,
    entries: HashMap<String, Entry>,
}

pub(super) fn targets(document: &Document) -> Vec<Target> {
    fn visit(node: &Projection, formula: NodeId, owner: u64, output: &mut Vec<Target>) {
        if matches!(node.kind, ProjectionKind::Raw | ProjectionKind::Script) {
            output.push(Target { key: format!("{owner}:{}", node.cache_id), node: node.origin,
                container: formula, slots: if node.kind == ProjectionKind::Script {
                    node.slots.iter().map(|s| (s.role.clone(), s.origin)).collect()
                } else { vec![] } });
        }
        for slot in &node.slots { for child in &slot.children { visit(child, formula, owner, output); } }
    }
    let mut output = vec![];
    for formula in &document.formulas {
        let id = formula.projection.cache_id;
        output.push(Target { key: format!("{id}:{id}"), node: formula.node, container: formula.node, slots: vec![] });
        if formula.valid { visit(&formula.projection, formula.node, id, &mut output); }
    }
    output
}

impl SvgCache {
    pub(super) fn reconcile(&mut self, targets: Vec<Target>, epoch: u64) {
        self.generation += 1;
        let alive: HashSet<_> = targets.iter().map(|t| t.key.clone()).collect();
        self.entries.retain(|key, _| alive.contains(key));
        for target in targets {
            if let Some(entry) = self.entries.get_mut(&target.key) { entry.target = target; }
            else {
                self.entries.insert(target.key.clone(), Entry {
                    target, pending: true, epoch, value: None, error: None, version: self.generation,
                });
            }
        }
    }

    pub(super) fn request(&mut self, nodes: &[NodeId], formulas: &[NodeId], all: bool, epoch: u64) {
        self.generation += 1;
        for entry in self.entries.values_mut() {
            if all || nodes.contains(&entry.target.node) || formulas.contains(&entry.target.container) {
                entry.pending = true; entry.epoch = epoch; entry.error = None;
                entry.version = self.generation;
            }
        }
    }

    pub(super) fn fail(&mut self, message: &str) {
        self.generation += 1;
        for entry in self.entries.values_mut().filter(|e| e.pending) {
            entry.error = Some(message.into()); entry.version = self.generation;
        }
    }

    pub(super) fn publish(&mut self, snapshot: &mut Snapshot) {
        self.generation += 1;
        for entry in self.entries.values_mut() {
            if !entry.pending || entry.target.node.revision != snapshot.revision || entry.epoch > snapshot.epoch { continue; }
            let target = &entry.target;
            let pieces: Vec<_> = snapshot.fragments.get_mut(&target.node).into_iter().flatten()
                .filter(|f| target.node == target.container || f.ancestors.contains(&target.container))
                .map(|f| f.json()).collect();
            // Keep the last success even if show rules remove an output or a
            // requested AST node has no independently traceable fragment.
            if pieces.is_empty() {
                entry.error = Some("这个节点没有独立输出；保留旧缓存（若有）".into());
            } else {
                let mut layout = serde_json::Map::new();
                // Determine side-vs-limits from actual placed slot bounds, not
                // from the operator's spelling. Ambiguous instances fall back.
                if let Some((_, base)) = target.slots.iter().find(|(role, _)| role == "base") {
                    let matching = |id: &NodeId| snapshot.fragments.get(id).and_then(|items| {
                        let mut items = items.iter().filter(|f| f.ancestors.contains(&target.node));
                        let first = items.next()?;
                        if items.next().is_some() { None } else { Some(first) }
                    });
                    if let Some(base) = matching(base) {
                        for (role, id) in &target.slots {
                            if role == "base" { continue; }
                            if let Some(side) = matching(id) {
                                if side.page == base.page {
                                    let centered = side.bounds.min.x < base.bounds.max.x &&
                                        (side.bounds.min.x + side.bounds.max.x - base.bounds.min.x - base.bounds.max.x).abs().to_pt() < 3.0;
                                    layout.insert(role.clone(), json!(if centered { "limits" } else { "side" }));
                                }
                            }
                        }
                    }
                }
                entry.value = Some(json!({"revision": snapshot.revision, "render_generation": snapshot.generation,
                    "fragments": pieces, "layout": layout}));
                entry.error = None;
            }
            entry.pending = false;
            entry.version = self.generation;
        }
    }

    pub(super) fn json(&self, revision: u64, since: Option<u64>) -> Value {
        let entries: Vec<_> = self.entries.iter().filter(|(_, entry)| since.is_none_or(|v| entry.version > v)).map(|(key, entry)| json!({
            "key": key, "pending": entry.pending, "error": entry.error, "cached": entry.value,
            // Conservative: we don't claim a frozen image is context-independent.
            "stale": entry.value.as_ref().is_some_and(|v| v["revision"].as_u64() != Some(revision))
        })).collect();
        json!({"revision": revision, "generation": self.generation, "entries": entries,
            "alive": self.entries.keys().collect::<Vec<_>>()})
    }
}
