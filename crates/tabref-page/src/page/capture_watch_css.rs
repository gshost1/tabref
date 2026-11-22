use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[path = "capture_watch_css_tokens.rs"]
mod lexer;
use lexer::tokens;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Cdo,
    Cdc,
    Word(String),
    Quoted(String),
    Url(String),
    Mark(char),
}

type Parsed = Option<Vec<String>>;
type Cache = HashMap<(String, usize), (String, Parsed)>;
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn cached<'a>(entries: &'a Cache, key: &(String, usize), text: &str) -> Option<&'a Parsed> {
    entries
        .get(key)
        .filter(|(old, _)| old == text)
        .or_else(|| entries.values().find(|(old, _)| old == text))
        .map(|(_, parsed)| parsed)
}

pub(super) fn reject_reason(text: &str) -> String {
    if text.len() > 1_000_000 {
        return "size-limit".into();
    }
    let Some(tokens) = tokens(text) else {
        return "tokenization-or-token-limit".into();
    };
    for (i, pair) in tokens.windows(2).enumerate() {
        let name = word(&pair[1]).unwrap_or_default();
        if pair[0] == Token::Mark('@') {
            if ![
                "charset",
                "namespace",
                "layer",
                "import",
                "media",
                "supports",
                "font-face",
            ]
            .contains(&name.as_str())
            {
                return format!("at-rule:{name}");
            }
            if name == "media" {
                let prelude = &tokens[i + 2..];
                let end = prelude
                    .iter()
                    .position(|t| *t == Token::Mark('{'))
                    .unwrap_or(prelude.len());
                if let Some(token) = prelude[..end]
                    .iter()
                    .find(|t| !media(std::slice::from_ref(t)))
                {
                    return format!("media:{token:?}");
                }
            }
        }
        if pair[0] == Token::Mark(':') && name == "has" {
            return "selector:has".into();
        }
    }
    "declaration-syntax-or-rule-limit".into()
}

pub(super) fn classify(url: &str, text: &str) -> Parsed {
    if text.len() > 1_000_000 {
        return None;
    }
    let key = (url.to_owned(), text.len());
    let cache = CACHE.get_or_init(Mutex::default);
    if let Ok(entries) = cache.lock() {
        if let Some(parsed) = cached(&entries, &key, text) {
            return parsed.clone();
        }
    }
    let parsed = tokens(text).and_then(|tokens| rules(&tokens, 0, &mut 10_000));
    if let Ok(mut entries) = cache.lock() {
        cache_insert(&mut entries, key, text.to_owned(), parsed.clone());
    }
    parsed
}

fn cache_insert(entries: &mut Cache, key: (String, usize), text: String, parsed: Parsed) {
    let bytes = |key: &str, text: &str, parsed: &Parsed| {
        key.len() + text.len() + parsed.iter().flatten().map(String::len).sum::<usize>()
    };
    let incoming = bytes(&key.0, &text, &parsed);
    entries.remove(&key);
    if incoming > 8 * 1024 * 1024 {
        return;
    }
    let retained: usize = entries
        .iter()
        .map(|((key, _), (text, parsed))| bytes(key, text, parsed))
        .sum();
    if entries.len() >= 64 || retained + incoming > 8 * 1024 * 1024 {
        entries.clear();
    }
    entries.insert(key, (text, parsed));
}

fn word(token: &Token) -> Option<String> {
    if let Token::Word(value) = token {
        Some(value.to_ascii_lowercase())
    } else {
        None
    }
}

fn rules(tokens: &[Token], depth: usize, budget: &mut usize) -> Parsed {
    if depth > 16 {
        return None;
    }
    let mut i = 0;
    let mut imports = Vec::new();
    while i < tokens.len() {
        if depth == 0 && matches!(tokens[i], Token::Cdo | Token::Cdc) {
            i += 1;
            continue;
        }
        *budget = budget.checked_sub(1)?;
        let at = tokens.get(i) == Some(&Token::Mark('@'));
        let name = if at {
            i += 1;
            let name = word(tokens.get(i)?)?;
            i += 1;
            name
        } else {
            String::new()
        };
        let start = i;
        let mut stack = Vec::new();
        while let Some(token) = tokens.get(i) {
            match token {
                Token::Mark('(') => stack.push(')'),
                Token::Mark('[') => stack.push(']'),
                Token::Mark(c @ (')' | ']')) => {
                    if stack.pop() != Some(*c) {
                        return None;
                    }
                }
                Token::Mark(';' | '{') if stack.is_empty() => break,
                Token::Mark('{' | '}' | ';' | '@') | Token::Cdo | Token::Cdc => return None,
                _ => {}
            }
            i += 1;
        }
        if !stack.is_empty() {
            return None;
        }
        let prelude = &tokens[start..i];
        let end = tokens.get(i)?;
        i += 1;
        if end == &Token::Mark(';') {
            match name.as_str() {
                "charset" if matches!(prelude, [Token::Quoted(_)]) => {}
                "namespace" | "layer" if !prelude.is_empty() => {}
                "import" => imports.push(import(prelude)?),
                _ => return None,
            }
            continue;
        }
        let start = i;
        let mut braces = 1;
        while braces > 0 {
            match tokens.get(i)? {
                Token::Mark('{') => braces += 1,
                Token::Mark('}') => braces -= 1,
                _ => {}
            }
            i += 1;
        }
        let body = &tokens[start..i - 1];
        match name.as_str() {
            "media" if media(prelude) => imports.extend(rules(body, depth + 1, budget)?),
            "supports" if !prelude.is_empty() => imports.extend(rules(body, depth + 1, budget)?),
            "layer" => imports.extend(rules(body, depth + 1, budget)?),
            "font-face" if prelude.is_empty() => declarations(body)?,
            "" if !prelude.is_empty() => {
                if prelude.windows(2).any(|pair| {
                    pair[0] == Token::Mark(':') && word(&pair[1]).as_deref() == Some("has")
                }) {
                    return None;
                }
                declarations(body)?;
            }
            _ => return None,
        }
    }
    Some(imports)
}

fn declarations(tokens: &[Token]) -> Option<()> {
    let mut start = 0;
    let mut stack = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if matches!(
            token,
            Token::Mark('@' | '{' | '}') | Token::Cdo | Token::Cdc
        ) {
            return None;
        }
        match token {
            Token::Mark('(') => stack.push(')'),
            Token::Mark('[') => stack.push(']'),
            Token::Mark(c @ (')' | ']')) if stack.pop() != Some(*c) => return None,
            Token::Mark(';') if stack.is_empty() => {
                declaration(&tokens[start..i])?;
                start = i + 1;
            }
            _ => {}
        }
        if let Some(name) = word(token) {
            if ["animation-timeline", "scroll-timeline", "view-timeline"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
                || name == "timeline-scope"
            {
                return None;
            }
        }
    }
    declaration(&tokens[start..])?;
    stack.is_empty().then_some(())
}

fn declaration(tokens: &[Token]) -> Option<()> {
    if tokens.is_empty() {
        return Some(());
    }
    let [Token::Word(property), Token::Mark(':'), values @ ..] = tokens else {
        return None;
    };
    if property.starts_with(|c: char| c.is_ascii_digit()) || values.is_empty() {
        return None;
    }
    let mut stack = Vec::new();
    for (i, token) in values.iter().enumerate() {
        match token {
            Token::Mark('(') => stack.push(')'),
            Token::Mark('[') => stack.push(']'),
            Token::Mark(c @ (')' | ']')) if stack.pop() != Some(*c) => return None,
            Token::Mark(':') if stack.is_empty() => return None,
            Token::Mark('!') if stack.is_empty() => {
                if i == 0
                    || !matches!(&values[i + 1..], [Token::Word(word)] if word.eq_ignore_ascii_case("important"))
                {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(())
}

fn import(tokens: &[Token]) -> Option<String> {
    let (url, rest) = match tokens.first()? {
        Token::Quoted(url) | Token::Url(url) => (url.clone(), &tokens[1..]),
        Token::Word(name)
            if name.eq_ignore_ascii_case("url") && tokens.get(1) == Some(&Token::Mark('(')) =>
        {
            let end = tokens.iter().position(|token| token == &Token::Mark(')'))?;
            let inside = &tokens[2..end];
            let url = if let [Token::Quoted(url)] = inside {
                url.clone()
            } else {
                let mut url = String::new();
                for token in inside {
                    match token {
                        Token::Word(value) => url.push_str(value),
                        Token::Mark(c) if !matches!(c, '(' | ')' | '\'' | '"' | '@') => {
                            url.push(*c)
                        }
                        _ => return None,
                    }
                }
                url
            };
            (url, &tokens[end + 1..])
        }
        _ => return None,
    };
    if url.is_empty() || (!rest.is_empty() && !media(rest)) {
        return None;
    }
    Some(url)
}

fn media(tokens: &[Token]) -> bool {
    !tokens.is_empty()
        && tokens.iter().all(|token| match token {
            Token::Mark(c) => matches!(
                c,
                '(' | ')' | ':' | ',' | '.' | '/' | '<' | '>' | '=' | '+' | '-'
            ),
            Token::Word(value) => {
                let value = value.to_ascii_lowercase();
                [
                    "all",
                    "screen",
                    "print",
                    "not",
                    "only",
                    "and",
                    "or",
                    "prefers-color-scheme",
                    "dark",
                    "light",
                    "prefers-reduced-motion",
                    "reduce",
                    "no-preference",
                    "prefers-contrast",
                    "more",
                    "less",
                    "custom",
                    "forced-colors",
                    "active",
                    "none",
                    "orientation",
                    "portrait",
                    "landscape",
                    "width",
                    "height",
                    "aspect-ratio",
                    "min-width",
                    "max-width",
                    "min-height",
                    "max-height",
                    "min-aspect-ratio",
                    "max-aspect-ratio",
                ]
                .contains(&value.as_str())
                    || dimension(&value)
            }
            _ => false,
        })
}

fn dimension(value: &str) -> bool {
    let split = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    split > 0
        && value[..split].parse::<f64>().is_ok()
        && ["", "px", "em", "rem", "vw", "vh", "vmin", "vmax"].contains(&&value[split..])
}

#[cfg(test)]
#[path = "capture_watch_css_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "capture_watch_css_structure.rs"]
pub(super) mod structure;
