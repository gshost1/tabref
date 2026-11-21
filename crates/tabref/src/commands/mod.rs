pub(crate) mod act_target;
pub(crate) mod acted;
pub(crate) mod action_report;
mod action_report_changes;
pub(crate) mod app_guard;
pub(crate) mod batch_step;
pub(crate) mod check;
pub(crate) mod click;
pub(crate) mod close;
pub(crate) mod close_browser;
pub(crate) mod connect;
pub(crate) mod dialog_report;
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
pub(crate) mod find_room;
pub(crate) mod focus;
pub(crate) mod hover;
pub(crate) mod js;
pub(crate) mod kept_download;
pub(crate) mod new_tab;
pub(crate) mod open;

pub(crate) mod open_request;
pub(crate) mod open_settle;
pub(crate) mod page_url;
pub(crate) mod page_view;
pub(crate) mod press;
pub(crate) mod read;
pub(crate) mod redrawn_rows;
pub(crate) mod screenshot;
pub(crate) mod screenshot_path;
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
    let reads = only_reads(&cli.command);
    if !reads {
        session.note_page_action();
    }
    let outcome = dispatch_cli(&mut session, cli);
    if !reads {
        session.note_page_action();
    }
    session.with_warning(outcome)
}

/// Runs the parsed command line against `session`, whose connection holds
/// the command's `--dialog` answer for the first dialog it applies to that
/// the whole command meets (`do` included). It is set when the session
/// opens, or when the daemon begins the command after accepting any dialog
/// left open since the last one. The answer is cleared after the command,
/// so a kept connection never carries it into the next one. The dialogs no
/// part of the report took are added to it (or to its error), those from
/// another tab marked so, and a result says when no dialog used the answer.
pub(crate) fn dispatch_cli(session: &mut Session, cli: &Cli) -> Result<Outcome, AppError> {
    let started_headless_profile = session.take_headless_profile_launch();
    let outcome = dispatch(session, &cli.command);
    let unused = session.connection().take_dialog_choice();
    let left = dialog_report::navigation_dialogs(session, None);
    let left = dialog_report::mark_other_tabs(session, left);
    let mut outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => return Err(dialog_report::error_with_dialogs(error, &left)),
    };
    dialog_report::after_report(&mut outcome, &left);
    if started_headless_profile {
        outcome.push_line("note: headless browser (add --headed to see it)");
    }
    if let Some(unused) = unused {
        outcome.push_line(&unused_note(&unused));
        action_report::set(&mut outcome.data, "dialog_unused", serde_json::json!(true));
    }
    Ok(outcome)
}

/// Says that `--dialog` found nothing to answer, and that a dialog opening
/// after the command is accepted as usual.
fn unused_note(choice: &tabref_cdp::DialogChoice) -> String {
    let (given, wanted) = match choice {
        tabref_cdp::DialogChoice::Dismiss => {
            ("dismiss".to_owned(), "confirm, prompt or leave-page dialog")
        }
        tabref_cdp::DialogChoice::Accept { text: Some(text) } => (
            format!("accept={}", crate::view::node_line::quote_text(text)),
            "prompt",
        ),
        tabref_cdp::DialogChoice::Accept { text: None } => {
            ("accept".to_owned(), "confirm, prompt or leave-page dialog")
        }
    };
    format!(
        "note: --dialog {given} was not used: no {wanted} opened during this command; one that \
         opens after it is accepted, and the next command reports it"
    )
}

/// Whether `command` only reads the page (or navigates and reads it), so it
/// may keep and reuse a capture in the daemon. Any other command may change
/// a page in ways no page script sees, such as a hover: in the daemon it
/// clears every kept capture, and in-process it stamps the ref store so the
/// daemon's kept captures are not used again.
pub(crate) fn only_reads(command: &Command) -> bool {
    matches!(
        command,
        Command::Open { .. } | Command::Snap { .. } | Command::Find { .. } | Command::Read { .. }
    )
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
        dialog: cli.dialog.clone(),
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
            to,
        } => snap::run(
            session,
            &ViewRequest {
                all: *all,
                full: *full,
                root: root.clone(),
                rows: rows.clone(),
                from: *from,
                to: *to,
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
        Command::Focus { target } => focus::run(session, target),
        Command::Hover { target } => hover::run(session, target),
        Command::Screenshot { target, out } => {
            screenshot::run(session, target.as_deref(), out.as_deref())
        }
        Command::Scroll {
            direction,
            target,
            px,
            until,
            to_end,
        } => scroll::run_loop(
            session,
            direction,
            target.as_deref(),
            *px,
            (until.as_deref(), *to_end),
        ),
        Command::Upload { target, files } => upload::run(session, target, files),
        Command::Find { text, role } => find::run(session, &text.join(" "), role.as_deref()),
        Command::Read {
            target,
            section,
            rows,
            from,
            to,
            ..
        } => read::run(
            session,
            &ViewRequest {
                root: target.clone(),
                rows: rows.clone(),
                from: *from,
                to: *to,
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
