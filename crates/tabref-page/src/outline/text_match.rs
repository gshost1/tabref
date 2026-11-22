use std::collections::HashSet;
use std::rc::Rc;

use super::control::Control;
use super::dom_tree::DomTree;
use super::element_kind::search_control;
use super::outline_input::OutlineInput;
use super::search_blocks::{find_folded, fold};
use super::text_runs::block_text;

/// One place a text search found its text: the headings the place sits
/// under (outermost first), the block's text around the match, the control
/// the match is in or nearest to, whether the page hides it, and the
/// control that shows it when it is hidden behind one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    pub hits: usize,
    pub section: Vec<String>,
    pub line: String,
    pub control: Option<Control>,
    pub hidden: bool,
    pub expander: Option<Control>,
}

#[derive(Default)]
pub(super) struct CappedRefs {
    seen: HashSet<String>,
    refs: Vec<String>,
    omitted: usize,
}

impl CappedRefs {
    pub(super) fn record(&mut self, reference: Option<&str>, printed: bool) {
        let Some(reference) = reference else { return };
        if self.seen.contains(reference) {
            return;
        }
        self.seen.insert(reference.to_owned());
        if !printed {
            if self.refs.len() < 8 {
                self.refs.push(reference.to_owned());
            } else {
                self.omitted += 1;
            }
        }
    }

    pub(super) fn row(mut self, hits: usize, section: &[String]) -> TextMatch {
        let mut line = format!("… {hits} more in this block");
        if !self.refs.is_empty() {
            if self.omitted > 0 {
                self.refs.push(format!("+{} refs", self.omitted));
            }
            line.push_str(&format!(" ({})", self.refs.join(" ")));
        }
        TextMatch {
            hits,
            section: section.to_vec(),
            line,
            control: None,
            hidden: false,
            expander: None,
        }
    }
}

#[derive(Clone)]
pub(super) struct PreControl {
    pub control: Rc<Control>,
    pub multiline: bool,
    pub text_matches: bool,
}

impl PreControl {
    pub(super) fn at(
        input: &OutlineInput,
        tree: &DomTree,
        index: usize,
        skipped: &HashSet<usize>,
        needle: &[char],
    ) -> Option<Self> {
        let control = Rc::new(search_control(input, tree, index)?);
        let multiline = super::element_kind::pre_has_break(tree, index, skipped);
        let text = block_text(tree, index, &|node| skipped.contains(&node));
        Some(Self {
            control,
            multiline,
            text_matches: find_folded(&fold(&text), needle).is_some(),
        })
    }
}

pub(super) fn line_control(
    tree: &DomTree,
    pre: usize,
    nodes: impl Iterator<Item = usize>,
    control_at: &mut dyn FnMut(usize) -> Option<PreControl>,
) -> Option<(usize, PreControl)> {
    let mut first = None;
    let mut wrapper = None;
    let candidates = nodes.flat_map(|index| {
        std::iter::successors(tree.nodes[index].parent, |at| {
            (*at != pre).then(|| tree.nodes[*at].parent).flatten()
        })
    });
    for index in candidates {
        let Some(control) = control_at(index) else {
            continue;
        };
        if index == pre || control.multiline {
            wrapper.get_or_insert((index, control));
        } else if control.text_matches {
            return Some((index, control));
        } else {
            first.get_or_insert((index, control));
        }
    }
    first.or(wrapper)
}
