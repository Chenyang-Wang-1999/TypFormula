//! Export fragments from finished frames. No evaluation or layout in this file.
//! Preserve ancestor group transforms, clips and hard-frame coordinate systems;
//! a plain page screenshot crop would include unrelated overlapping content.
use std::collections::HashMap;
use serde_json::{Value, json};
use typst::{foundations::{Content, Smart}, introspection::{Location, MetadataElem, Tag},
    layout::{Abs, Frame, FrameItem, GroupItem, Point, Rect, Sides, Size, Transform}, math::EquationElem, utils::Numeric};
use typst_layout::{Page, PagedDocument};
use typst_syntax::Span;
use crate::NodeId;

pub struct Fragment {
    pub ancestors: Vec<NodeId>,
    pub(super) page: usize,
    pub(super) bounds: Rect,
    instance: usize,
    frame: Frame,
    svg: Option<String>,
    em: Abs,
}
impl Fragment {
    pub fn json(&mut self) -> Value {
        let svg = self.svg.get_or_insert_with(|| typst_svg::svg(&Page {
            frame: self.frame.clone(), bleed: Sides::splat(Abs::zero()), fill: Smart::Custom(None),
            numbering: None, supplement: Content::empty(), number: 1,
        }, &Default::default()));
        json!({"svg": svg, "width": self.frame.width().to_pt(), "height": self.frame.height().to_pt(),
            "baseline": self.frame.baseline().to_pt(), "em": self.em.to_pt(), "page": self.page, "instance": self.instance})
    }
}

struct Capture {
    origin: NodeId,
    location: Location,
    ancestors: Vec<NodeId>,
    page: usize,
    baseline: Abs,
    frame: Frame,
    bounds: Option<Rect>,
    complete: bool,
    em: Abs,
}

pub fn collect(document: &PagedDocument, targets: &HashMap<Span, NodeId>) -> HashMap<NodeId, Vec<Fragment>> {
    let mut captures = vec![];
    // Capture a target wherever it is actually placed. Repeated source nodes
    // get separate instance entries rather than silently choosing the first.
    for (page, output) in document.pages().iter().enumerate() {
        let mut active = vec![];
        walk(&output.frame, Transform::identity(), &mut vec![], targets,
            &mut captures, &mut active, page, &output.frame);
        // Cross-page/unbalanced boundaries are deliberately unavailable. The
        // full-page preview remains correct; don't return a misleading crop.
    }
    let mut result: HashMap<NodeId, Vec<Fragment>> = HashMap::new();
    for capture in captures {
        if !capture.complete { continue; }
        let Some(bounds) = capture.bounds else { continue; };
        let padding = Abs::pt(1.0);
        let min = bounds.min - Point::splat(padding);
        let max = bounds.max + Point::splat(padding);
        let size = Size::new(max.x - min.x, max.y - min.y);
        let mut frame = Frame::soft(size);
        frame.set_baseline((capture.baseline - min.y).max(Abs::zero()).min(size.y));
        frame.push_frame(-min, capture.frame);
        let entries = result.entry(capture.origin).or_default();
        entries.push(Fragment { ancestors: capture.ancestors, page: capture.page, bounds, instance: entries.len(), frame, svg: None, em: capture.em });
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn walk(frame: &Frame, transform: Transform, groups: &mut Vec<(Point, GroupItem)>,
        targets: &HashMap<Span, NodeId>, captures: &mut Vec<Capture>, active: &mut Vec<usize>,
        page: usize, page_frame: &Frame) {
    for (position, item) in frame.items() {
        let local = transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Tag(Tag::Start(content, flags)) => {
                let candidate = content.is::<EquationElem>() ||
                    (content.is::<MetadataElem>() && !flags.introspectable && !flags.tagged);
                if candidate {
                    if let Some(&origin) = targets.get(&content.span()) {
                        let index = captures.len();
                        captures.push(Capture { origin, location: content.location().unwrap(),
                            ancestors: active.iter().map(|&i| captures[i].origin).collect(), page,
                            baseline: Point::zero().transform(local).y,
                            frame: Frame::new(page_frame.size(), page_frame.kind()), bounds: None, complete: false, em: Abs::zero() });
                        active.push(index);
                    }
                }
            }
            FrameItem::Tag(Tag::End(location, ..)) => {
                if let Some(index) = active.iter().rposition(|&i| captures[i].location == *location) {
                    captures[active[index]].complete = true;
                    active.remove(index);
                }
            }
            FrameItem::Group(group) => {
                groups.push((*position, group.clone()));
                walk(&group.frame, local.pre_concat(group.transform), groups, targets,
                    captures, active, page, page_frame);
                groups.pop();
            }
            _ if !active.is_empty() => {
                let bounds = match item {
                    FrameItem::Text(text) => Some(text.bbox()),
                    FrameItem::Shape(shape, _) => Some(shape.bbox(true)),
                    FrameItem::Image(_, size, _) | FrameItem::Link(_, size) => Some(Rect::from_pos_size(Point::zero(), *size)),
                    _ => None,
                };
                let Some(bounds) = bounds.map(|rect| transformed(rect, local)) else { continue; };
                if !bounds.min.x.is_finite() || !bounds.min.y.is_finite() || !bounds.max.x.is_finite() || !bounds.max.y.is_finite() { continue; }
                for &i in active.iter() {
                    let capture = &mut captures[i];
                    if let FrameItem::Text(text) = item { capture.em = capture.em.max(text.size); }
                    capture.bounds = Some(match capture.bounds {
                        None => bounds,
                        Some(old) => Rect::new(old.min.min(bounds.min), old.max.max(bounds.max)),
                    });
                    // Retain original ancestor frame sizes so gradients and
                    // clipping are not reinterpreted relative to the crop.
                    let mut branch = Frame::new(frame.size(), frame.kind());
                    branch.push(*position, item.clone());
                    for (depth, (offset, group)) in groups.iter().enumerate().rev() {
                        let mut group = group.clone();
                        group.frame = branch;
                        let parent = if depth == 0 { page_frame } else { &groups[depth - 1].1.frame };
                        branch = Frame::new(parent.size(), parent.kind());
                        branch.push(*offset, FrameItem::Group(group));
                    }
                    capture.frame.push_frame(Point::zero(), branch);
                }
            }
            _ => {}
        }
    }
}

fn transformed(rect: Rect, transform: Transform) -> Rect {
    let corners = [rect.min, rect.max, Point::new(rect.min.x, rect.max.y), Point::new(rect.max.x, rect.min.y)];
    let mut min = Point::splat(Abs::inf());
    let mut max = Point::splat(-Abs::inf());
    for corner in corners { let p = corner.transform(transform); min = min.min(p); max = max.max(p); }
    Rect::new(min, max)
}
