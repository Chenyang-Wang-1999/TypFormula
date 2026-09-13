// SPDX-License-Identifier: GPL-2.0-or-later
//! The part of a document that a piece of source still depends on.
//!
//! A fragment is compiled where it stands, so the context used to be everything before it.
//! That is slow -- a keystroke anywhere invalidates the compile, and a long document costs
//! about a second per picture -- and it is also more than the fragment can see. What it can
//! see is smaller, and Typst's own rules say exactly how small:
//!
//! * A `#set` or `#show` rule is attached to **the rest of the block it is written in**
//!   (`typst-eval`: the rule wraps the tail of that block and travels with it), so a rule in
//!   a closed sibling block, or after the target, cannot reach it, while a rule in an
//!   enclosing block before it does.
//! * Templates leak the other way: a rule inside a function body reaches the target when the
//!   target is placed in that body's tail. The body travels with its definition, so a
//!   `#let` is kept whole -- that is why `is_stmt` includes it.
//! * Bindings do not escape a closed block either (`typst::scope_bindings` relies on the
//!   same rule for macros), and `#import` brings names the syntax cannot see, so a statement
//!   before the target is kept whether or not its name is used.
//!
//! What that leaves is: the statements before the target at every level of its ancestor
//! path, the target's own node, and enough of the enclosing structure for the result to
//! parse. Prose, the formulas nobody asked about, and closed blocks are dropped.
use typst_syntax::{LinkedNode, Source, SyntaxKind};

/// A reduced document, and where the ranges that were asked about went.
pub struct Context {
    /// The kept pieces, in document order.
    pub source: String,
    /// The ranges that were asked about, in `source` coordinates.
    pub ranges: Vec<(usize, usize)>,
    /// `(original start, length, new start)` for each kept run, for `original`.
    pieces: Vec<(usize, usize, usize)>,
}

impl Context {
    /// The kept text before `at`, for a caller whose source continues at that offset.
    pub fn prefix(&self, at: usize) -> &str {
        let text = self.source.get(..at).unwrap_or(&self.source);
        text.strip_suffix('\n').unwrap_or(text)
    }
    /// Where an offset of the reduced document came from.
    ///
    /// The macro registry is built from reduced text -- the same text after an unrelated edit,
    /// which is what lets it be cached -- but everything it reports has to be an offset in the
    /// document the caller handed in, because that is the text its definitions and fragments
    /// are located in.
    pub fn original(&self, at: usize) -> Option<usize> {
        self.pieces.iter().find(|&&(_, length, new)| new <= at && at <= new + length).map(|&(start, _, new)| start + (at - new))
    }
    /// A short, stable identity for this context.
    ///
    /// Two formulas whose context is the same text get the same digest, and an edit outside
    /// that text does not change it -- which is what lets an image or a placement be kept
    /// across an unrelated keystroke. The hash is over the text only, so two processes agree.
    pub fn digest(&self) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in self.source.as_bytes() { hash ^= u64::from(*byte); hash = hash.wrapping_mul(0x100_0000_01b3); }
        format!("{hash:016x}")
    }
}

/// Every part of `source` that can affect `wanted`, with `wanted` themselves.
///
/// `None` when there is nothing to reduce (no ranges, or a range that no node holds).
pub fn context(source: &str, wanted: &[(usize, usize)]) -> Option<Context> {
    context_in(&Source::detached(source.to_owned()), wanted)
}

/// The same reduction against a tree the caller already has.
///
/// A document that is already parsed -- the kernel's `Document` keeps its `Source` up to date
/// incrementally -- must not be parsed again for every formula that asks about its context,
/// which on a long document is the difference between one parse and one per formula.
pub fn context_in(parsed: &Source, wanted: &[(usize, usize)]) -> Option<Context> {
    let source = parsed.text();
    if wanted.is_empty() || wanted.iter().any(|&(start, end)| start >= end || end > source.len() || !source.is_char_boundary(start) || !source.is_char_boundary(end)) {
        return None;
    }
    let mut builder = Builder::default();
    keep(&LinkedNode::new(parsed.root()), source, wanted, &mut builder);
    if builder.text.is_empty() { return None; }
    let ranges = wanted.iter().map(|&(start, end)| Some((builder.map(start)?, builder.map(end)?))).collect::<Option<Vec<_>>>()?;
    Some(Context { source: builder.text, ranges, pieces: builder.pieces })
}

/// Everything in `source` that can affect a point at `at` -- where a formula begins.
pub fn context_before(source: &str, at: usize) -> Option<Context> {
    if at == 0 || at > source.len() || !source.is_char_boundary(at) { return None; }
    context(source, &[(at - 1, at)])
}

/// Kept pieces of the original, so an offset can be moved into the reduced document.
#[derive(Default)]
struct Builder {
    text: String,
    /// `(original start, length, new start)` for each run of kept text, in order.
    pieces: Vec<(usize, usize, usize)>,
}

impl Builder {
    /// Keep one run of the original verbatim. Runs that were adjacent stay adjacent, and the
    /// separator between two runs is a single newline rather than the whitespace of the
    /// original, which was written around text that is gone.
    fn keep(&mut self, source: &str, start: usize, end: usize) {
        if start >= end { return; }
        let (start, end) = (start.min(source.len()), end.min(source.len()));
        let mut text = &source[start..end];
        if let Some(last) = self.pieces.last_mut()
            && last.0 + last.1 == start && last.2 + last.1 == self.text.len() {
            last.1 += end - start;
            self.text.push_str(text);
            return;
        }
        if !self.text.is_empty() {
            if !self.text.ends_with('\n') { self.text.push('\n'); }
            text = text.trim_start_matches(['\n', '\r']);
        }
        if text.is_empty() { return; }
        let at = self.text.len();
        let start = end - text.len();
        self.text.push_str(text);
        self.pieces.push((start, text.len(), at));
    }
    /// Where an offset of the original lives in the reduced document.
    fn map(&self, at: usize) -> Option<usize> {
        self.pieces.iter().find(|&&(start, length, _)| start <= at && at <= start + length).map(|&(start, _, new)| new + (at - start))
    }
}

/// Keep what can affect the wanted ranges inside `node`.
fn keep(node: &LinkedNode, source: &str, wanted: &[(usize, usize)], out: &mut Builder) {
    let (start, end) = (node.offset(), node.offset() + node.len());
    if !wanted.iter().any(|&(a, b)| start <= a && b <= end) { return; }
    let children: Vec<_> = node.children().collect();
    // The children that hold a wanted range. A node holding one but having none below it is a
    // leaf of the reduction -- a formula, a call, a word -- and is kept as it stands.
    let holds = |child: &LinkedNode| {
        let (a, b) = (child.offset(), child.offset() + child.len());
        wanted.iter().any(|&(want_a, want_b)| a <= want_a && want_b <= b)
    };
    let carriers: Vec<bool> = children.iter().map(holds).collect();
    if !carriers.iter().any(|carrier| *carrier) {
        out.keep(source, start, end);
        return;
    }
    // Anywhere but markup -- an argument list, a block, an equation, an array -- a piece left
    // out changes what the rest means (`$ ... $` loses its dollar, a call loses an argument),
    // so the node is copied as it stands with only the wanted children reduced.
    if node.kind() != SyntaxKind::Markup {
        for (index, child) in children.iter().enumerate() {
            let (a, b) = (child.offset(), child.offset() + child.len());
            if carriers[index] { keep(child, source, wanted, out); } else { out.keep(source, a, b); }
        }
        return;
    }
    // Markup is a sequence of independent statements: a rule reaches what follows it in the
    // same block, so only the statements before the target can affect it, and a closed block's
    // rules stay inside that block. The `#` introducing a kept expression comes along, and the
    // pieces are joined by a newline -- which is also what terminates a `#let`.
    let last = carriers.iter().rposition(|carrier| *carrier).unwrap_or(0);
    let mut wanted_here = carriers.clone();
    for index in 0..children.len() {
        if index < last && (statement(&children[index]) || open_block(&children[index])) { wanted_here[index] = true; }
    }
    // A `#` is kept with the expression it introduces, which is a second pass because that
    // expression may have been marked by the pass above.
    for index in 0..children.len() {
        if index + 1 < children.len() && wanted_here[index + 1] && children[index].kind() == SyntaxKind::Hash { wanted_here[index] = true; }
    }
    for (index, child) in children.iter().enumerate() {
        if !wanted_here[index] { continue; }
        let (a, b) = (child.offset(), child.offset() + child.len());
        if carriers[index] { keep(child, source, wanted, out); } else { out.keep(source, a, b); }
    }
}

/// Whether a node is a statement whose effect outlives it: a binding, a rule, an import.
fn statement(node: &LinkedNode) -> bool {
    node.kind().is_stmt() || (node.kind() == SyntaxKind::Hash && node.next_sibling().is_some_and(|next| next.kind().is_stmt()))
}

/// A block the parser never closed. Its bindings and its rules are still in scope at the
/// target -- `typst::scope_bindings` skips only blocks that end with their closing delimiter --
/// so it is context even though it looks like a sibling that finished.
fn open_block(node: &LinkedNode) -> bool {
    matches!(node.kind(), SyntaxKind::ContentBlock | SyntaxKind::CodeBlock)
        && !node.children().last().is_some_and(|last| matches!(last.kind(), SyntaxKind::RightBracket | SyntaxKind::RightBrace))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reduce(source: &str, text: &str) -> (String, Vec<(usize, usize)>) {
        let start = source.find(text).expect("the text is in the document");
        let context = context(source, &[(start, start + text.len())]).expect("a context");
        let moved = context.ranges[0];
        assert_eq!(&context.source[moved.0..moved.1], text, "the range still points at the same text: {:?}", context.source);
        (context.source, context.ranges)
    }

    #[test]
    fn prose_and_unasked_formulas_are_dropped() {
        let source = "#set text(size: 11pt)\n\n前情提要，一段很长的正文。\n\n#let twice(x) = $#x + #x$\n\n中间还有别的正文 $a+b$ 之类。\n\n$ cancel(y) $\n\n末尾正文，以及 #panic(\"坏了\") 。\n";
        let (reduced, _) = reduce(source, "cancel(y)");
        assert_eq!(reduced, "#set text(size: 11pt)\n#let twice(x) = $#x + #x$\n$ cancel(y) $");
    }

    #[test]
    fn a_rule_inside_a_closed_block_is_not_context() {
        // `#[#show ...]` attaches to the tail of its own block, which is empty: it cannot
        // reach anything outside, so it is not part of the context.
        let source = "前文 #[#show heading: it => [X]\n#set text(red)] 后文\n\n$ a_(1) $\n";
        let (reduced, _) = reduce(source, "a_(1)");
        assert_eq!(reduced, "$ a_(1) $");
    }

    #[test]
    fn a_rule_after_the_target_is_not_context() {
        let source = "$ a_(1) $\n\n#set text(red)\n\n$ b_(2) $\n";
        let (reduced, _) = reduce(source, "a_(1)");
        assert_eq!(reduced, "$ a_(1) $");
    }

    #[test]
    fn a_rule_before_the_target_in_an_enclosing_block_is_context() {
        let source = "#block[\n  #set math.limits(inline: true)\n  正文\n  $ sum_(j) $\n]\n";
        let (reduced, _) = reduce(source, "sum_(j)");
        assert!(reduced.starts_with("#block["), "{reduced:?}");
        assert!(reduced.contains("#set math.limits(inline: true)"), "{reduced:?}");
        assert!(reduced.ends_with(']'), "{reduced:?}");
        assert!(!reduced.contains("正文"), "{reduced:?}");
    }

    #[test]
    fn a_definition_brings_the_rules_its_body_carries() {
        // The rule lives in the macro body, which is kept whole: when the target is placed in
        // that body's tail -- at the call site -- the rule reaches it.
        let source = "#let styled(body) = [\n  #show heading: it => [X]\n  #body\n]\n\n前文\n\n$ f(1) $\n";
        let (reduced, _) = reduce(source, "f(1)");
        assert!(reduced.contains("#show heading"), "{reduced:?}");
        assert!(reduced.contains("$ f(1) $"), "{reduced:?}");
    }

    #[test]
    fn structure_around_the_target_is_kept_so_that_it_parses() {
        let source = "#columns(2)[\n  正文\n  #set text(size: 8pt)\n  $ a_(1) $\n  尾部正文\n]\n";
        let (reduced, _) = reduce(source, "a_(1)");
        assert!(reduced.starts_with("#columns(2)["), "{reduced:?}");
        assert!(reduced.contains("#set text(size: 8pt)"), "{reduced:?}");
        assert!(reduced.ends_with(']'), "{reduced:?}");
        assert!(!reduced.contains("尾部正文"), "what follows the target is cut: {reduced:?}");
    }

    #[test]
    fn two_ranges_come_back_in_one_context() {
        let source = "#set text(size: 11pt)\n\n正文\n\n$ a_(1) $\n\n正文\n\n$ b_(2) $\n";
        let first = source.find("a_(1)").unwrap();
        let second = source.find("b_(2)").unwrap();
        let context = context(source, &[(first, first + 5), (second, second + 5)]).unwrap();
        for (index, text) in ["a_(1)", "b_(2)"].iter().enumerate() {
            let (start, end) = context.ranges[index];
            assert_eq!(&context.source[start..end], *text);
        }
        assert!(context.source.starts_with("#set text(size: 11pt)"), "{:?}", context.source);
    }

    #[test]
    fn the_prefix_before_a_range_is_what_a_writer_needs() {
        // The kernel writes `definitions + formula`, so what it needs is the kept text before
        // the formula the range points at.
        let source = "#let twice(x) = $#x + #x$\n\n正文\n\n$ twice(a) $\n";
        let formula = "$ twice(a) $";
        let start = source.find(formula).unwrap();
        let context = context(source, &[(start, start + formula.len())]).unwrap();
        assert_eq!(context.prefix(context.ranges[0].0), "#let twice(x) = $#x + #x$");
        assert!(context.source.ends_with(formula), "{:?}", context.source);
    }

    #[test]
    fn the_digest_ignores_edits_outside_the_context() {
        let source = "#let twice(x) = $#x + #x$\n\n正文\n\n$ twice(a) $\n";
        let formula = "$ twice(a) $";
        let start = source.find(formula).unwrap();
        let digest = context(source, &[(start, start + formula.len())]).unwrap().digest();
        // A prose edit before the formula used to change the whole prefix, and with it every
        // image and placement keyed by it.
        let edited = source.replace("正文", "正文改过了，长了很多");
        let moved = edited.find(formula).unwrap();
        assert_eq!(context(&edited, &[(moved, moved + formula.len())]).unwrap().digest(), digest);
        // The context itself changing does change it.
        let with_rule = source.replace("#let twice", "#set text(size: 9pt)\n#let twice");
        let moved = with_rule.find(formula).unwrap();
        assert_ne!(context(&with_rule, &[(moved, moved + formula.len())]).unwrap().digest(), digest);
    }

    #[test]
    fn nothing_wanted_means_no_context() {
        assert!(context("$ a $", &[]).is_none());
        assert!(context("$ a $", &[(2, 1)]).is_none());
        assert!(context("$ a $", &[(0, 99)]).is_none());
    }
}
