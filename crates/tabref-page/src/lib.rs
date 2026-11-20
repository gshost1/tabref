//! Reads a page into an accessibility tree and acts on its elements over
//! CDP. The snapshot reader grafts out-of-process iframes beneath their owner
//! elements, promotes click-listening elements to controls, and merges inline
//! text into readable runs. Actions address a DOM node by its element token,
//! prove the node's document is still the one the snapshot read, refuse a
//! press another element would intercept, and read back what they changed.

mod act;
mod locate;
mod outline;
mod page;
mod settle;
mod tree;

pub(crate) use tabref_cdp::{CdpConnection, CdpEvent, PageSession};

pub use act::action_need::ActionNeed;
pub use act::actionable::wait_actionable;
pub use act::append_outcome::AppendOutcome;
pub use act::append_text::append_text;
pub use act::assigned_input::AssignedInput;
pub use act::blocker::Blocker;
pub use act::captured_image::CapturedImage;
pub use act::change_watch::{mark_page, page_changes};
pub use act::checked::set_checked;
pub use act::click::{click, click_times};
pub use act::element_reading::ElementReading;
pub use act::file_input_choice::FileInputVia;
pub use act::fill::fill;
pub use act::fill_outcome::FillOutcome;
pub use act::focus::{focus, focus_target, page_focused};
pub use act::focus_target::FocusTarget;
pub use act::focus_watch::FocusWatch;
pub use act::focus_watch::{focus_kept, release_focus_watch, watch_focus};
pub use act::hover::hover;
pub use act::key_press::{press_key, supported_key_names};
pub use act::page_changes::PageChanges;
pub use act::page_mark::PageMark;
pub use act::read::read_element;
pub use act::screenshot::{capture_screenshot, png_size};
pub use act::scroll::{box_center, scroll_into_view};
pub use act::select_option::select_option;
pub use act::select_outcome::SelectOutcome;
pub use act::text_change::TextChange;
pub use act::unactionable::Unactionable;
pub use act::upload::{upload_files, upload_input_files};
pub use act::upload_outcome::UploadOutcome;
pub use locate::locate_intent::LocateIntent;
pub use locate::locate_node::locate;
pub use locate::located::Located;
pub use locate::locator::Locator;
pub use locate::scope_kind::ScopeKind;
pub use locate::word::Word;
pub use outline::block::Block;
pub use outline::block_kind::BlockKind;
pub use outline::build::{build_outline, build_outline_at};
pub use outline::control::Control;
pub use outline::dom_capture::{capture_dom, frame_owner_in, read_viewport};
pub use outline::dom_node::DomNode;
pub use outline::dom_parse::parse_capture;
pub use outline::dom_tree::DomTree;
pub use outline::form_state::FormState;
pub use outline::frame_stats::FrameStats;
pub use outline::item::Item;
pub use outline::node_kind::NodeKind;
pub use outline::node_layout::NodeLayout;
pub use outline::outline_input::OutlineInput;
pub use outline::page_outline::PageOutline;
pub use outline::ref_label::RefLabel;
pub use outline::select_option::SelectOption;
pub use outline::text_match::TextMatch;
pub use outline::text_runs::{collapse_run, cut_chars, squash, subtree_text};
pub use outline::text_search::search_text;
pub use outline::url_resolve::{origin_of, resolve_href, short_href};
pub use page::capture_watch::{
    CaptureWatch, IDLE_WAIT_MS, arm_capture_watch, check_capture_watch, has_closed_shadow_root,
};
pub use page::evaluate::evaluate;
pub use page::page_info::PageInfo;
pub use page::page_text::page_text;
pub use page::scroll_site::ScrollSite;
pub use settle::action_settle::{follow_late_navigation, settle_action, watch_navigation};
pub use settle::dialog_json::{DIALOG_OPENING, dialog_json, drain_dialogs, mark_other_tab};
pub use settle::download_events::{
    cancel_download, deny_downloads, enable_downloads, next_download_event, note_downloads, now_ms,
    restore_downloads,
};
pub use settle::download_origin::DownloadOrigin;
pub use settle::download_record::DownloadRecord;
pub use settle::download_state::DownloadState;
pub use settle::download_timing::DownloadTiming;
pub use settle::element_state::ElementState;
pub use settle::element_wait::wait_for_state;
pub use settle::page_wait::{wait_for_text, wait_for_url};
pub use settle::scroll_outcome::ScrollOutcome;
pub use settle::scroll_settle::scroll_and_settle;
pub use settle::settle_outcome::SettleOutcome;
pub use settle::state_wait::StateWait;
pub use settle::watched_action::WatchedAction;
pub use tree::element_token::ElementToken;
pub use tree::frame_skip_reason::FrameSkipReason;
pub use tree::node_origin::NodeOrigin;
pub use tree::node_read::{read_node, read_node_identity};
pub use tree::skipped_frame::SkippedFrame;
pub use tree::snapshot::{snapshot_page, snapshot_page_within_budget};
pub use tree::web_roles::map_cdp_role;
pub use tree::web_snapshot::WebSnapshot;
