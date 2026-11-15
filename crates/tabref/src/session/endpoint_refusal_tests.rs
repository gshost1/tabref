use std::net::SocketAddr;

use tabref_cdp::ListenerVerdict;

use super::{named_port_warning, named_warning, unverified_port};

#[test]
fn a_named_endpoint_off_the_checked_address_always_warns() {
    let v6: SocketAddr = "[::1]:9333".parse().unwrap();
    let v4: SocketAddr = "127.0.0.1:9333".parse().unwrap();
    let warning = named_warning(v6, &ListenerVerdict::Nothing).unwrap();
    assert!(
        warning.contains("[::1]:9333") && warning.contains("not checked"),
        "{warning}"
    );
    assert_eq!(named_warning(v4, &ListenerVerdict::Nothing), None);
    assert_eq!(named_warning(v6, &ListenerVerdict::Unknown), None);
}

fn untrusted(holder: Option<&str>) -> ListenerVerdict {
    ListenerVerdict::Untrusted {
        holder: holder.map(str::to_owned),
    }
}

#[test]
fn a_visible_holder_is_named_and_launch_comes_first() {
    let error = unverified_port(9222, &untrusted(Some("python3 (pid 1234)")));
    assert!(
        error.message.contains("held by python3 (pid 1234)"),
        "{}",
        error.message
    );
    let suggestion = error.suggestion.unwrap();
    assert!(
        suggestion.starts_with("Run 'tabref open <url> --launch'"),
        "{suggestion}"
    );
    assert!(suggestion.contains("--cdp 9222"), "{suggestion}");
    assert_eq!(error.details.unwrap()["holder"], "python3 (pid 1234)");
}

#[test]
fn a_holder_tabref_cannot_see_is_never_offered_to_cdp() {
    let error = unverified_port(9222, &untrusted(None));
    assert!(
        error.message.contains("another user or of root"),
        "{}",
        error.message
    );
    let suggestion = error.suggestion.unwrap();
    assert!(suggestion.contains("--launch"), "{suggestion}");
    assert!(!suggestion.contains("--cdp"), "{suggestion}");
}

#[test]
fn tables_tabref_cannot_read_are_said_so_and_never_offered_to_cdp() {
    let error = unverified_port(
        9222,
        &ListenerVerdict::CantCheck("the TCP socket table could not be read".into()),
    );
    assert!(
        error.message.contains("could not check"),
        "{}",
        error.message
    );
    assert!(!error.suggestion.unwrap().contains("--cdp"));
    let details = error.details.unwrap();
    assert_eq!(details["kind"], "cdp_endpoint_unverified");
    assert_eq!(details["checked"], false);
}

#[test]
fn a_named_port_warns_unless_a_chrome_of_this_user_holds_it() {
    assert_eq!(named_port_warning(9333, &ListenerVerdict::Unknown), None);
    assert_eq!(named_port_warning(9333, &ListenerVerdict::Nothing), None);
    let named = named_port_warning(9333, &untrusted(Some("socat (pid 7)"))).unwrap();
    assert!(
        named.contains("socat (pid 7)") && named.contains("--cdp"),
        "{named}"
    );
    assert!(
        named_port_warning(9333, &untrusted(None))
            .unwrap()
            .contains("another user")
    );
    assert!(
        named_port_warning(9333, &ListenerVerdict::CantCheck("x".into()))
            .unwrap()
            .contains("could not be checked")
    );
}
