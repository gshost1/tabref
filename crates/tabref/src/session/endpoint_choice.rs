use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use tabref_cdp::{
    BrowserEndpoint, EndpointSpec, HeldPort, LaunchOptions, LaunchedBrowser, ListenerVerdict,
    abandon_launch, check_listener, connect, connect_held, endpoint_from_held, is_chrome_family,
    launch_chrome,
};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode, RefStore};

use super::connected_browser::ConnectedBrowser;
use super::endpoint_refusal::{named_warning, unverified_port};
use super::launch_check::{LaunchCheck, launched_check};
use super::launch_profile::{
    normalize, prepare_profile, requested_profile, running_on_profile, same_dir,
};
use super::launch_record::{
    forget_launched, load_launched, parse_launched, read_launched, save_launched,
};
use super::session_options::SessionOptions;
use super::user_chrome::{GONE_KIND, connect_user_chrome};
use super::user_chrome_record::{UserChromeRecord, forget_user_chrome};

const DEFAULT_PORT: u16 = 9222;
const PROBE_BUDGET: Duration = Duration::from_millis(300);
const REFUSALS: [&str; 2] = ["cdp_endpoint_changed", "cdp_endpoint_mismatch"];

/// Finds the browser and opens the one DevTools connection the command
/// uses. tabref attaches only to a browser it can vouch for, in order:
/// - the endpoint `--cdp`/`TABREF_CDP` names: the user chose it; when its
///   port is not held by a Chrome of this user, the command says so in a
///   warning;
/// - `user_chrome`, the user's own browser `tabref connect --user-chrome`
///   chose (the caller passes it only without `--cdp` and `--launch`), while
///   the same browser process holds its profile and port (see
///   `user_chrome`); a browser that is gone loses the opt-in. Without that
///   opt-in no profile of the user is read;
/// - the browser a previous `open --launch` started, while its port is held
///   by a process running that launch's profile (Linux), or while that very
///   process still runs (pid and start time; Windows: the profile's lock),
///   and its profile's `DevToolsActivePort` still names that port and run
///   (see `launch_check`);
/// - a Chrome of this user on port 9222, checked to own the port before
///   anything is sent to it (Linux only; elsewhere the port must be named);
///   anything else there is refused with kind `cdp_endpoint_unverified`;
/// - when the command allows it, a browser started now.
///
/// A checked port is connected to only at `127.0.0.1:<port>`, and only
/// while it still has the sockets the check saw. A launch record is
/// forgotten once its browser is gone; one that is slow to answer is busy,
/// not gone, so the error is returned rather than a second browser started.
pub(crate) fn connect_browser(
    options: &SessionOptions,
    store: &RefStore,
    user_chrome: Option<(&str, &UserChromeRecord)>,
    deadline: Deadline,
) -> Result<ConnectedBrowser, AppError> {
    if let Some(spec) = &options.cdp {
        return named_browser(spec, deadline);
    }
    if let Some((text, record)) = user_chrome {
        return connect_user_chrome(record, deadline)
            .map(|browser| ConnectedBrowser {
                choice: Some(format!("user:{text}")),
                ..browser
            })
            .inspect_err(|error| {
                if error.kind() == Some(GONE_KIND) {
                    let _ = forget_user_chrome(store);
                }
            });
    }
    let profile = requested_profile(options).map(normalize).transpose()?;
    let profile = profile.as_deref();
    if let Some(found) = launched_browser(store, profile, deadline)? {
        return Ok(found);
    }
    if profile.is_none() {
        if let Some(found) = default_port(deadline, options.launch.is_some())? {
            return Ok(found);
        }
    }
    match &options.launch {
        Some(launch) => start_browser(launch, store, deadline),
        None => Err(no_browser()),
    }
}

fn named_browser(spec: &str, deadline: Deadline) -> Result<ConnectedBrowser, AppError> {
    let endpoint = EndpointSpec::parse(spec)?.resolve(deadline)?;
    let verdict = check_listener(endpoint.port(), &is_chrome_family);
    let warning = named_warning(endpoint.websocket_url.socket_addr(), &verdict);
    let connection = match &verdict {
        ListenerVerdict::Trusted(held) if at_checked_address(&endpoint, held) => {
            connect_held(&endpoint, held, deadline)?
        }
        _ => connect(&endpoint, deadline)?,
    };
    Ok(ConnectedBrowser {
        endpoint,
        connection,
        warning,
        choice: None,
    })
}

fn at_checked_address(endpoint: &BrowserEndpoint, held: &HeldPort) -> bool {
    endpoint.websocket_url.socket_addr() == SocketAddr::from((Ipv4Addr::LOCALHOST, held.port))
}

/// The browser a previous `open --launch` started, unless the command asks
/// for a `profile` that browser does not run (its record is then kept for
/// commands that do not ask).
fn launched_browser(
    store: &RefStore,
    profile: Option<&Path>,
    deadline: Deadline,
) -> Result<Option<ConnectedBrowser>, AppError> {
    let Some(text) = read_launched(store) else {
        return Ok(None);
    };
    let Some(record) = parse_launched(&text) else {
        forget_launched(store);
        return Ok(None);
    };
    if profile.is_some_and(|profile| !same_dir(profile, &record.profile_dir)) {
        return Ok(None);
    }
    let Ok(endpoint) = record.endpoint() else {
        forget_launched(store);
        return Ok(None);
    };
    let connected = match launched_check(&record) {
        LaunchCheck::Held(held) => connect_held(&endpoint, &held, deadline),
        LaunchCheck::Alive => connect(&endpoint, deadline),
        LaunchCheck::Unverified(error) => return Err(error),
        LaunchCheck::Gone => {
            forget_launched(store);
            return Ok(None);
        }
    };
    match connected {
        Ok(connection) => Ok(Some(ConnectedBrowser {
            endpoint,
            connection,
            warning: None,
            choice: Some(format!("launched:{text}")),
        })),
        Err(error) if browser_is_gone(&error, process_alive(record.pid)) => {
            forget_launched(store);
            Ok(None)
        }
        Err(error) if error.code == ErrorCode::Timeout => Err(error.with_suggestion(
            "The browser 'open --launch' started is busy; retry with a longer --timeout",
        )),
        Err(error) => Err(error),
    }
}

/// Whether a failed connection to a launched browser means it is gone: the
/// port refused the connection, or its process is known to have exited.
pub(crate) fn browser_is_gone(error: &AppError, alive: Option<bool>) -> bool {
    error.kind() == Some("cdp_endpoint_unavailable") || alive == Some(false)
}

/// Whether process `pid` exists, where `/proc` can say; `None` elsewhere.
fn process_alive(pid: u32) -> Option<bool> {
    let proc_root = Path::new("/proc");
    proc_root
        .join("self")
        .exists()
        .then(|| proc_root.join(pid.to_string()).exists())
}

fn default_port(
    deadline: Deadline,
    can_launch: bool,
) -> Result<Option<ConnectedBrowser>, AppError> {
    let verdict = check_listener(DEFAULT_PORT, &is_chrome_family);
    let Some(held) = implicit_port(verdict, can_launch)? else {
        return Ok(None);
    };
    let probe = deadline.capped(PROBE_BUDGET);
    let Some(endpoint) = reached(endpoint_from_held(&held, probe))? else {
        return Ok(None);
    };
    Ok(
        reached(connect_held(&endpoint, &held, deadline))?.map(|connection| ConnectedBrowser {
            endpoint,
            connection,
            warning: None,
            choice: None,
        }),
    )
}

/// A step of reaching the implicit port: its value, `None` when the port
/// simply could not be reached (so the search goes on), or the error when
/// the port changed hands or points elsewhere, which the caller must see
/// rather than "no browser" or a second browser of tabref's own.
pub(crate) fn reached<T>(step: Result<T, AppError>) -> Result<Option<T>, AppError> {
    match step {
        Ok(value) => Ok(Some(value)),
        Err(error) if REFUSALS.contains(&error.kind().unwrap_or_default()) => Err(error),
        Err(_) => Ok(None),
    }
}

/// The checked port 9222, when it may be tried without being named: only
/// when a Chrome of this user holds it. Anything else listening there, or a
/// port tabref cannot check, is refused, unless the command may start a
/// browser of its own instead.
pub(crate) fn implicit_port(
    verdict: ListenerVerdict,
    can_launch: bool,
) -> Result<Option<HeldPort>, AppError> {
    match verdict {
        ListenerVerdict::Trusted(held) => Ok(Some(held)),
        ListenerVerdict::Nothing | ListenerVerdict::Unknown => Ok(None),
        _ if can_launch => Ok(None),
        refused => Err(unverified_port(DEFAULT_PORT, &refused)),
    }
}

fn start_browser(
    launch: &LaunchOptions,
    store: &RefStore,
    deadline: Deadline,
) -> Result<ConnectedBrowser, AppError> {
    let (launch, _turn) = match &launch.profile_dir {
        Some(dir) => {
            let (profile, turn) = prepare_profile(dir, deadline)?;
            let launch = LaunchOptions {
                profile_dir: Some(profile),
                ..launch.clone()
            };
            (launch, Some(turn))
        }
        None => (launch.clone(), None),
    };
    let running = launch
        .profile_dir
        .as_deref()
        .and_then(|profile| running_on_profile(profile, !launch.headed));
    let (endpoint, connection, text) = match running {
        Some(adopted) => {
            let endpoint = adopted.endpoint()?;
            let connection = connect(&endpoint, deadline)?;
            (endpoint, connection, save_launched(store, &adopted)?)
        }
        None => {
            let started = launch_chrome(&launch, deadline)?;
            let text = save_launched(store, &started);
            match (text, started.endpoint()) {
                (Ok(text), Ok(endpoint)) => match connect(&endpoint, deadline) {
                    Ok(connection) => (endpoint, connection, text),
                    Err(error) => return Err(abandon(store, &started, error)),
                },
                (Err(error), _) | (_, Err(error)) => return Err(abandon(store, &started, error)),
            }
        }
    };
    Ok(ConnectedBrowser {
        endpoint,
        connection,
        warning: None,
        choice: Some(format!("launched:{text}")),
    })
}

/// Ends a browser this launch started that could not be used, and forgets
/// its record once it is gone; while it still runs, the record (saved before
/// connecting) keeps it reachable for `close --browser`.
fn abandon(store: &RefStore, started: &LaunchedBrowser, error: AppError) -> AppError {
    if abandon_launch(started) && load_launched(store).as_ref() == Some(started) {
        forget_launched(store);
    }
    error
}

fn no_browser() -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        "No browser with DevTools is reachable",
    )
    .with_suggestion(
        "Run 'tabref open <url> --launch' to start one, or start Chrome with \
         --remote-debugging-port=<port> and pass --cdp <port>",
    )
    .with_details(serde_json::json!({ "kind": "cdp_no_browser" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "endpoint_choice_tests.rs"]
mod tests;
