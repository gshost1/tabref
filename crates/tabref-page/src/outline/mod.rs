//! The page read from its DOM into blocks in reading order: headings,
//! paragraphs, lists, tables, landmarks, dialogs and frames, with every
//! control named and referenced as the accessibility snapshot has it. The
//! accessibility tree gives refs; the DOM gives what the tree drops: block
//! boundaries, link addresses, select options, classes that mark repeated
//! siblings, and which content the page hides.

pub(crate) mod block;
pub(crate) mod block_kind;
pub(crate) mod build;
pub(crate) mod capture_strings;
pub(crate) mod container;
pub(crate) mod control;
pub(crate) mod control_build;
pub(crate) mod dom_capture;
pub(crate) mod dom_node;
pub(crate) mod dom_parse;
pub(crate) mod dom_tree;
pub(crate) mod element_kind;
pub(crate) mod expander;
pub(crate) mod form_state;
pub(crate) mod frame_stats;
pub(crate) mod frame_walk;
pub(crate) mod item;
pub(crate) mod node_kind;
pub(crate) mod node_layout;
pub(crate) mod outline_builder;
pub(crate) mod outline_input;
pub(crate) mod overlay;
pub(crate) mod page_outline;
pub(crate) mod rare_data;
pub(crate) mod ref_label;
pub(crate) mod repeat_key;
pub(crate) mod select_option;
pub(crate) mod shown_map;
pub(crate) mod text_match;
pub(crate) mod text_runs;
pub(crate) mod text_search;
pub(crate) mod url_resolve;
pub(crate) mod walk_context;
