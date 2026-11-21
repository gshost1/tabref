use clap::{ArgMatches, Command as ClapCommand, Error, error::ErrorKind};

use super::command::Command;

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
            let sub = sub.mut_arg("target", |arg| arg.index(index).num_args(1..));
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
    if name == "scroll" {
        let direction = args
            .get_one::<String>("direction")
            .map(String::as_str)
            .unwrap_or("down");
        if !["up", "down", "left", "right"]
            .contains(&direction.trim().to_ascii_lowercase().as_str())
        {
            words.insert(0, direction);
        }
    }
    normalize(name, &words)
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

fn usage(name: &str, words: &[&str]) -> Error {
    let target = if words.len() > 1 && words[1..].iter().step_by(2).all(|word| keyword(word)) {
        let mut parts = vec![quote(words[0])];
        for pair in words[1..].chunks(2) {
            let operand = pair.get(1).copied().unwrap_or("row");
            let operand = if pair[0].eq_ignore_ascii_case("nth")
                && !operand.parse::<usize>().is_ok_and(|n| n > 0)
            {
                "2"
            } else {
                operand
            };
            parts.extend([pair[0].to_owned(), quote(operand)]);
        }
        parts.join(" ")
    } else {
        quote(&words.join(" "))
    };
    let target = target.replace('\'', "'\\''");
    Error::raw(
        ErrorKind::ValueValidation,
        format!(
            "extra target words must be qualifier pairs (in/after/before TEXT or nth NUMBER); quote the whole target: tabref {name} '{target}'"
        ),
    )
}

fn normalize(name: &str, words: &[&str]) -> Result<Option<String>, Error> {
    if words.len() <= 1 {
        return Ok(words.first().map(|word| (*word).to_owned()));
    }
    let mut parts = vec![quote(words[0])];
    let mut pairs = words[1..].chunks_exact(2);
    for pair in &mut pairs {
        if !keyword(pair[0])
            || (pair[0].eq_ignore_ascii_case("nth")
                && !pair[1].parse::<usize>().is_ok_and(|n| n > 0))
        {
            return Err(usage(name, words));
        }
        parts.push(pair[0].to_owned());
        parts.push(quote(pair[1]));
    }
    if !pairs.remainder().is_empty() {
        return Err(usage(name, words));
    }
    Ok(Some(parts.join(" ")))
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
        Command::Upload { target, files } => {
            let mut count = 0;
            while count + 2 < files.len()
                && files
                    .get(count)
                    .and_then(|path| path.to_str())
                    .is_some_and(keyword)
            {
                count += 2;
            }
            if count > 0 {
                let mut words = vec![target.as_str()];
                words.extend(files.iter().take(count).filter_map(|path| path.to_str()));
                if let Some(normalized) = normalize("upload", &words)? {
                    *target = normalized;
                }
                files.drain(..count);
            }
        }
        _ => {}
    }
    Ok(())
}
