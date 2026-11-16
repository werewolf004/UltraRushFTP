//! Commands of the command designer (FTPRush "FTP Command Designer"): FTP commands, one
//! per line, with parameters filled in when the command runs.
//!
//! | Parameter | Value |
//! |---|---|
//! | `%f` | Selected name; the line is sent once per selected item |
//! | `%p` | Current folder of the panel |
//! | `%c` | Text of the clipboard |
//! | `%d[caption]` | Text asked when the command runs, with this caption |
//! | `%1` … `%9` | Answer to the Nth question (asked when there is no `%d` for it) |
//! | `%%` | `%` |
//!
//! Options, anywhere in the text: `%all%` sends to every connected site, `%clearbuf%`
//! clears the directory cache and lists the folder again, `&window` shows the replies in
//! a window.

/// A command read before it runs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandSpec {
    /// Lines with their parameters, options removed, empty lines dropped.
    pub lines: Vec<String>,
    /// Captions of the questions to ask, in order (empty: no caption given).
    pub prompts: Vec<String>,
    /// `%all%`: every connected site.
    pub all: bool,
    /// `%clearbuf%`: directory cache cleared, folder listed again.
    pub clear_cache: bool,
    /// `&window`: replies shown in a window.
    pub window: bool,
}

/// Values of the parameters when the command runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct CommandValues<'a> {
    pub files: &'a [String],
    pub path: &'a str,
    pub clipboard: &'a str,
    /// Answers to [`CommandSpec::prompts`], in order.
    pub answers: &'a [String],
}

/// A command that cannot be sent as is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("the command uses %f but nothing is selected")]
    NoSelection,
    #[error("the command has no line to send")]
    Empty,
}

/// Reads the options and the questions of a command.
pub fn parse(text: &str) -> CommandSpec {
    let mut spec = CommandSpec::default();
    let mut cleaned = String::with_capacity(text.len());
    for line in text.lines() {
        let mut l = line.to_string();
        for (token, flag) in [("%all%", &mut spec.all), ("%clearbuf%", &mut spec.clear_cache), ("&window", &mut spec.window)] {
            if contains_ignore_case(&l, token) {
                *flag = true;
                l = remove_ignore_case(&l, token);
            }
        }
        let l = l.trim();
        if !l.is_empty() {
            cleaned.push_str(l);
            cleaned.push('\n');
            spec.lines.push(l.to_string());
        }
    }
    // Questions: each `%d[...]` in order, then the numbers used beyond them.
    let mut highest = 0usize;
    for tok in tokens(&cleaned) {
        match tok {
            Token::Prompt(caption) => spec.prompts.push(caption),
            Token::Answer(n) => highest = highest.max(n),
            _ => {}
        }
    }
    while spec.prompts.len() < highest {
        spec.prompts.push(String::new());
    }
    spec
}

/// The lines to send, parameters replaced.
pub fn expand(spec: &CommandSpec, v: &CommandValues<'_>) -> Result<Vec<String>, CommandError> {
    let mut out = Vec::new();
    // `%d[...]` questions are numbered across the whole command.
    let mut asked = 0usize;
    for line in &spec.lines {
        let toks = tokens(line);
        let start = asked;
        asked += toks.iter().filter(|t| matches!(t, Token::Prompt(_))).count();
        let per_file = toks.iter().any(|t| matches!(t, Token::File));
        if per_file && v.files.is_empty() {
            return Err(CommandError::NoSelection);
        }
        let names: Vec<Option<&str>> = if per_file { v.files.iter().map(|f| Some(f.as_str())).collect() } else { vec![None] };
        for name in names {
            let mut prompt = start;
            let mut s = String::new();
            for t in &toks {
                match t {
                    Token::Text(x) => s.push_str(x),
                    Token::File => s.push_str(name.unwrap_or_default()),
                    Token::Path => s.push_str(v.path),
                    Token::Clipboard => s.push_str(v.clipboard.trim_end_matches(['\r', '\n'])),
                    Token::Prompt(_) => {
                        prompt += 1;
                        s.push_str(answer(v, prompt));
                    }
                    Token::Answer(n) => s.push_str(answer(v, *n)),
                }
            }
            out.push(s);
        }
    }
    if out.is_empty() { Err(CommandError::Empty) } else { Ok(out) }
}

fn answer<'a>(v: &CommandValues<'a>, n: usize) -> &'a str {
    v.answers.get(n.wrapping_sub(1)).map_or("", String::as_str)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Text(String),
    File,
    Path,
    Clipboard,
    Prompt(String),
    Answer(usize),
}

/// Splits a text into literal parts and parameters (in order: [`parse`] numbers the
/// `%d[...]` questions of the whole command).
fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut rest = text;
    while let Some(pos) = rest.find('%') {
        lit.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        // (parameter, bytes it takes after the `%`); every marker is ASCII.
        let (tok, used) = match after.chars().next() {
            Some('%') => {
                lit.push('%');
                rest = &after[1..];
                continue;
            }
            Some('f' | 'F') => (Some(Token::File), 1),
            Some('p' | 'P') => (Some(Token::Path), 1),
            Some('c' | 'C') => (Some(Token::Clipboard), 1),
            Some(d @ '1'..='9') => (Some(Token::Answer(usize::from(d as u8 - b'0'))), 1),
            Some('d' | 'D') => match after[1..].strip_prefix('[').and_then(|r| r.find(']').map(|end| (r[..end].to_string(), end))) {
                // `d`, `[`, the caption, `]`.
                Some((caption, end)) => (Some(Token::Prompt(caption)), end + 3),
                None => (None, 0),
            },
            _ => (None, 0),
        };
        match tok {
            Some(t) => {
                flush(&mut lit, &mut out);
                out.push(t);
                rest = &after[used..];
            }
            None => {
                lit.push('%');
                rest = after;
            }
        }
    }
    lit.push_str(rest);
    flush(&mut lit, &mut out);
    out
}

fn flush(lit: &mut String, out: &mut Vec<Token>) {
    if !lit.is_empty() {
        out.push(Token::Text(std::mem::take(lit)));
    }
}

fn contains_ignore_case(hay: &str, needle: &str) -> bool {
    hay.to_ascii_lowercase().contains(needle)
}

fn remove_ignore_case(hay: &str, needle: &str) -> String {
    let lower = hay.to_ascii_lowercase();
    let mut out = String::new();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(needle) {
        out.push_str(&hay[from..from + pos]);
        from += pos + needle.len();
    }
    out.push_str(&hay[from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, v: CommandValues<'_>) -> Result<Vec<String>, CommandError> {
        expand(&parse(text), &v)
    }

    #[test]
    fn options_are_read_and_removed() {
        let s = parse("%all% SITE WHO\n&window\n%CLEARBUF%");
        assert!(s.all && s.window && s.clear_cache);
        assert_eq!(s.lines, ["SITE WHO"]);
    }

    #[test]
    fn files_path_and_clipboard() {
        let files = ["a.nfo".to_string(), "b sfv".to_string()];
        let v = CommandValues { files: &files, path: "/incoming", clipboard: "Some.Release-GRP\r\n", answers: &[] };
        assert_eq!(run("SITE NUKE %p/%f 3", v).unwrap(), ["SITE NUKE /incoming/a.nfo 3", "SITE NUKE /incoming/b sfv 3"]);
        assert_eq!(run("SITE SEARCH %c", v).unwrap(), ["SITE SEARCH Some.Release-GRP"]);
        assert_eq!(run("SITE UTIME %f\nSTAT", CommandValues { files: &[], ..v }), Err(CommandError::NoSelection));
        assert_eq!(run("100%% done", v).unwrap(), ["100% done"]);
        assert_eq!(run("50%x", v).unwrap(), ["50%x"]);
    }

    #[test]
    fn prompts_and_answers() {
        let s = parse("SITE ADDUSER %d[User] %d[Password]\nSITE GADDUSER %d[Group] %1");
        assert_eq!(s.prompts, ["User", "Password", "Group"]);
        let answers = ["bob".to_string(), "pw".to_string(), "staff".to_string()];
        let v = CommandValues { answers: &answers, ..CommandValues::default() };
        assert_eq!(expand(&s, &v).unwrap(), ["SITE ADDUSER bob pw", "SITE GADDUSER staff bob"]);
        // The former "%1" commands still ask their parameter.
        let s = parse("SITE SEARCH %1");
        assert_eq!(s.prompts, [""]);
        let a = ["dragon".to_string()];
        assert_eq!(expand(&s, &CommandValues { answers: &a, ..CommandValues::default() }).unwrap(), ["SITE SEARCH dragon"]);
        // A "%d" without brackets is plain text.
        assert!(parse("SITE %dx").prompts.is_empty());
    }

    #[test]
    fn empty_command() {
        assert_eq!(run("%all%\n  \n", CommandValues::default()), Err(CommandError::Empty));
    }
}
