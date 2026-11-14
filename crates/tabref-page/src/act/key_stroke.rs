use super::key_definition::find_key;

const ALT: u32 = 1;
const CONTROL: u32 = 2;
const META: u32 = 4;
const SHIFT: u32 = 8;

/// One key press as `Input.dispatchKeyEvent` takes it: the DOM `key` and
/// `code`, the Windows virtual key code, the text it types, if any, and the
/// CDP modifier bits held while it is pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyStroke {
    pub(crate) key: String,
    pub(crate) code: String,
    pub(crate) key_code: u32,
    pub(crate) text: Option<String>,
    pub(crate) modifiers: u32,
}

impl KeyStroke {
    /// Parses a key name (`Enter`, `esc`, `ArrowDown`), a single character
    /// (`a`, `7`, `/`), or either after modifiers joined by `+`
    /// (`Control+a`, `Shift+Tab`, `Meta+Enter`). A key pressed with Alt,
    /// Control or Meta types no text.
    pub(crate) fn parse(combo: &str) -> Option<Self> {
        let combo = combo.trim();
        let (modifier_part, key_part) = if combo == "+" {
            ("", "+")
        } else if let Some(modifiers) = combo.strip_suffix("++") {
            (modifiers, "+")
        } else {
            combo.rsplit_once('+').unwrap_or(("", combo))
        };
        let mut modifiers = 0;
        for name in modifier_part.split('+').filter(|name| !name.is_empty()) {
            modifiers |= modifier_bit(name)?;
        }
        let mut stroke = named(key_part).or_else(|| character(key_part, modifiers))?;
        stroke.modifiers = modifiers;
        if modifiers & (ALT | CONTROL | META) != 0 {
            stroke.text = None;
        }
        Some(stroke)
    }
}

fn modifier_bit(name: &str) -> Option<u32> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "alt" | "option" => ALT,
        "control" | "ctrl" => CONTROL,
        "meta" | "cmd" | "command" | "super" => META,
        "shift" => SHIFT,
        _ => return None,
    })
}

fn named(name: &str) -> Option<KeyStroke> {
    let definition = find_key(name)?;
    Some(KeyStroke {
        key: definition.key.to_owned(),
        code: definition.code.to_owned(),
        key_code: definition.key_code,
        text: definition.text.map(str::to_owned),
        modifiers: 0,
    })
}

fn character(text: &str, modifiers: u32) -> Option<KeyStroke> {
    let mut chars = text.chars();
    let (Some(single), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if single.is_control() {
        return None;
    }
    let shifted = modifiers & SHIFT != 0;
    let typed = if shifted {
        single.to_uppercase().collect::<String>()
    } else {
        single.to_string()
    };
    let upper = single.to_ascii_uppercase();
    let (code, key_code) = if single.is_ascii_alphabetic() {
        (format!("Key{upper}"), u32::from(upper))
    } else if single.is_ascii_digit() {
        (format!("Digit{single}"), u32::from(single))
    } else {
        (String::new(), 0)
    };
    Some(KeyStroke {
        key: typed.clone(),
        code,
        key_code,
        text: Some(typed),
        modifiers: 0,
    })
}

#[cfg(test)]
#[path = "key_stroke_tests.rs"]
mod tests;
