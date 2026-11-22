use super::Token;

pub(super) fn tokens(text: &str) -> Option<Vec<Token>> {
    let mut input = Vec::with_capacity(text.len());
    let mut raw = text.chars().peekable();
    while let Some(c) = raw.next() {
        input.push(match c {
            '\r' => {
                if raw.peek() == Some(&'\n') {
                    raw.next();
                }
                '\n'
            }
            '\u{000c}' => '\n',
            '\0' => '\u{fffd}',
            _ => c,
        });
    }
    let mut lexer = Lexer { input, pos: 0 };
    let mut result = Vec::new();
    while let Some(c) = lexer.peek(0) {
        if space(c) {
            lexer.pos += 1;
            continue;
        }
        if c == '/' && lexer.peek(1) == Some('*') {
            lexer.pos += 2;
            while !(lexer.peek(0) == Some('*') && lexer.peek(1) == Some('/')) {
                lexer.peek(0)?;
                lexer.pos += 1;
            }
            lexer.pos += 2;
            if matches!(result.last(), Some(Token::Word(_)))
                && (lexer.ident_start() || lexer.peek(0).is_some_and(|c| c.is_ascii_digit()))
            {
                return None;
            }
            continue;
        }
        if lexer.rest("<!--") {
            lexer.pos += 4;
            result.push(Token::Cdo);
        } else if lexer.rest("-->") {
            lexer.pos += 3;
            result.push(Token::Cdc);
        } else if c == '\'' || c == '"' {
            result.push(Token::Quoted(lexer.string()?));
        } else if c == '@' || c == '#' {
            lexer.pos += 1;
            if !(lexer.ident_start() || (c == '#' && lexer.peek(0).is_some_and(name))) {
                return None;
            }
            result.push(Token::Mark(c));
            result.push(Token::Word(lexer.name()?));
        } else if lexer.number_start() {
            result.push(Token::Word(lexer.number()?));
        } else if lexer.ident_start() {
            let name = lexer.name()?;
            if lexer.peek(0) == Some('(') {
                lexer.pos += 1;
                if name.eq_ignore_ascii_case("url") {
                    while lexer.peek(0).is_some_and(space) {
                        lexer.pos += 1;
                    }
                    if !matches!(lexer.peek(0), Some('\'' | '"')) {
                        result.push(Token::Url(lexer.url()?));
                        if result.len() > 100_000 {
                            return None;
                        }
                        continue;
                    }
                }
                result.push(Token::Word(name));
                result.push(Token::Mark('('));
            } else {
                result.push(Token::Word(name));
            }
        } else if c == '\\' {
            return None;
        } else {
            lexer.pos += 1;
            result.push(Token::Mark(c));
        }
        if result.len() > 100_000 {
            return None;
        }
    }
    Some(result)
}

struct Lexer {
    input: Vec<char>,
    pos: usize,
}

fn space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n')
}
fn start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}
fn name(c: char) -> bool {
    start(c) || c.is_ascii_digit() || c == '-'
}

impl Lexer {
    fn peek(&self, n: usize) -> Option<char> {
        self.input.get(self.pos + n).copied()
    }
    fn rest(&self, value: &str) -> bool {
        value
            .chars()
            .enumerate()
            .all(|(n, c)| self.peek(n) == Some(c))
    }
    fn escape_at(&self, n: usize) -> bool {
        self.peek(n) == Some('\\') && self.peek(n + 1).is_some_and(|c| c != '\n')
    }
    fn ident_start(&self) -> bool {
        match self.peek(0) {
            Some('-') => self.peek(1).is_some_and(|c| start(c) || c == '-') || self.escape_at(1),
            Some(c) if start(c) => true,
            _ => self.escape_at(0),
        }
    }
    fn escaped(&mut self) -> Option<char> {
        if !self.escape_at(0) {
            return None;
        }
        self.pos += 1;
        let first = self.peek(0)?;
        if !first.is_ascii_hexdigit() {
            self.pos += 1;
            return Some(first);
        }
        let mut value = 0;
        for _ in 0..6 {
            let Some(c) = self.peek(0).filter(char::is_ascii_hexdigit) else {
                break;
            };
            value = value * 16 + c.to_digit(16)?;
            self.pos += 1;
        }
        if self.peek(0).is_some_and(space) {
            self.pos += 1;
        }
        Some(
            char::from_u32(value)
                .filter(|c| *c != '\0')
                .unwrap_or('\u{fffd}'),
        )
    }
    fn name(&mut self) -> Option<String> {
        let mut value = String::new();
        loop {
            match self.peek(0) {
                Some(c) if name(c) => {
                    self.pos += 1;
                    value.push(c);
                }
                Some('\\') if self.escape_at(0) => value.push(self.escaped()?),
                _ => return Some(value),
            }
        }
    }
    fn string(&mut self) -> Option<String> {
        let quote = self.peek(0)?;
        self.pos += 1;
        let mut value = String::new();
        loop {
            match self.peek(0)? {
                c if c == quote => {
                    self.pos += 1;
                    return Some(value);
                }
                '\n' => return None,
                '\\' if self.peek(1) == Some('\n') => self.pos += 2,
                '\\' => value.push(self.escaped()?),
                c => {
                    self.pos += 1;
                    value.push(c);
                }
            }
        }
    }
    fn bad_url(&mut self) -> Option<String> {
        while let Some(c) = self.peek(0) {
            if c == ')' {
                self.pos += 1;
                break;
            }
            if self.escape_at(0) {
                self.escaped()?;
            } else {
                self.pos += 1;
            }
        }
        None
    }
    fn url(&mut self) -> Option<String> {
        let mut value = String::new();
        loop {
            match self.peek(0)? {
                ')' => {
                    self.pos += 1;
                    return Some(value);
                }
                c if space(c) => {
                    while self.peek(0).is_some_and(space) {
                        self.pos += 1;
                    }
                    if self.peek(0) == Some(')') {
                        self.pos += 1;
                        return Some(value);
                    }
                    return self.bad_url();
                }
                '\''
                | '"'
                | '('
                | '\u{0001}'..='\u{0008}'
                | '\u{000b}'
                | '\u{000e}'..='\u{001f}'
                | '\u{007f}' => return self.bad_url(),
                '\\' if self.escape_at(0) => value.push(self.escaped()?),
                '\\' => return self.bad_url(),
                c => {
                    self.pos += 1;
                    value.push(c);
                }
            }
        }
    }
    fn number_start(&self) -> bool {
        let digit = |n| self.peek(n).is_some_and(|c| c.is_ascii_digit());
        digit(0)
            || (self.peek(0) == Some('.') && digit(1))
            || (matches!(self.peek(0), Some('+' | '-'))
                && (digit(1) || (self.peek(1) == Some('.') && digit(2))))
    }
    fn number(&mut self) -> Option<String> {
        let begin = self.pos;
        if matches!(self.peek(0), Some('+' | '-')) {
            self.pos += 1;
        }
        while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
            while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(0), Some('e' | 'E')) {
            let offset = if matches!(self.peek(1), Some('+' | '-')) {
                2
            } else {
                1
            };
            if self.peek(offset).is_some_and(|c| c.is_ascii_digit()) {
                self.pos += offset;
                while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
        }
        let mut value: String = self.input[begin..self.pos].iter().collect();
        if self.ident_start() {
            value.push_str(&self.name()?);
        } else if self.peek(0) == Some('%') {
            self.pos += 1;
            value.push('%');
        }
        Some(value)
    }
}
