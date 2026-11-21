use clap::{ArgAction, ArgMatches, Command as ClapCommand, Error, error::ErrorKind};
use tabref_page::{Locator, Word};

use super::command::Command;
use crate::commands::target_spec::{TargetSpec, has_target_grammar, looks_like_ref};

const ACTIONS: &[&str] = &[
    "click",
    "double-click",
    "fill",
    "type",
    "select",
    "check",
    "uncheck",
    "press",
    "focus",
    "hover",
    "screenshot",
    "scroll",
];

pub(super) fn command(mut command: ClapCommand) -> ClapCommand {
    for name in ACTIONS {
        command = command.mut_subcommand(name, |sub| {
            let index = if ["press", "scroll"].contains(name) {
                2
            } else {
                1
            };
            let sub = sub.mut_arg("target", |arg| {
                arg.index(index)
                    .num_args(1..)
                    .action(ArgAction::Append)
                    .allow_negative_numbers(true)
            });
            match *name {
                "fill" | "type" => sub.mut_arg("text", |arg| arg.index(2)),
                "select" => sub.mut_arg("option", |arg| arg.index(2)),
                "press" => sub.mut_arg("key", |arg| arg.index(1)),
                "scroll" => sub.mut_arg("direction", |arg| arg.index(1)),
                _ => sub,
            }
        });
    }
    command
}

pub(super) fn target(matches: &ArgMatches) -> Result<Option<String>, Error> {
    let Some((name, args)) = matches.subcommand() else {
        return Ok(None);
    };
    if !ACTIONS.contains(&name) {
        return Ok(None);
    }
    let mut words: Vec<&str> = args
        .get_many::<String>("target")
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let value = match name {
        "fill" | "type" => args.get_one::<String>("text"),
        "select" => args.get_one::<String>("option"),
        _ => None,
    };
    let mut context = Context {
        name,
        value: value.map(String::as_str),
        direction: None,
    };
    if name == "scroll" {
        let direction = args
            .get_one::<String>("direction")
            .map(String::as_str)
            .unwrap_or("down");
        if is_direction(direction) {
            context.direction = Some(direction);
            if words
                .first()
                .is_some_and(|word| word.eq_ignore_ascii_case("in"))
            {
                words.remove(0);
                if words.is_empty() {
                    return Err(context.usage(&["in"], "in takes a non-empty container name"));
                }
            }
        } else {
            words.insert(0, direction);
        }
    }
    normalize(&context, &words)
}

fn is_direction(word: &str) -> bool {
    ["up", "down", "left", "right"].contains(&word.trim().to_ascii_lowercase().as_str())
}

fn keyword(word: &str) -> bool {
    ["in", "after", "before", "nth"]
        .iter()
        .any(|key| word.eq_ignore_ascii_case(key))
}

fn quote(word: &str) -> String {
    let escaped = word.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn target_word(word: &str) -> String {
    if has_target_grammar(word) {
        word.to_owned()
    } else {
        quote(word)
    }
}

struct Context<'a> {
    name: &'a str,
    value: Option<&'a str>,
    direction: Option<&'a str>,
}

impl Context<'_> {
    fn usage(&self, words: &[&str], reason: &str) -> Error {
        if self.name == "scroll"
            && self.direction.is_none()
            && words.first().is_some_and(|word| looks_like_ref(word))
            && words.get(1).is_some_and(|word| looks_like_ref(word))
        {
            return Error::raw(
                ErrorKind::ValueValidation,
                format!("scroll takes one target: tabref scroll {}", words[0]),
            );
        }
        if let Some(hint) = self.value_hint(words) {
            return Error::raw(
                ErrorKind::ValueValidation,
                format!("extra value words must be one argument; quote the value: {hint}"),
            );
        }
        let mut command = format!("tabref {}", self.name);
        if let Some(direction) = self.direction {
            command.push_str(&format!(" {direction}"));
        }
        command.push_str(&format!(" {}", shell_quote(&words.join(" "))));
        if let Some(value) = self.value {
            command.push_str(&format!(" {}", shell_quote(value)));
        }
        Error::raw(
            ErrorKind::ValueValidation,
            format!("{reason}; quote the whole target: {command}"),
        )
    }

    fn value_hint(&self, words: &[&str]) -> Option<String> {
        let value = self.value?;
        let target_context = Context {
            name: self.name,
            value: None,
            direction: None,
        };
        for end in (1..words.len()).rev() {
            if keyword(words[end]) {
                continue;
            }
            let Ok(Some(target)) = normalize(&target_context, &words[..end]) else {
                continue;
            };
            let target = if end == 1
                && target
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-@:.".contains(c))
            {
                target
            } else {
                shell_quote(&target)
            };
            let mut value_words = words[end..].to_vec();
            value_words.push(value);
            return Some(format!(
                "tabref {} {target} {}",
                self.name,
                shell_quote(&value_words.join(" "))
            ));
        }
        None
    }
}

fn shell_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

fn normalize(context: &Context<'_>, words: &[&str]) -> Result<Option<String>, Error> {
    if words.len() <= 1 {
        return words
            .first()
            .map(|word| validate(context, words, (*word).to_owned()))
            .transpose();
    }
    let role = Locator::from_words(&[Word::bare(words[0])])
        .ok()
        .is_some_and(|locator| locator.role.is_some());
    let mut parts = vec![if role {
        words[0].to_owned()
    } else {
        target_word(words[0])
    }];
    let start = if role && !keyword(words[1]) {
        parts.push(quote(words[1]));
        2
    } else {
        1
    };
    let mut pairs = words[start..].chunks_exact(2);
    for pair in &mut pairs {
        if !keyword(pair[0])
            || (pair[0].eq_ignore_ascii_case("nth")
                && !pair[1].parse::<usize>().is_ok_and(|n| n > 0))
        {
            return Err(context.usage(
                words,
                "expected qualifier pairs (in/after/before TEXT or nth NUMBER from 1)",
            ));
        }
        parts.push(pair[0].to_owned());
        parts.push(quote(pair[1]));
    }
    if !pairs.remainder().is_empty() {
        return Err(context.usage(
            words,
            "a qualifier needs an operand; otherwise join the words as one name",
        ));
    }
    validate(context, words, parts.join(" ")).map(Some)
}

fn validate(context: &Context<'_>, words: &[&str], target: String) -> Result<String, Error> {
    let spec =
        TargetSpec::from_arg(&target).map_err(|error| context.usage(words, &error.message))?;
    if let TargetSpec::Find(locator) = spec
        && (locator
            .name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
            || locator
                .scope
                .as_deref()
                .is_some_and(|scope| scope.trim().is_empty()))
    {
        return Err(context.usage(words, "target names and scopes must not be empty"));
    }
    Ok(target)
}

pub(super) fn apply(command: &mut Command, normalized: Option<String>) -> Result<(), Error> {
    match command {
        Command::Click { target }
        | Command::DoubleClick { target }
        | Command::Fill { target, .. }
        | Command::Type { target, .. }
        | Command::Select { target, .. }
        | Command::Check { target }
        | Command::Uncheck { target }
        | Command::Focus { target }
        | Command::Hover { target } => {
            if let Some(normalized) = normalized {
                *target = normalized;
            }
        }
        Command::Press { target, .. } | Command::Screenshot { target, .. } => *target = normalized,
        Command::Scroll {
            direction, target, ..
        } => {
            if ["up", "down", "left", "right"]
                .contains(&direction.trim().to_ascii_lowercase().as_str())
            {
                *target = normalized;
            } else if let Some(normalized) = normalized {
                *direction = normalized;
                *target = None;
            }
        }
        _ => {}
    }
    Ok(())
}
