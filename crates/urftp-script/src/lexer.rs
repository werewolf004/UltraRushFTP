//! Lexical analysis of Pascal Script (case-insensitive keywords).

use crate::ScriptError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Int(i64),
    Str(String),
    /// Symbol or operator: `:=`, `;`, `(`, `<=`...
    Sym(&'static str),
    /// Keyword (lower case): `begin`, `if`, `and`...
    Kw(&'static str),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
}

const KEYWORDS: &[&str] = &[
    "and",
    "begin",
    "break",
    "case",
    "continue",
    "in",
    "of",
    "div",
    "do",
    "downto",
    "else",
    "end",
    "exit",
    "false",
    "for",
    "if",
    "mod",
    "nil",
    "not",
    "or",
    "repeat",
    "then",
    "to",
    "true",
    "until",
    "uses",
    "var",
    "while",
    "xor",
    "const",
    "procedure",
    "function",
];

const SYMBOLS: &[&str] = &[":=", "<=", ">=", "<>", ";", ",", ".", "(", ")", "[", "]", "+", "-", "*", "/", "=", "<", ">", ":"];

pub fn tokenize(src: &str) -> Result<Vec<Token>, ScriptError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line) = (0, 1);
    let err = |line: usize, m: String| ScriptError::syntax(line, m);
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '{' => {
                let start = line;
                while i < chars.len() && chars[i] != '}' {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(err(start, "unclosed { comment".into()));
                }
                i += 1;
            }
            '(' if chars.get(i + 1) == Some(&'*') => {
                let start = line;
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == ')') {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                if i + 1 >= chars.len() {
                    return Err(err(start, "unclosed (* comment".into()));
                }
                i += 2;
            }
            '\'' | '#' => {
                // String: 'abc' with '' for an apostrophe, and #65 for a character by code;
                // both can be chained: 'a'#13#10'b'.
                let mut s = String::new();
                while i < chars.len() && (chars[i] == '\'' || chars[i] == '#') {
                    if chars[i] == '#' {
                        i += 1;
                        let start = i;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                        let code: u32 =
                            chars[start..i].iter().collect::<String>().parse().map_err(|_| err(line, "invalid # character code".into()))?;
                        s.push(char::from_u32(code).ok_or_else(|| err(line, format!("invalid character #{code}")))?);
                        continue;
                    }
                    i += 1;
                    loop {
                        match chars.get(i) {
                            None | Some('\n') => return Err(err(line, "unclosed string".into())),
                            Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                                s.push('\'');
                                i += 2;
                            }
                            Some('\'') => {
                                i += 1;
                                break;
                            }
                            Some(&ch) => {
                                s.push(ch);
                                i += 1;
                            }
                        }
                    }
                }
                out.push(Token { tok: Tok::Str(s), line });
            }
            '$' if chars.get(i + 1).is_some_and(char::is_ascii_hexdigit) => {
                let start = i + 1;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let hex: String = chars[start..i].iter().collect();
                let v = i64::from_str_radix(&hex, 16).map_err(|_| err(line, format!("number ${hex} too large")))?;
                out.push(Token { tok: Tok::Int(v), line });
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let v = text.parse().map_err(|_| err(line, format!("number {text} too large")))?;
                out.push(Token { tok: Tok::Int(v), line });
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let lower = word.to_ascii_lowercase();
                let tok = match KEYWORDS.iter().find(|k| **k == lower) {
                    Some(k) => Tok::Kw(k),
                    None => Tok::Ident(word),
                };
                out.push(Token { tok, line });
            }
            _ => {
                let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
                let sym =
                    SYMBOLS.iter().find(|s| rest.starts_with(**s)).ok_or_else(|| err(line, format!("unexpected character \"{c}\"")))?;
                i += sym.chars().count();
                out.push(Token { tok: Tok::Sym(sym), line });
            }
        }
    }
    out.push(Token { tok: Tok::Eof, line });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        tokenize(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn basics() {
        assert_eq!(
            toks("RushApp.FTP.Login('a''b', $1F, 12); // fin"),
            vec![
                Tok::Ident("RushApp".into()),
                Tok::Sym("."),
                Tok::Ident("FTP".into()),
                Tok::Sym("."),
                Tok::Ident("Login".into()),
                Tok::Sym("("),
                Tok::Str("a'b".into()),
                Tok::Sym(","),
                Tok::Int(31),
                Tok::Sym(","),
                Tok::Int(12),
                Tok::Sym(")"),
                Tok::Sym(";"),
                Tok::Eof
            ]
        );
        assert_eq!(toks("IF x <> 1 THEN { c } (* d *) BEGIN END")[..3], [Tok::Kw("if"), Tok::Ident("x".into()), Tok::Sym("<>")]);
        assert_eq!(toks("'a'#13#10'b'")[0], Tok::Str("a\r\nb".into()));
        let lines: Vec<usize> = tokenize("a\n\nb").unwrap().iter().map(|t| t.line).collect();
        assert_eq!(lines[..2], [1, 3]);
        assert!(tokenize("'abc").is_err() && tokenize("x @ y").is_err());
    }
}
