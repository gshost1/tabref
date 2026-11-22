use super::{Token, classify, tokens};

pub(crate) fn tree(text: &str) -> Option<Vec<String>> {
    classify("differential-tree", text)?;
    let tokens = tokens(text)?;
    let mut result = Vec::new();
    walk(&tokens, "", &mut result)?;
    Some(result)
}

fn walk(tokens: &[Token], parent: &str, result: &mut Vec<String>) -> Option<()> {
    let mut i = 0;
    let mut index = 0;
    while i < tokens.len() {
        if matches!(tokens[i], Token::Cdo | Token::Cdc) {
            i += 1;
            continue;
        }
        let kind = if tokens[i] == Token::Mark('@') {
            super::word(tokens.get(i + 1)?)?
        } else {
            "style".into()
        };
        let mut parentheses = 0;
        while let Some(token) = tokens.get(i) {
            match token {
                Token::Mark('(' | '[') => parentheses += 1,
                Token::Mark(')' | ']') => parentheses -= 1,
                Token::Mark('{' | ';') if parentheses == 0 => break,
                _ => {}
            }
            i += 1;
        }
        let block = tokens.get(i)? == &Token::Mark('{');
        i += 1;
        let path = if parent.is_empty() {
            index.to_string()
        } else {
            format!("{parent}/{index}")
        };
        if kind != "charset" {
            let class = match kind.as_str() {
                "style" => "CSSStyleRule",
                "import" => "CSSImportRule",
                "namespace" => "CSSNamespaceRule",
                "media" => "CSSMediaRule",
                "supports" => "CSSSupportsRule",
                "layer" if block => "CSSLayerBlockRule",
                "layer" => "CSSLayerStatementRule",
                "font-face" => "CSSFontFaceRule",
                _ => return None,
            };
            result.push(format!("{path}:{class}"));
            index += 1;
        }
        if block {
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
            if ["media", "supports", "layer"].contains(&kind.as_str()) {
                walk(&tokens[start..i - 1], &path, result)?;
            }
        }
    }
    Some(())
}

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
