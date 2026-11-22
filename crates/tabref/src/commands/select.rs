use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, LocateIntent, SelectOutcome, read_element, select_option};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::{TargetSpec, looks_like_ref};
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

/// Picks the option `option` names in the select `target` names.
pub(crate) fn run(session: &mut Session, target: &str, option: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = resolve_target(session, &spec, LocateIntent::Select, deadline)?;
    let acted = act(session, target, option, deadline)?;
    let Some(effects) = acted.effects.as_ref() else {
        return full_report(session, acted);
    };
    let what = if effects.new_tab.is_some() {
        "the new tab"
    } else if effects.settled.navigated {
        "the page"
    } else {
        return full_report(session, acted);
    };
    let mut delivered = Outcome::new(format!("selected {}", acted.summary), acted.data.clone());
    delivered.data["navigated"] = json!(effects.settled.navigated);
    if let Some(tab) = &effects.new_tab {
        delivered.data["new_tab"] = json!(tab);
        delivered.push_line("opened a new tab, now the current tab");
    }
    if let Some(page) = &effects.page {
        delivered.data["url"] = json!(page.url);
        delivered.data["title"] = json!(page.title);
    }
    Ok(full_report(session, acted).unwrap_or_else(|error| {
        let line = unread(&mut delivered.data, what, &error);
        delivered.push_line(&line);
        delivered
    }))
}

/// Picks a native select or ARIA combobox option and reads the result back.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    option: &str,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Select, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let chosen = select_option(
        session.connection(),
        &target.session,
        &target.token,
        option,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline);
    let mut acted = if chosen.combobox {
        combobox_report(session, target, chosen, deadline)
    } else {
        native_report(session, target, chosen, deadline)
    };
    match effects {
        Ok(mut effects) => {
            if acted.data["combobox"] == true
                && !effects.settled.navigated
                && !effects.settled.same_document
            {
                effects.page = None;
            }
            acted.effects = Some(effects);
        }
        Err(error) => add_unread(&mut acted, "the page after selection", &error),
    }
    Ok(acted)
}

fn native_report(
    session: &mut Session,
    target: ActTarget,
    chosen: SelectOutcome,
    deadline: Deadline,
) -> Acted {
    let reading = read_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    );
    let verified = reading
        .as_ref()
        .is_ok_and(|reading| reading.value.as_deref() == Some(chosen.value.as_str()));
    let mut summary = match &chosen.previous {
        Some(previous) => format!(
            "{} {} -> {}",
            target.label,
            quote_text(previous),
            quote_text(&chosen.label)
        ),
        None => format!("{} = {}", target.label, quote_text(&chosen.label)),
    };
    if chosen.value != chosen.label {
        summary.push_str(&format!(" (value {})", quote_text(&chosen.value)));
    }
    if reading.is_ok() && !verified {
        summary.push_str(" (the select did not keep it)");
    }
    let mut acted = Acted {
        verb: "select",
        summary,
        data: json!({
            "action": "select",
            "target": target.label,
            "value": chosen.value,
            "label": chosen.label,
            "previous": chosen.previous,
            "verified": verified,
        }),
        effects: None,
        known: target.known,
    };
    if let Err(error) = reading {
        add_unread(&mut acted, "the select's value", &error);
    }
    acted
}

fn combobox_report(
    session: &mut Session,
    target: ActTarget,
    chosen: SelectOutcome,
    deadline: Deadline,
) -> Acted {
    let reading = SelectOutcome::read_combobox_value(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    );
    let label = target
        .label
        .split_once(' ')
        .map(|(first, _)| first)
        .filter(|first| looks_like_ref(first))
        .unwrap_or(&target.label);
    let subject = if label == "combobox" {
        label.to_owned()
    } else {
        format!("{label} combobox")
    };
    let mut acted = Acted {
        verb: "select",
        summary: format!(
            "{subject} {} -> {}",
            quote_text(chosen.previous.as_deref().unwrap_or_default()),
            quote_text(reading.as_deref().unwrap_or(&chosen.label)),
        ),
        data: json!({
            "action": "select",
            "target": target.label,
            "combobox": true,
            "value": reading.as_ref().ok(),
            "label": chosen.label,
            "previous": chosen.previous,
            "verified": reading.is_ok(),
        }),
        effects: None,
        known: target.known,
    };
    if let Err(error) = reading {
        add_unread(&mut acted, "the combobox's value", &error);
    }
    acted
}

fn add_unread(acted: &mut Acted, what: &str, error: &AppError) {
    let line = unread(&mut acted.data, what, error);
    acted.summary.push('\n');
    acted.summary.push_str(&line);
}

fn unread(data: &mut Value, what: &str, error: &AppError) -> String {
    let said = format!(
        "could not read {what} ({}); run tabref snap to see it",
        error.message
    );
    data["report_error"] = json!(match data["report_error"].as_str() {
        Some(earlier) => format!("{earlier}; {said}"),
        None => said.clone(),
    });
    format!("note: {said}")
}
