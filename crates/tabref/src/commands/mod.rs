pub(crate) mod action_report;
pub(crate) mod check;
pub(crate) mod click;
pub(crate) mod close;
pub(crate) mod fill;
pub(crate) mod find;
pub(crate) mod hover;
pub(crate) mod js;
pub(crate) mod open;
pub(crate) mod press;
pub(crate) mod read;
pub(crate) mod scroll;
pub(crate) mod select;
pub(crate) mod skill;
pub(crate) mod snap;
pub(crate) mod tab;
pub(crate) mod tabs;
pub(crate) mod upload;
pub(crate) mod wait;
pub(crate) mod wait_condition;

use tabref_cdp::LaunchOptions;
use tabref_core::AppError;

use crate::cli::Cli;
use crate::cli::command::Command;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::session_options::SessionOptions;

/// Runs the parsed command. Every command but `skill` runs against one
/// [`Session`], built here and nowhere else.
pub(crate) fn run(cli: &Cli) -> Result<Outcome, AppError> {
    if let Command::Skill = cli.command {
        return Ok(skill::run());
    }
    let mut session = Session::open(&session_options(cli))?;
    dispatch(&mut session, &cli.command)
}

/// What the parsed command asks of its session.
pub(crate) fn session_options(cli: &Cli) -> SessionOptions {
    let launch = match &cli.command {
        Command::Open {
            launch: true,
            headed,
            ..
        } => Some(LaunchOptions {
            headed: *headed,
            extra_args: extra_browser_args(),
            ..LaunchOptions::default()
        }),
        _ => None,
    };
    SessionOptions {
        cdp: cli.cdp.clone(),
        tab: cli.tab.clone(),
        timeout_ms: cli.timeout,
        launch,
        stdin: None,
    }
}

/// Runs `command` against `session`, in-process or inside the daemon.
pub(crate) fn dispatch(session: &mut Session, command: &Command) -> Result<Outcome, AppError> {
    match command {
        Command::Open { url, .. } => open::run(session, url),
        Command::Snap { all, root } => snap::run(session, *all, root.as_deref()),
        Command::Click { target } => click::run(session, target),
        Command::Fill { target, text } => fill::run(session, target, text),
        Command::Select { target, option } => select::run(session, target, option),
        Command::Check { target } => check::run(session, target, true),
        Command::Uncheck { target } => check::run(session, target, false),
        Command::Press { key, target } => press::run(session, key, target.as_deref()),
        Command::Hover { target } => hover::run(session, target),
        Command::Scroll {
            direction,
            target,
            px,
        } => scroll::run(session, direction, target.as_deref(), *px),
        Command::Upload { target, files } => upload::run(session, target, files),
        Command::Find { text, role } => find::run(session, text, role.as_deref()),
        Command::Read { target } => read::run(session, target.as_deref()),
        Command::Wait {
            text,
            gone,
            url,
            idle,
        } => wait::run(
            session,
            wait_condition::Condition::from_flags(
                text.as_deref(),
                gone.as_deref(),
                url.as_deref(),
                *idle,
            )?,
        ),
        Command::Js { expression } => js::run(session, expression),
        Command::Tabs => tabs::run(session),
        Command::Tab { tab } => tab::run(session, tab),
        Command::Close { tab } => close::run(session, tab.as_deref()),
        Command::Skill => Ok(skill::run()),
        Command::Daemon { action } => crate::daemon::control::run_in_session(session, action),
    }
}

/// Extra browser flags for `open --launch`, from `TABREF_CHROME_ARGS`,
/// split on whitespace.
fn extra_browser_args() -> Vec<String> {
    std::env::var("TABREF_CHROME_ARGS")
        .map(|args| args.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}
