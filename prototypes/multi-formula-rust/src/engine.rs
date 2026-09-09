//! A long-lived document compiler. Fragment requests only export cached layout;
//! they never construct an isolated Typst source or start another compiler.
mod fragments;
mod svg_cache;

use std::{collections::{HashMap, HashSet}, fs, path::PathBuf, sync::{Arc, Mutex, mpsc, atomic::{AtomicU64, Ordering}}, thread, time::Instant};
use serde_json::{Value, json};
use typst::{Library, LibraryExt, World, diag::{FileError, FileResult},
    foundations::{Bytes, Datetime, Duration}, text::{Font, FontBook}, utils::LazyHash};
use typst_kit::{datetime::Time, fonts::{self, FontStore}};
use typst_layout::PagedDocument;
use typst_syntax::{FileId, Source, Span, VirtualRoot};
use crate::{Diagnostic, Document, Error, NodeId};

pub struct Backend {
    pub document: Mutex<Document>,
    state: Mutex<CompileState>,
    queue: mpsc::SyncSender<()>,
    foundation: Arc<Foundation>,
    svg: Mutex<svg_cache::SvgCache>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::layout::{Frame, FrameItem, Transform};

    fn foundation() -> Arc<Foundation> {
        let mut fonts = FontStore::new();
        fonts.extend(fonts::embedded());
        Arc::new(Foundation { library: LazyHash::new(Library::default()), fonts,
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize().unwrap(),
            files: Mutex::new(HashMap::new()), epoch: AtomicU64::new(0) })
    }

    fn draw_list(frame: &Frame, transform: Transform, output: &mut Vec<(Transform, FrameItem)>) {
        for (position, item) in frame.items() {
            let local = transform.pre_concat(Transform::translate(position.x, position.y));
            match item {
                FrameItem::Tag(_) => {},
                FrameItem::Group(group) => draw_list(&group.frame, local.pre_concat(group.transform), output),
                _ => output.push((local, item.clone())),
            }
        }
    }

    #[test]
    fn provenance_markers_preserve_geometry_and_drawn_items() {
        let document = Document::parse(
            "#set page(width: 200pt, height: auto)\n#show math.equation: set text(fill: red)\n$ cancel(a + b) + frac(c, d) $",
        );
        let mut nodes = vec![];
        document.formulas[0].projection.origins(&mut nodes);
        assert!(!nodes.is_empty());
        let mut world = CompileWorld { foundation: foundation(), source: document.source().clone(),
            origins: HashSet::new(), time: Time::system() };
        let plain = typst::compile::<PagedDocument>(&world).output.unwrap();
        world.origins = nodes.iter().map(|&id| document.node(id).unwrap().span).collect();
        let traced = typst::compile::<PagedDocument>(&world).output.unwrap();
        assert_eq!(plain.pages().len(), traced.pages().len());
        for (a, b) in plain.pages().iter().zip(traced.pages()) {
            assert_eq!(a.frame.size(), b.frame.size());
            let mut original = vec![];
            let mut marked = vec![];
            draw_list(&a.frame, Transform::identity(), &mut original);
            draw_list(&b.frame, Transform::identity(), &mut marked);
            assert_eq!(typst::utils::hash128(&original), typst::utils::hash128(&marked));
        }
        let targets = nodes.iter().map(|&id| (document.node(id).unwrap().span, id)).collect();
        let mut output = fragments::collect(&traced, &targets);
        let raw = output.get_mut(&nodes[0]).expect("Raw should retain its origin in the finished frame");
        assert_eq!(raw.len(), 1);
        assert!(raw[0].json()["svg"].as_str().unwrap().contains("<svg"));
    }

    #[test]
    fn fragment_export_does_not_compile_or_read_the_editable_source() {
        let document = Document::parse("$x + y$");
        let formula = document.formulas[0].node;
        let foundation = foundation();
        let world = CompileWorld { foundation: foundation.clone(), source: document.source().clone(),
            origins: HashSet::new(), time: Time::system() };
        let output = typst::compile::<PagedDocument>(&world).output.unwrap();
        let targets = [(document.node(formula).unwrap().span, formula)].into_iter().collect();
        let pieces = fragments::collect(&output, &targets);
        assert!(pieces.contains_key(&formula));
        let (queue, _receiver) = mpsc::sync_channel(1);
        let backend = Backend { document: Mutex::new(document), foundation, queue, svg: Mutex::default(),
            state: Mutex::new(CompileState { busy: false, attempted: Some(0), runs: 1, milliseconds: 0,
                diagnostics: vec![], snapshot: Some(Snapshot { revision: 0, generation: 1, epoch: 0,
                    document: output, fragments: pieces, pages: HashMap::new() }) }) };
        // Holding this lock also proves that export never waits on source parsing.
        let _source_guard = backend.document.lock().unwrap();
        let first = backend.fragment(formula, None).unwrap();
        let second = backend.fragment(formula, None).unwrap();
        assert_eq!(first, second);
        assert_eq!(backend.state_json()["compile_runs"], 1);
    }

    fn frozen_backend(source: &str) -> Backend {
        let (queue, _receiver) = mpsc::sync_channel(1);
        let backend = Backend { document: Mutex::new(Document::parse(source)), foundation: foundation(), queue,
            svg: Mutex::default(), state: Mutex::new(CompileState { busy: false, attempted: None, runs: 0,
                milliseconds: 0, diagnostics: vec![], snapshot: None }) };
        backend.sync_svg_targets(); backend.compile(); backend
    }

    fn cached(backend: &Backend, key: &str) -> Value {
        backend.svg_json(None)["entries"].as_array().unwrap().iter()
            .find(|entry| entry["key"] == key).unwrap().clone()
    }

    #[test]
    fn frozen_svg_survives_compile_and_selected_refresh_does_not_recompile() {
        let backend = frozen_backend("#let value = 1\n$value$ and $cancel(x)$");
        let (a, b, raw) = {
            let document = backend.document.lock().unwrap();
            let a = document.formulas[0].projection.cache_id;
            let b = document.formulas[1].projection.cache_id;
            let mut origins = vec![];
            document.formulas[1].projection.origins(&mut origins);
            let raw = document.node(origins[0]).unwrap().cache_id;
            (format!("{a}:{a}"), format!("{b}:{b}"), format!("{b}:{raw}"))
        };
        let original_a = cached(&backend, &a)["cached"].clone();
        let original_b = cached(&backend, &b)["cached"].clone();
        let original_raw = cached(&backend, &raw)["cached"].clone();
        assert!(!original_a.is_null()); assert!(!original_raw.is_null());
        {
            let mut document = backend.document.lock().unwrap();
            let start = document.source().text().find('1').unwrap();
            document.edit(0, start..start+1, "2").unwrap();
        }
        backend.sync_svg_targets(); backend.compile();
        assert_eq!(cached(&backend, &a)["cached"], original_a);
        assert_eq!(cached(&backend, &b)["cached"], original_b);
        assert_eq!(cached(&backend, &raw)["cached"], original_raw);
        let node = backend.document.lock().unwrap().formulas[0].node;
        let runs = backend.state_json()["compile_runs"].clone();
        backend.refresh_svg(1, &[node], &[], false, false).unwrap();
        assert_eq!(backend.state_json()["compile_runs"], runs);
        assert_eq!(cached(&backend, &a)["cached"]["revision"], 1);
        assert_eq!(cached(&backend, &b)["cached"], original_b);
        let raw_node = {
            let document = backend.document.lock().unwrap();
            let mut origins = vec![]; document.formulas[1].projection.origins(&mut origins); origins[0]
        };
        backend.refresh_svg(1, &[], &[raw_node], false, false).unwrap();
        assert_eq!(cached(&backend, &raw)["cached"]["revision"], 1);
        assert_eq!(cached(&backend, &b)["cached"], original_b);
        backend.refresh_svg(1, &[], &[], true, false).unwrap();
        assert_eq!(cached(&backend, &b)["cached"]["revision"], 1);
        let generation = backend.svg.lock().unwrap().generation;
        assert!(backend.svg_json(Some(generation))["entries"].as_array().unwrap().is_empty());
    }

    #[test]
    fn failed_refresh_retains_last_good_svg_then_retries_after_repair() {
        let backend = frozen_backend("$x$");
        let id = backend.document.lock().unwrap().formulas[0].projection.cache_id;
        let key = format!("{id}:{id}");
        let original = cached(&backend, &key)["cached"].clone();
        backend.document.lock().unwrap().edit(0, 1..2, "not-defined()").unwrap();
        backend.sync_svg_targets();
        backend.refresh_svg(1, &[], &[], true, false).unwrap();
        backend.compile();
        let failed = cached(&backend, &key);
        assert_eq!(failed["cached"], original);
        assert_eq!(failed["pending"], true);
        assert!(failed["error"].is_string());
        {
            let mut document = backend.document.lock().unwrap();
            let range = document.formulas[0].body.clone();
            document.edit(1, range, "y").unwrap();
        }
        backend.sync_svg_targets(); backend.compile();
        assert_eq!(cached(&backend, &key)["cached"]["revision"], 2);
        assert_eq!(cached(&backend, &key)["pending"], false);
    }
}

struct CompileState {
    busy: bool,
    attempted: Option<u64>,
    runs: u64,
    milliseconds: u128,
    diagnostics: Vec<Diagnostic>,
    snapshot: Option<Snapshot>,
}

struct Snapshot {
    revision: u64,
    generation: u64,
    epoch: u64,
    document: PagedDocument,
    fragments: HashMap<NodeId, Vec<fragments::Fragment>>,
    pages: HashMap<usize, String>,
}

struct Foundation {
    library: LazyHash<Library>,
    fonts: FontStore,
    root: PathBuf,
    files: Mutex<HashMap<FileId, CachedFile>>,
    epoch: AtomicU64,
}
struct CachedFile { bytes: Bytes, source: Option<Source> }

struct CompileWorld {
    foundation: Arc<Foundation>,
    source: Source,
    origins: HashSet<Span>,
    time: Time,
}

impl World for CompileWorld {
    fn library(&self) -> &LazyHash<Library> { &self.foundation.library }
    fn book(&self) -> &LazyHash<FontBook> { self.foundation.fonts.book() }
    fn main(&self) -> FileId { self.source.id() }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main() { return Ok(self.source.clone()); }
        let bytes = self.file(id)?;
        let mut files = self.foundation.files.lock().unwrap();
        let cached = files.entry(id).or_insert_with(|| CachedFile { bytes: bytes.clone(), source: None });
        if let Some(source) = &cached.source { return Ok(source.clone()); }
        let text = std::str::from_utf8(bytes.as_slice()).map_err(|_| FileError::InvalidUtf8)?;
        let source = Source::new(id, text.into());
        cached.source = Some(source.clone());
        Ok(source)
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main() { return Ok(Bytes::from_string(self.source.clone())); }
        let mut files = self.foundation.files.lock().unwrap();
        if let Some(file) = files.get(&id) { return Ok(file.bytes.clone()); }
        // Local imports and resources use the actual World. They are not
        // copied into the macro index. Package downloading is not implicit.
        if !matches!(id.root(), VirtualRoot::Project) { return Err(FileError::AccessDenied); }
        let path = id.vpath().realize(&self.foundation.root).map_err(|_| FileError::AccessDenied)?;
        let resolved = path.canonicalize().map_err(|e| FileError::from_io(e, &path))?;
        if !resolved.starts_with(&self.foundation.root) { return Err(FileError::AccessDenied); }
        let bytes = Bytes::new(fs::read(&resolved).map_err(|e| FileError::from_io(e, &resolved))?);
        files.insert(id, CachedFile { bytes: bytes.clone(), source: None });
        Ok(bytes)
    }
    fn font(&self, index: usize) -> Option<Font> { self.foundation.fonts.font(index) }
    fn today(&self, offset: Option<Duration>) -> Option<Datetime> { self.time.today(offset) }
    fn editor_math_origin(&self, span: Span) -> bool { self.origins.contains(&span) }
}

impl Backend {
    pub fn new(source: Source, root: PathBuf) -> Result<Arc<Self>, Error> {
        let mut fonts = FontStore::new();
        fonts.extend(fonts::embedded());
        fonts.extend(fonts::system());
        let root = root.canonicalize().map_err(|e| Error(e.to_string()))?;
        let foundation = Arc::new(Foundation { library: LazyHash::new(Library::default()),
            fonts, root, files: Mutex::new(HashMap::new()), epoch: AtomicU64::new(0) });
        let (queue, receiver) = mpsc::sync_channel(1);
        let backend = Arc::new(Self { document: Mutex::new(Document::from_source(source)),
            state: Mutex::new(CompileState { busy: false, attempted: None, runs: 0, milliseconds: 0,
                diagnostics: vec![], snapshot: None }), queue, foundation, svg: Mutex::default() });
        let weak = Arc::downgrade(&backend);
        thread::spawn(move || {
            while receiver.recv().is_ok() {
                // Coalesce a burst of keystrokes. This worker never owns the
                // editable document lock while compiling.
                while receiver.recv_timeout(std::time::Duration::from_millis(180)).is_ok() {}
                let Some(backend) = weak.upgrade() else { break; };
                backend.compile();
            }
        });
        backend.sync_svg_targets();
        backend.schedule();
        Ok(backend)
    }

    pub fn schedule(&self) { let _ = self.queue.try_send(()); }

    pub fn sync_svg_targets(&self) {
        let document = self.document.lock().unwrap();
        self.svg.lock().unwrap().reconcile(svg_cache::targets(&document), self.foundation.epoch.load(Ordering::SeqCst));
    }

    pub fn svg_json(&self, since: Option<u64>) -> Value {
        let document = self.document.lock().unwrap();
        self.svg.lock().unwrap().json(document.revision(), since)
    }

    pub fn refresh_svg(&self, revision: u64, formulas: &[NodeId], nodes: &[NodeId], all: bool, dependencies: bool) -> Result<Value, Error> {
        let document = self.document.lock().unwrap();
        if revision != document.revision() { return Err(Error("刷新请求属于旧版本".into())); }
        for &node in formulas.iter().chain(nodes) { document.node(node)?; }
        if dependencies {
            self.foundation.files.lock().unwrap().clear();
            self.foundation.epoch.fetch_add(1, Ordering::SeqCst);
        }
        let epoch = self.foundation.epoch.load(Ordering::SeqCst);
        // Lock order is always document -> state -> svg.
        let mut state = self.state.lock().unwrap();
        let mut svg = self.svg.lock().unwrap();
        svg.request(nodes, formulas, all, epoch);
        let ready = state.snapshot.as_ref().is_some_and(|s| s.revision == revision && s.epoch == epoch);
        if ready { svg.publish(state.snapshot.as_mut().unwrap()); }
        else { self.schedule(); }
        Ok(json!({"scheduled": !ready, "generation": svg.generation}))
    }

    pub fn document_json(&self) -> Value {
        let document = self.document.lock().unwrap();
        json!({ "revision": document.revision(), "source": document.source().text(),
            "formulas": document.formulas, "macros": document.macros,
            "diagnostics": document.diagnostics })
    }

    pub fn state_json(&self) -> Value {
        let state = self.state.lock().unwrap();
        json!({"busy": state.busy, "attempted_revision": state.attempted,
            "compiled_revision": state.snapshot.as_ref().map(|s| s.revision),
            "render_generation": state.snapshot.as_ref().map(|s| s.generation),
            "pages": state.snapshot.as_ref().map_or(0, |s| s.document.pages().len()),
            "compile_runs": state.runs, "compile_ms": state.milliseconds,
            "svg_generation": self.svg.lock().unwrap().generation,
            "diagnostics": state.diagnostics})
    }

    pub fn fragment(&self, node: NodeId, container: Option<NodeId>) -> Result<Value, Error> {
        let mut state = self.state.lock().unwrap();
        let snapshot = state.snapshot.as_mut().ok_or_else(|| Error("文档尚未成功编译".into()))?;
        if snapshot.revision != node.revision { return Err(Error("当前节点的编译结果尚未就绪".into())); }
        let fragments = snapshot.fragments.get_mut(&node).ok_or_else(|| Error(
            "该节点没有独立的输出片段：可能未执行、已被替换，或不在数学显示求值边界".into()))?;
        let fragments: Vec<_> = fragments.iter_mut()
            .filter(|f| container.is_none_or(|id| id == node || f.ancestors.contains(&id)))
            .map(|f| f.json()).collect();
        if fragments.is_empty() { return Err(Error("在这个公式实例中没有找到节点输出".into())); }
        Ok(json!({"revision": snapshot.revision, "render_generation": snapshot.generation, "fragments": fragments}))
    }

    pub fn page(&self, revision: u64, generation: u64, page: usize) -> Result<Value, Error> {
        let mut state = self.state.lock().unwrap();
        let snapshot = state.snapshot.as_mut().ok_or_else(|| Error("文档尚未成功编译".into()))?;
        if snapshot.revision != revision || snapshot.generation != generation { return Err(Error("预览版本已经改变".into())); }
        let document_page = snapshot.document.pages().get(page).ok_or_else(|| Error("页码越界".into()))?;
        let svg = snapshot.pages.entry(page).or_insert_with(|| typst_svg::svg(document_page, &Default::default()));
        Ok(json!({"revision": revision, "render_generation": snapshot.generation, "svg": svg}))
    }

    pub fn refresh_files(&self) {
        self.foundation.files.lock().unwrap().clear();
        self.foundation.epoch.fetch_add(1, Ordering::SeqCst);
        self.schedule();
    }

    fn compile(&self) {
        let epoch = self.foundation.epoch.load(Ordering::SeqCst);
        let (source, revision, targets, origins) = {
            let document = self.document.lock().unwrap();
            let mut targets = HashMap::new();
            let mut origins = HashSet::new();
            for formula in &document.formulas {
                targets.insert(document.node(formula.node).unwrap().span, formula.node);
                let mut raw = vec![];
                formula.projection.origins(&mut raw);
                for node in raw {
                    let span = document.node(node).unwrap().span;
                    targets.insert(span, node);
                    origins.insert(span);
                }
            }
            (document.source().clone(), document.revision(), targets, origins)
        };
        self.state.lock().unwrap().busy = true;
        let started = Instant::now();
        let world = CompileWorld { foundation: self.foundation.clone(), source, origins, time: Time::system() };
        let result = typst::compile::<PagedDocument>(&world);
        let mut diagnostics: Vec<_> = result.warnings.iter().map(|warning| Diagnostic {
            message: warning.message.to_string(), range: None,
        }).collect();
        let output = match result.output {
            Ok(document) => {
                let fragments = fragments::collect(&document, &targets);
                Some(Snapshot { revision, generation: 0, epoch, document, fragments, pages: HashMap::new() })
            }
            Err(errors) => {
                diagnostics.extend(errors.iter().map(|e| Diagnostic { message: e.message.to_string(), range: None }));
                None
            }
        };
        let current = self.document.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        state.busy = false;
        state.runs += 1;
        state.milliseconds = started.elapsed().as_millis();
        // A slow old compilation must never replace a newer preview.
        if current.revision() != revision || epoch != self.foundation.epoch.load(Ordering::SeqCst) { return; }
        state.attempted = Some(revision);
        state.diagnostics = diagnostics;
        if let Some(mut snapshot) = output {
            snapshot.generation = state.runs;
            self.svg.lock().unwrap().publish(&mut snapshot);
            state.snapshot = Some(snapshot);
        } else {
            self.svg.lock().unwrap().fail("当前文档编译未成功，保留上一份 SVG，修复后重试");
        }
        // Bounded-age memoization, while retaining recent document revisions.
        typst::comemo::evict(10);
    }
}
