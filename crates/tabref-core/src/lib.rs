//! Shared types for tabref: the error and envelope contract, deadlines, the
//! accessibility node a page snapshot is read into, and snapshot-qualified
//! refs with the store that keeps them between invocations. Nothing here
//! talks to a browser or to the operating system.

mod error;
mod refs;
mod response;
mod storage;
mod tree;
mod wait;

pub use error::app_error::AppError;
pub use error::delivery_disposition::DeliveryDisposition;
pub use error::delivery_semantics::DeliverySemantics;
pub use error::error_code::ErrorCode;
pub use error::retry_disposition::RetryDisposition;
pub use refs::ref_able::{is_ref_able, is_ref_able_role_actions};
pub use refs::ref_alloc::allocate_refs;
pub use refs::ref_entry::RefEntry;
pub use refs::ref_id::RefId;
pub use refs::ref_map::RefMap;
pub use refs::ref_store::RefStore;
pub use refs::snapshot_id::{new_snapshot_id, validate_snapshot_id};
pub use response::envelope::{ENVELOPE_VERSION, Response};
pub use response::error_payload::ErrorPayload;
pub use response::recovery_hint::RecoveryHint;
pub use storage::private_file::{
    create_new_private_dir, create_private_dir, read_bounded, write_atomic,
};
pub use storage::state_root::{STATE_ROOT_ENV, resolve_state_root};
pub use tree::capability;
pub use tree::name_source::NameSource;
pub use tree::node::AccessibilityNode;
pub use tree::node_identity::NodeIdentity;
pub use tree::node_presentation::NodePresentation;
pub use tree::point::Point;
pub use tree::roles::{CANONICAL_ROLES, INTERACTIVE_ROLES, is_canonical_role, is_interactive_role};
pub use tree::state;
pub use wait::deadline::Deadline;
