use super::{Token, classify, tokens};

pub(crate) fn structure(text: &str) -> Option<Vec<String>> {
    classify("differential", text)?;
    let tokens = tokens(text)?;
    let mut names = Vec::new();
    let mut depth = 0;
    let mut beginning = true;
    for (i, token) in tokens.iter().enumerate() {
        if beginning && matches!(token, Token::Cdo | Token::Cdc) {
            continue;
        }
        if beginning {
            let name = if *token == Token::Mark('@') {
                super::word(tokens.get(i + 1)?)?
            } else {
                "style".into()
            };
            if name != "charset" {
                names.push(name);
            }
            beginning = false;
        }
        match token {
            Token::Mark('{') => depth += 1,
            Token::Mark('}') => {
                depth -= 1;
                beginning = depth == 0;
            }
            Token::Mark(';') if depth == 0 => beginning = true,
            _ => {}
        }
    }
    Some(names)
}
