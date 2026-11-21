#![allow(clippy::unwrap_used, clippy::panic)]

use clap::Parser;
use tabref_page::Locator;

use super::{Cli, command::Command};
use crate::commands::target_spec::TargetSpec;

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(std::iter::once("tabref").chain(args.iter().copied())).unwrap()
}

fn locator(target: &str) -> Locator {
    let TargetSpec::Find(locator) = TargetSpec::from_arg(target).unwrap() else {
        panic!("not a locator")
    };
    locator
}

#[test]
fn value_hints_preserve_the_last_argument_even_when_it_is_a_keyword() {
    for verb in ["fill", "type", "select"] {
        for (words, hint) in [
            (
                vec![verb, "Search", "nth", "2"],
                format!("tabref {verb} 'Search nth' '2'"),
            ),
            (
                vec![verb, "Email", "in", "Billing", "extra", "a@b.c"],
                format!("tabref {verb} 'Email in Billing extra' 'a@b.c'"),
            ),
        ] {
            let error = Cli::try_parse_from(std::iter::once("tabref").chain(words)).unwrap_err();
            assert!(error.to_string().contains(&hint), "{error}");
        }
        let cli = parse(&[verb, "Search", "in"]);
        let (target, value) = match &cli.command {
            Command::Fill { target, text } | Command::Type { target, text } => (target, text),
            Command::Select { target, option } => (target, option),
            _ => panic!("wrong command"),
        };
        assert_eq!((target.as_str(), value.as_str()), ("Search", "in"));
    }
}

#[test]
fn hints_do_not_invent_operands_or_lose_scroll_arguments() {
    for (args, hint) in [
        (vec!["click", "Sign", "in"], "tabref click 'Sign in'"),
        (vec!["scroll", "e5", "e7"], "tabref scroll 'e5 e7'"),
        (
            vec!["scroll", "down", "in", "Results", "extra"],
            "tabref scroll down 'Results extra'",
        ),
    ] {
        let error = Cli::try_parse_from(std::iter::once("tabref").chain(args)).unwrap_err();
        assert!(error.to_string().contains(hint), "{error}");
        assert!(!error.to_string().contains("row"), "{error}");
    }
}

#[test]
fn scroll_in_names_the_container_as_in_a_step() {
    let cli = parse(&["scroll", "down", "in", "Results"]);
    let Command::Scroll {
        direction, target, ..
    } = &cli.command
    else {
        panic!("wrong command")
    };
    assert_eq!(direction, "down");
    assert_eq!(target.as_deref(), Some("Results"));
}

#[test]
fn role_words_keep_step_semantics_before_names_and_qualifiers() {
    for role in ["button", "link", "textbox", "checkbox", "row", "heading"] {
        for (words, name) in [
            (vec![role, "nth", "3"], None),
            (vec![role, "Save", "nth", "3"], Some("Save")),
        ] {
            let cli = parse(&[&["click"], words.as_slice()].concat());
            let Command::Click { target } = &cli.command else {
                panic!("wrong command")
            };
            let found = locator(target);
            assert_eq!(found.role.as_deref(), Some(role));
            assert_eq!(found.name.as_deref(), name);
            assert_eq!(found.nth, Some(3));
        }
    }
}

#[test]
fn flags_can_interrupt_target_words_and_negative_nth_gets_a_hint() {
    let cli = parse(&["click", "Edit", "--timeout", "5", "in", "row"]);
    assert_eq!(cli.timeout, Some(5));
    let Command::Click { target } = &cli.command else {
        panic!("wrong command")
    };
    assert_eq!(locator(target).scope.as_deref(), Some("row"));
    let cli = parse(&["screenshot", "Chart", "--out", "a.png", "nth", "2"]);
    let Command::Screenshot { target, out } = &cli.command else {
        panic!("wrong command")
    };
    assert_eq!(locator(target.as_deref().unwrap()).nth, Some(2));
    assert_eq!(out.as_deref(), Some(std::path::Path::new("a.png")));
    let error = Cli::try_parse_from(["tabref", "click", "Edit", "nth", "-1"]).unwrap_err();
    assert!(error.to_string().contains("nth NUMBER from 1"), "{error}");
    assert!(
        error.to_string().contains("quote the whole target"),
        "{error}"
    );
}

#[test]
fn upload_paths_are_never_consumed_as_qualifiers() {
    let cli = parse(&["upload", "Photo", "after", "b.png", "c.png"]);
    let Command::Upload { target, files } = &cli.command else {
        panic!("wrong command")
    };
    assert_eq!(target, "Photo");
    assert_eq!(
        files,
        &["after", "b.png", "c.png"].map(std::path::PathBuf::from)
    );
}

#[test]
fn apostrophes_in_names_are_literals_with_or_without_qualifiers() {
    for args in [
        vec!["click", "Don't save"],
        vec!["click", "Don't save", "nth", "2"],
    ] {
        let cli = parse(&args);
        let Command::Click { target } = &cli.command else {
            panic!("wrong command")
        };
        assert_eq!(locator(target).name.as_deref(), Some("Don't save"));
    }
}

#[test]
fn empty_names_and_scopes_are_usage_errors() {
    for args in [
        vec!["click", ""],
        vec!["click", "   "],
        vec!["click", "\"\""],
        vec!["click", "Edit", "in", ""],
        vec!["click", "Edit", "in", "  "],
        vec!["click", "", "nth", "2"],
        vec!["click", "\"Edit\" in \"\""],
    ] {
        let error = Cli::try_parse_from(std::iter::once("tabref").chain(args)).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("must not be empty"), "{error}");
    }
}
