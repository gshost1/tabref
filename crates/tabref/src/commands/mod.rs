pub(crate) mod act_target;
pub(crate) mod acted;
pub(crate) mod action_report;
pub(crate) mod app_guard;
pub(crate) mod batch_step;
pub(crate) mod check;
pub(crate) mod click;
pub(crate) mod close;
pub(crate) mod close_browser;
pub(crate) mod connect;
pub(crate) mod do_batch;
pub(crate) mod do_expand;
pub(crate) mod do_read_steps;
pub(crate) mod do_step;
pub(crate) mod download_folder;
pub(crate) mod download_ledger;
pub(crate) mod download_lines;
pub(crate) mod download_place;
pub(crate) mod download_tend;
pub(crate) mod download_wait;
pub(crate) mod downloads;
pub(crate) mod fill;
pub(crate) mod find;
pub(crate) mod hover;
pub(crate) mod js;
pub(crate) mod kept_download;
pub(crate) mod new_tab;
pub(crate) mod open;
pub(crate) mod open_request;
pub(crate) mod page_url;
pub(crate) mod page_view;
pub(crate) mod press;
pub(crate) mod read;
pub(crate) mod scroll;
pub(crate) mod select;
pub(crate) mod skill;
pub(crate) mod snap;
pub(crate) mod step_done;
pub(crate) mod step_words;
pub(crate) mod tab;
pub(crate) mod tabs;
pub(crate) mod target_spec;
pub(crate) mod type_text;
pub(crate) mod upload;
pub(crate) mod wait;
pub(crate) mod wait_condition;
pub(crate) mod wait_flags;

use tabref_cdp::LaunchOptions;
use tabref_core::AppError;

use crate::cli::Cli;
use crate::cli::command::Command;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::session_options::SessionOptions;
use crate::view::view_request::ViewRequest;

/// Runs the parsed command. Every command but `skill` runs against one
/// [`Session`], built here and nowhere else.
pub(crate) fn run(cli: &Cli) -> Result<Outcome, AppError> {
    if let Command::Skill { topic } = &cli.command {
        return Ok(skill::run(topic.as_deref()));
    }
    let mut session = Session::open(&session_options(cli))?;
    let outcome = dispatch(&mut session, &cli.command);
    session.with_warning(outcome)
}

/// What the parsed command asks of its session.
pub(crate) fn session_options(cli: &Cli) -> SessionOptions {
    let launch = match &cli.command {
        Command::Open {
            launch: true,
            headed,
            profile,
            ..
        } => Some(LaunchOptions {
            headed: *headed,
            profile_dir: profile
                .as_deref()
                .map(|dir| std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf())),
            extra_args: extra_browser_args(),
            ..LaunchOptions::default()
        }),
        _ => None,
    };
    SessionOptions {
        cdp: cli.cdp.clone(),
        tab: cli.tab.clone(),
        timeout_ms: cli.timeout_ms(),
        launch,
        stdin: None,
    }
}

/// Runs `command` against `session`, in-process or inside the daemon.
pub(crate) fn dispatch(session: &mut Session, command: &Command) -> Result<Outcome, AppError> {
    match command {
        Command::Open {
            url,
            new_tab,
            download_dir,
            no_download_dir,
            download_max_mb,
            ..
        } => open::run(
            session,
            url,
            &open_request::OpenRequest {
                new_tab: *new_tab,
                download_dir: download_dir.clone(),
                clear_download_dir: *no_download_dir,
                download_max_mb: *download_max_mb,
            },
        ),
        Command::Snap {
            all,
            root,
            rows,
            full,
            from,
        } => snap::run(
            session,
            &ViewRequest {
                all: *all,
                full: *full,
                root: root.clone(),
                rows: rows.clone(),
                from: *from,
                section: None,
            },
        ),
        Command::Click { target } => click::run(session, target, 1),
        Command::DoubleClick { target } => click::run(session, target, 2),
        Command::Fill { target, text } => fill::run(session, target, text),
        Command::Type { target, text } => type_text::run(session, target, text),
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
        Command::Read {
            target,
            section,
            rows,
            from,
            ..
        } => read::run(
            session,
            &ViewRequest {
                root: target.clone(),
                rows: rows.clone(),
                from: *from,
                section: section.clone(),
                ..ViewRequest::default()
            },
        ),
        Command::Wait {
            text,
            gone,
            url,
            idle,
            download,
            target,
            state,
        } => wait::run(
            session,
            wait_condition::Condition::from_flags(&wait_flags::WaitFlags {
                text: text.as_deref(),
                gone: gone.as_deref(),
                url: url.as_deref(),
                idle: *idle,
                download: *download,
                target: target.as_deref(),
                state: state.as_deref(),
            })?,
        ),
        Command::Js { expression } => js::run(session, expression),
        Command::Do {
            steps,
            step_timeout,
        } => do_batch::run(session, steps.as_deref(), *step_timeout),
        Command::Tabs => tabs::run(session),
        Command::Tab { tab } => tab::run(session, tab),
        Command::Close { browser: true, .. } => Err(AppError::invalid_args(
            "'close --browser' is handled by the client, not inside a session",
        )),
        Command::Close { tab, .. } => close::run(session, tab.as_deref()),
        Command::Skill { topic } => Ok(skill::run(topic.as_deref())),
        Command::Daemon { action } => crate::daemon::control::run_in_session(session, action),
        Command::Connect { .. } => Err(AppError::invalid_args(
            "'connect' is handled by the client, not inside a session",
        )),
    }
}

/// Extra browser flags for `open --launch`, from `TABREF_CHROME_ARGS`,
/// split on whitespace.
fn extra_browser_args() -> Vec<String> {
    std::env::var("TABREF_CHROME_ARGS")
        .map(|args| args.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}
