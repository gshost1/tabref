//! A synchronous Chrome DevTools Protocol client: a loopback WebSocket
//! transport, request/reply and event buffering over flattened target
//! sessions, endpoint discovery, page targets and navigation, and launching
//! a browser with remote debugging. It depends only on `tabref-core`.

mod cdp;
mod discovery;
mod launch;
mod target;
mod transport;

pub use cdp::cdp_event::CdpEvent;
pub use cdp::connection::CdpConnection;
pub use cdp::connection_health::ConnectionGuard;
pub use cdp::dialog_choice::DialogChoice;
pub use cdp::protocol_error::{is_node_gone, is_protocol_error, is_target_gone, target_gone};
pub use discovery::active_port::{ActivePort, parse_active_port, read_active_port};
pub use discovery::browser_endpoint::BrowserEndpoint;
pub use discovery::connect::{connect, connect_held, connect_with};
pub use discovery::default_profiles::{default_user_data_dirs, is_default_user_data_dir};
pub use discovery::endpoint_source::EndpointSource;
pub use discovery::endpoint_spec::EndpointSpec;
pub use discovery::held_port::HeldPort;
pub use discovery::json_version::{endpoint_from_held, endpoint_from_port};
pub use discovery::listener_owner::{check_listener, is_chrome_family};
pub use discovery::listener_verdict::ListenerVerdict;
pub use discovery::lsof_owner::listener_pids;
pub use discovery::process_view::ProcessView;
pub use discovery::profile_lock::profile_lock_pid;
pub use discovery::user_browser::UserBrowser;
pub use discovery::user_data_dir::endpoint_from_user_data_dir;
pub use discovery::websocket_url::WebSocketUrl;
pub use launch::abandon::abandon_launch;
pub use launch::chrome_binary::{CHROME_ENV, find_chrome};
pub use launch::chrome_launch::launch_chrome;
pub use launch::launch_options::LaunchOptions;
pub use launch::launched_browser::LaunchedBrowser;
pub use launch::process_start::{launched_alive, process_start};
pub use launch::profile_use::ProfileUse;
pub use launch::recorded_process::is_launched_browser;
pub use target::frame_entry::FrameEntry;
pub use target::frame_tree::{current_loader, list_frames, parse_frame_tree};
pub use target::navigate::{navigate, navigate_by, wait_for_load};
pub use target::page_session::PageSession;
pub use target::page_target::PageTarget;
pub use target::targets::{
    activate_target, attach_to_target, attach_to_target_with, close_target, create_background_tab,
    list_page_targets,
};
pub use transport::cdp_transport::CdpTransport;
#[cfg(any(test, feature = "scripted"))]
pub use transport::scripted_transport::ScriptedTransport;
pub use transport::transport_error::{
    is_timeout, transport_closed, transport_failed, transport_timeout,
};
pub use transport::websocket::WebSocketTransport;
