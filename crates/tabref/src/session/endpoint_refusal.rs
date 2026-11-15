//! What tabref says when it will not attach to a port, or attaches to one it
//! could not vouch for. The way out it offers first is always a browser of
//! tabref's own (`open --launch`); naming the port with `--cdp` is offered
//! only when the holder is a program of this user the message names, since a
//! holder tabref cannot see belongs to another user or to root.

use std::net::{Ipv4Addr, SocketAddr};

use tabref_cdp::ListenerVerdict;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

const LAUNCH: &str = "Run 'tabref open <url> --launch' for a browser of tabref's own";

/// The refusal of port `port`, found without being named, for `verdict`
/// (`Untrusted` or `CantCheck`).
pub(crate) fn unverified_port(port: u16, verdict: &ListenerVerdict) -> AppError {
    let (message, suggestion, holder) = match verdict {
        ListenerVerdict::Untrusted {
            holder: Some(holder),
        } => (
            format!(
                "Port {port} is held by {holder}, not by a Chrome of this user; tabref does not attach to it"
            ),
            format!(
                "{LAUNCH}. Only if {holder} is a browser you started yourself, name it with --cdp {port}"
            ),
            Some(holder.as_str()),
        ),
        ListenerVerdict::CantCheck(reason) => (
            format!(
                "tabref could not check who holds port {port} ({reason}), so it does not attach to it"
            ),
            LAUNCH.to_owned(),
            None,
        ),
        _ => (
            format!(
                "Port {port} is held by a process of another user or of root, which tabref cannot see; \
                 tabref does not attach to it"
            ),
            LAUNCH.to_owned(),
            None,
        ),
    };
    AppError::new(ErrorCode::PermDenied, message)
        .with_suggestion(suggestion)
        .with_details(serde_json::json!({
            "kind": "cdp_endpoint_unverified",
            "port": port,
            "holder": holder,
            "checked": !matches!(verdict, ListenerVerdict::CantCheck(_)),
        }))
        .with_disposition(DeliverySemantics::not_delivered())
}

/// The warning for an endpoint the user named, at `address`, given the
/// owner check of its port's IPv4-reachable listeners. An address other than
/// `127.0.0.1:<port>` (an IPv6 endpoint, or `localhost` that fell back to
/// one) is a socket the check never looked at, so it always warns, whatever
/// the check said; `None` only when a Chrome of this user holds the checked
/// address, or when the platform cannot tell.
pub(crate) fn named_warning(address: SocketAddr, verdict: &ListenerVerdict) -> Option<String> {
    if matches!(verdict, ListenerVerdict::Unknown) {
        return None;
    }
    let port = address.port();
    if address != SocketAddr::from((Ipv4Addr::LOCALHOST, port)) {
        return Some(format!(
            "{address} was not checked (tabref checks who holds 127.0.0.1:{port} only); \
             tabref attached only because --cdp names it"
        ));
    }
    named_port_warning(port, verdict)
}

/// The warning for an endpoint the user named whose port tabref could not
/// vouch for; `None` when it could, or when the platform cannot tell.
pub(crate) fn named_port_warning(port: u16, verdict: &ListenerVerdict) -> Option<String> {
    let whose = match verdict {
        ListenerVerdict::Trusted(_) | ListenerVerdict::Nothing | ListenerVerdict::Unknown => {
            return None;
        }
        ListenerVerdict::Untrusted {
            holder: Some(holder),
        } => format!("is held by {holder}, not by a Chrome of this user"),
        ListenerVerdict::Untrusted { holder: None } => {
            "is held by a process of another user or of root".to_owned()
        }
        ListenerVerdict::CantCheck(reason) => format!("could not be checked ({reason})"),
    };
    Some(format!(
        "port {port} {whose}; tabref attached only because --cdp names it"
    ))
}

#[cfg(test)]
#[path = "endpoint_refusal_tests.rs"]
mod tests;
