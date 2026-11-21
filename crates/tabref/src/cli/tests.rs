#![allow(clippy::unwrap_used, clippy::panic)]

use clap::Parser;

use super::{Cli, command::Command};

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("tabref").chain(args.iter().copied())).unwrap()
}

#[test]
fn shell_split_qualifiers_are_literal_names_and_pairs() {
    for verb in [
        "click",
        "double-click",
        "hover",
        "focus",
        "check",
        "uncheck",
    ] {
        for pair in [
            ["in", "row"],
            ["after", "Billing"],
            ["before", "Shipping"],
            ["nth", "2"],
        ] {
            let cli = parse(&[verb, "Edit", pair[0], pair[1]]);
            let target = match &cli.command {
                Command::Click { target }
                | Command::DoubleClick { target }
                | Command::Hover { target }
                | Command::Focus { target }
                | Command::Check { target }
                | Command::Uncheck { target } => target,
                _ => panic!("wrong command"),
            };
            assert_eq!(target, &format!("\"Edit\" {} \"{}\"", pair[0], pair[1]));
        }
    }
}

#[test]
fn value_commands_reserve_the_last_word() {
    for verb in ["fill", "type", "select"] {
        let cli = parse(&[verb, "Email", "in", "Billing", "a@b.c"]);
        let (target, value) = match &cli.command {
            Command::Fill { target, text } | Command::Type { target, text } => (target, text),
            Command::Select { target, option } => (target, option),
            _ => panic!("wrong command"),
        };
        assert_eq!(target, "\"Email\" in \"Billing\"");
        assert_eq!(value, "a@b.c");
    }
}

#[test]
fn malformed_extra_words_explain_quoting() {
    for args in [
        vec!["click", "Sign", "up"],
        vec!["hover", "Edit", "nth"],
        vec!["fill", "Email", "nth", "zero", "value"],
    ] {
        let error =
            Cli::try_parse_from(std::iter::once("tabref").chain(args.iter().copied())).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(
            error
                .to_string()
                .contains(&format!("quote the whole target: tabref {} '", args[0])),
            "{error}"
        );
    }
}

#[test]
fn one_argument_keeps_its_original_spelling() {
    for target in [
        "Add to cart",
        "Edit nth 2",
        "e4",
        "button \"Save\"",
        "O'Reilly",
        "",
    ] {
        let cli = parse(&["click", target]);
        let Command::Click { target: actual } = &cli.command else {
            panic!("wrong command")
        };
        assert_eq!(actual, target);
    }
}

#[test]
fn qualifiers_preserve_literal_quotes_keywords_and_backslashes() {
    use crate::commands::target_spec::TargetSpec;

    let cli = parse(&[
        "click",
        "button",
        "nth",
        "2",
        "in",
        "O'Reilly \\\"Billing\\\"",
    ]);
    let Command::Click { target } = &cli.command else {
        panic!("wrong command")
    };
    let TargetSpec::Find(locator) = TargetSpec::from_arg(target).unwrap() else {
        panic!("wrong target")
    };
    assert_eq!(locator.name.as_deref(), Some("button"));
    assert_eq!(locator.role, None);
    assert_eq!(locator.nth, Some(2));
    assert_eq!(locator.scope.as_deref(), Some("O'Reilly \\\"Billing\\\""));
}

#[test]
fn optional_targets_options_and_upload_files_keep_their_places() {
    let cli = parse(&[
        "--json",
        "press",
        "Enter",
        "Email",
        "in",
        "Billing",
        "--timeout",
        "700",
    ]);
    assert!(cli.json);
    assert_eq!(cli.timeout, Some(700));
    let Command::Press { key, target } = &cli.command else {
        panic!("wrong command")
    };
    assert_eq!(key, "Enter");
    assert_eq!(target.as_deref(), Some("\"Email\" in \"Billing\""));
    let cli = parse(&["screenshot", "Save", "nth", "2", "--out", "image.png"]);
    let Command::Screenshot { target, out } = &cli.command else {
        panic!("wrong command")
    };
    assert_eq!(target.as_deref(), Some("\"Save\" nth \"2\""));
    assert_eq!(out.as_deref(), Some(std::path::Path::new("image.png")));
    let cli = parse(&["upload", "File", "in", "Billing", "one.txt", "two.txt"]);
    let Command::Upload { target, files } = &cli.command else {
        panic!("wrong command")
    };
    assert_eq!(target, "\"File\" in \"Billing\"");
    assert_eq!(
        files,
        &[
            std::path::PathBuf::from("one.txt"),
            std::path::PathBuf::from("two.txt")
        ]
    );
    for args in [vec!["press", "Enter"], vec!["screenshot"], vec!["scroll"]] {
        parse(&args);
    }
}

#[test]
fn scroll_accepts_qualified_elements_and_containers() {
    for args in [
        vec!["scroll", "Save", "in", "Billing"],
        vec!["scroll", "down", "Pane", "nth", "2"],
    ] {
        let cli = parse(&args);
        let Command::Scroll {
            direction, target, ..
        } = &cli.command
        else {
            panic!("wrong command")
        };
        if args[1] == "down" {
            assert_eq!(direction, "down");
            assert_eq!(target.as_deref(), Some("\"Pane\" nth \"2\""));
        } else {
            assert_eq!(direction, "\"Save\" in \"Billing\"");
            assert_eq!(target, &None);
        }
    }
}

#[test]
fn invalid_qualifier_pairs_do_not_become_names() {
    for tail in [
        vec!["in"],
        vec!["nth", "0"],
        vec!["nth", "-1"],
        vec!["nth", "many"],
        vec!["in", "row", "extra"],
        vec!["around", "row"],
    ] {
        let args = ["tabref", "click", "Edit"].into_iter().chain(tail);
        assert!(Cli::try_parse_from(args).is_err());
    }
    let error = Cli::try_parse_from(["tabref", "click", "Edit", "in", "row", "extra"]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("quote the whole target: tabref click '\"Edit in row extra\"'"),
        "{error}"
    );
    let error = Cli::try_parse_from(["tabref", "click", "Edit", "nth"]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("quote the whole target: tabref click '\"Edit\" nth \"2\"'"),
        "{error}"
    );
}

#[test]
fn required_targets_and_values_still_fail_when_missing() {
    for args in [
        vec!["click"],
        vec!["fill", "Email"],
        vec!["type", "Email"],
        vec!["select", "Size"],
    ] {
        assert!(Cli::try_parse_from(std::iter::once("tabref").chain(args)).is_err());
    }
    for verb in ["fill", "type", "select"] {
        let cli = parse(&[verb, "Email", ""]);
        let value = match &cli.command {
            Command::Fill { text, .. } | Command::Type { text, .. } => text,
            Command::Select { option, .. } => option,
            _ => panic!("wrong command"),
        };
        assert!(value.is_empty());
    }
}
