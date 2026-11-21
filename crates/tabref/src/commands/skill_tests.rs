#![allow(clippy::unwrap_used, clippy::panic)]

use clap::Parser;

use super::run;
use crate::cli::Cli;
use crate::commands::batch_step::BatchStep;
use crate::commands::do_expand::expand;
use crate::commands::do_step::split_value;
use crate::commands::step_words::split_steps;

const STEP_VERBS: &[&str] = &[
    "open", "click", "double-click", "fill", "type", "select", "check", "uncheck", "press",
    "focus", "hover", "scroll", "wait", "expect", "read", "find", "js", "snap",
];

fn shell_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => word.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, ' ') if started => {
                words.push(std::mem::take(&mut word));
                started = false;
            }
            (None, ' ') => {}
            (None, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    assert!(quote.is_none(), "unclosed quote in {line}");
    if started {
        words.push(word);
    }
    words
}

fn inline_commands(guide: &str) -> Vec<String> {
    guide
        .split('`')
        .skip(1)
        .step_by(2)
        .flat_map(|span| span.split(" && "))
        .filter(|command| command.starts_with("tabref "))
        .map(str::to_owned)
        .collect()
}

fn heredoc_batches(guide: &str) -> Vec<String> {
    guide
        .split("tabref do <<'EOF'\n")
        .skip(1)
        .map(|rest| rest.split("\n    EOF").next().unwrap().to_owned())
        .collect()
}

fn assert_steps_parse(text: &str) {
    let steps: Vec<BatchStep> = expand(&split_steps(text).unwrap()).unwrap();
    assert!(!steps.is_empty(), "{text}");
    for step in &steps {
        assert!(STEP_VERBS.contains(&step.verb().as_str()), "{}", step.source);
        if matches!(step.verb().as_str(), "fill" | "type" | "select") {
            split_value(step).unwrap();
        }
    }
}

#[test]
fn every_command_example_in_the_guides_parses() {
    for guide in [run(None).text, run(Some("more")).text] {
        let commands = inline_commands(&guide);
        assert!(!commands.is_empty());
        for command in commands {
            let words = shell_words(&command);
            if let Err(error) = Cli::try_parse_from(&words) {
                panic!("{command}: {error}");
            }
            if words.get(1).map(String::as_str) == Some("do") {
                assert_steps_parse(&words[2]);
            }
        }
        for batch in heredoc_batches(&guide) {
            Cli::try_parse_from(["tabref", "do"]).unwrap();
            assert_steps_parse(&batch);
        }
    }
}

#[test]
fn the_guide_explains_how_to_start_chrome_when_needed() {
    let guide = run(None).text;
    assert!(guide.contains("If no browser is connected, use `tabref open <url> --launch`."));
    assert!(!guide.contains("Chrome is already connected"));
    assert!(!guide.contains("Chrome is connected"));
}

#[test]
fn the_guide_stays_short_and_points_to_the_rare_commands() {
    let guide = run(None).text;
    assert!(guide.len() <= 1_800, "{} bytes", guide.len());
    assert!(guide.contains("tabref skill more` for waits, `connect` to your own Chrome"));
    let more = run(Some("more")).text;
    for rare in [
        "--user-chrome",
        "--profile DIR",
        "--download-dir",
        "Electron",
        "wait --download",
        "tabref tabs",
        "tabref js",
    ] {
        assert!(more.contains(rare), "{rare}");
        assert!(!guide.contains(rare), "{rare} belongs in skill more");
    }
}

#[test]
fn the_guide_does_not_offer_upload_as_a_batch_step() {
    let guide = run(None).text;
    let steps = guide
        .lines()
        .find(|line| line.starts_with("Steps:"))
        .unwrap_or_default();
    assert!(steps.contains("but `upload`"), "{steps}");
}

#[test]
fn the_guide_explains_when_commands_wait_and_when_to_read_the_result() {
    let guide = run(None).text;
    for rule in [
        "read that instead of taking a snapshot",
        "in one `tabref do`",
        "each step prints its result",
        "repeat `scroll down` in a `for`",
        "Commands wait for the page. Don't add sleeps.",
    ] {
        assert!(guide.contains(rule), "{rule}");
    }
    for misleading in [
        "not shell loops",
        "do not\nsnapshot again",
        "waits for the page itself",
    ] {
        assert!(!guide.contains(misleading), "{misleading}");
    }
}

#[test]
fn the_batch_example_fills_checks_and_submits_in_one_call() {
    let guide = run(None).text;
    let batches = heredoc_batches(&guide);
    assert_eq!(batches.len(), 1);
    let verbs: Vec<String> = split_steps(&batches[0])
        .unwrap()
        .iter()
        .map(BatchStep::verb)
        .collect();
    assert!(verbs.iter().filter(|verb| *verb == "fill").count() >= 2, "{verbs:?}");
    assert!(verbs.contains(&"check".to_owned()), "{verbs:?}");
    assert_eq!(verbs.last().map(String::as_str), Some("click"), "{verbs:?}");
    assert!(!verbs.contains(&"snap".to_owned()), "{verbs:?}");
}

#[test]
fn the_guide_loops_pages_in_one_call() {
    let guide = run(None).text;
    let paging = inline_commands(&guide)
        .into_iter()
        .find(|command| command.contains("open ?p={p}"))
        .unwrap_or_else(|| panic!("no paging loop in the guide"));
    assert!(paging.starts_with("tabref do 'for p in"), "{paging}");
    assert!(paging.contains("; find "), "{paging}");
}

#[test]
fn the_guide_keeps_the_forms_a_model_copies() {
    let guide = run(None).text;
    for form in [
        "across the whole page, not the first block",
        "[e9 select Size = \"M\" options: S | M | L]",
        "visible|enabled|hidden|gone|value=X",
        "tabref read --section \"<heading>\"",
        "`\"Edit\" in \"<row text>\"`",
        "`upload T FILE`",
    ] {
        assert!(guide.contains(form), "{form}");
    }
}

#[test]
fn the_guide_leaves_error_fixes_to_the_error_lines() {
    let guide = run(None).text;
    for code in ["ELEMENT_NOT_FOUND", "AMBIGUOUS_TARGET", "STALE_REF"] {
        assert!(!guide.contains(code), "{code}");
    }
}
