#![allow(clippy::unwrap_used, clippy::panic)]

use clap::{CommandFactory, Parser};

use super::run;
use crate::cli::Cli;
use crate::commands::do_expand::expand;
use crate::commands::step_words::{split_steps, split_words};
use crate::commands::wait_condition::Condition;

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

fn spans(guide: &str) -> Vec<String> {
    guide
        .split('`')
        .skip(1)
        .step_by(2)
        .flat_map(|span| span.split(" && "))
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

fn variants(words: &[String]) -> Vec<Vec<String>> {
    let mut all = vec![Vec::new()];
    for word in words {
        let choices: Vec<Option<&str>> = match word.strip_prefix('[') {
            Some(rest) if word.ends_with(']') => vec![None, Some(&rest[..rest.len() - 1])],
            _ => word.split('|').map(Some).collect(),
        };
        all = all
            .into_iter()
            .flat_map(|done| {
                choices.iter().map(move |choice| {
                    let mut next = done.clone();
                    next.extend(choice.map(str::to_owned));
                    next
                })
            })
            .collect();
    }
    all
}

fn known_verb(verb: &str) -> bool {
    matches!(verb, "expect" | "goto")
        || Cli::command()
            .get_subcommands()
            .any(|command| command.get_name() == verb)
}

fn check_command(words: &[String]) -> Result<(), String> {
    let verb = words.first().map(String::as_str).unwrap_or_default();
    let rest = words.get(1..).unwrap_or_default();
    let bare_wait = verb == "wait" && !rest.first().is_some_and(|word| word.starts_with("--"));
    if verb == "expect" || bare_wait {
        let words = split_words(&rest.join(" ")).map_err(|error| error.message)?;
        return Condition::from_words(&words)
            .map(drop)
            .map_err(|error| error.message);
    }
    let verb = if verb == "goto" { "open" } else { verb };
    let args = std::iter::once("tabref")
        .chain(std::iter::once(verb))
        .chain(rest.iter().map(String::as_str));
    Cli::try_parse_from(args).map_err(|error| error.to_string())?;
    match rest {
        [steps, ..] if verb == "do" => check_steps(steps),
        _ => Ok(()),
    }
}

fn check_steps(text: &str) -> Result<(), String> {
    let steps = expand(&split_steps(text).map_err(|error| error.message)?)
        .map_err(|error| error.message)?;
    if steps.is_empty() {
        return Err(format!("no steps in {text}"));
    }
    for step in &steps {
        let words: Vec<String> = step.words.iter().map(|word| word.text.clone()).collect();
        if !known_verb(&step.verb()) || step.verb() == "upload" {
            return Err(format!("{}: not a step", step.source));
        }
        check_command(&words).map_err(|error| format!("{}: {error}", step.source))?;
    }
    Ok(())
}

fn check_span(span: &str) -> Result<(), String> {
    if span.starts_with("for ") {
        return check_steps(&format!("{span}\nsnap\nend"));
    }
    let prefixed = span.starts_with("tabref ");
    let words = shell_words(span.strip_prefix("tabref ").unwrap_or(span));
    let Some(verb) = words.first() else {
        return Ok(());
    };
    if !prefixed && (words.len() == 1 || !known_verb(verb)) {
        return Ok(());
    }
    variants(&words)
        .iter()
        .try_for_each(|variant| check_command(variant))
        .map_err(|error| format!("{span}: {error}"))
}

#[test]
fn every_command_example_in_the_guides_parses() {
    for guide in [run(None).text, run(Some("more")).text] {
        for span in spans(&guide) {
            if let Err(error) = check_span(&span) {
                panic!("{error}");
            }
        }
        for batch in heredoc_batches(&guide) {
            if let Err(error) = check_steps(&batch) {
                panic!("{error}");
            }
        }
    }
}

#[test]
fn the_example_check_refuses_flags_and_steps_the_cli_does_not_take() {
    for wrong in [
        "tabref do 'for p in 2 3; open --bogus ?p={p}; end'",
        "tabref do 'find --exact TEXT'",
        "tabref do 'check --force \"I agree\"'",
        "tabref do 'upload T FILE'",
        "tabref do 'expect'",
        "tabref CMD --help",
        "press --bogus",
        "scroll down|T --bogus",
        "for n in 1 2; scroll --bogus",
        "scroll down --until",
        "scroll down --to-end --until TEXT",
        "tabref do \"scroll down --to-end --bogus\"",
    ] {
        assert!(check_span(wrong).is_err(), "{wrong}");
    }
    assert!(check_steps("fill \"Name\" Ada\nfrobnicate x").is_err());
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
    assert!(guide.len() <= 2_100, "{} bytes", guide.len());
    assert!(guide.contains("tabref skill more` for waits, `connect` to your own Chrome"));
    assert!(guide.contains("`tabref js \"EXPR\"` returns a page value as JSON."));
    let more = run(Some("more")).text;
    for rare in [
        "--user-chrome",
        "--profile DIR",
        "--download-dir",
        "Electron",
        "wait --download",
        "tabref tabs",
        "`focus T`",
        "`double-click T`",
        "`press KEY [T]`",
        "`tabref read` prints the main text",
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
        "Snap only if the output\ndid not show what you need",
        "`STALE_REF`: use the fresh ref it printed.",
        "Lazy list: `scroll down --until \"TEXT\"` or `scroll down --to-end`",
        "if it says more may load, run it again",
        "Commands wait for the page. Don't add sleeps.",
    ] {
        assert!(guide.contains(rule), "{rule}");
    }
    for misleading in [
        "not shell loops",
        "for n in 1 2 3 4 5 6",
        "do not\nsnapshot again",
        "waits for the page itself",
        "unless unsure",
        "for the rest: run it",
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
        .map(|step| step.verb())
        .collect();
    assert!(
        verbs.iter().filter(|verb| *verb == "fill").count() >= 2,
        "{verbs:?}"
    );
    assert!(verbs.contains(&"check".to_owned()), "{verbs:?}");
    assert_eq!(verbs.last().map(String::as_str), Some("click"), "{verbs:?}");
    assert!(!verbs.contains(&"snap".to_owned()), "{verbs:?}");
}

#[test]
fn the_guide_loops_pairs_and_pages_without_a_fixed_address() {
    let guide = run(None).text;
    let loops: Vec<String> = spans(&guide)
        .into_iter()
        .filter(|span| span.starts_with("for "))
        .collect();
    assert!(
        loops.iter().any(|span| {
            let names = span.split(" in ").next().unwrap_or_default();
            names.split(' ').count() == 3
        }),
        "{loops:?}"
    );
    let paging = guide
        .lines()
        .find(|line| line.starts_with("Pages:"))
        .unwrap_or_default();
    assert!(paging.contains("`Next` or `2` link's address"), "{paging}");
    assert!(guide.contains("no address: `click \"Next\"` in the loop"));
    assert!(guide.contains("A cut view ends with an outline of the rest; if what you need is not shown, use its `--from N`"));
    assert!(guide.contains("`open \"<address with {n}>\"`"));
    for fixed in ["?p=", "?page=", "&page="] {
        assert!(!guide.contains(fixed), "{fixed}");
    }
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
        "repeats `(1/3)`",
        "`type T TEXT` (appends)",
        "(hidden text too)",
        "`\"Reply\" after \"<text>\"`",
        "`[e7 Next -> <address>]`",
    ] {
        assert!(guide.contains(form), "{form}");
    }
}

#[test]
fn the_guide_leaves_error_fixes_to_the_error_lines() {
    let guide = run(None).text;
    for code in ["ELEMENT_NOT_FOUND", "AMBIGUOUS_TARGET"] {
        assert!(!guide.contains(code), "{code}");
    }
}
