//! Tests avec les exemples de l'aide de FTPRush v2.

use super::*;

/// Hôte de test : enregistre les appels et répond à quelques propriétés.
#[derive(Default)]
struct Mock {
    calls: Vec<(String, Vec<Value>)>,
    sets: Vec<(String, Value)>,
    input: Option<String>,
}

impl Host for Mock {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, HostError> {
        self.calls.push((name.to_string(), args.to_vec()));
        if name.eq_ignore_ascii_case("InputQuery") {
            return Ok(self.input.clone().map_or(Value::Nil, Value::Str));
        }
        if name.eq_ignore_ascii_case("RushApp.FTP.Fail") {
            return Err(HostError::InvalidArgument { function: name.into(), reason: "simulated failure".into() });
        }
        Ok(Value::Nil)
    }

    fn get(&mut self, name: &str) -> Result<Value, HostError> {
        match name.to_ascii_lowercase().as_str() {
            "rushapp.version" => Ok(Value::Str("UltraRushFTP 0.1.0".into())),
            "rushapp.totaldownloadsize" => Ok(Value::Int(1234)),
            "rushapp.containers.containercount" => Ok(Value::Int(3)),
            "rushapp.language" => Ok(Value::Str("French".into())),
            _ => Err(HostError::UnknownProperty(name.into())),
        }
    }

    fn set(&mut self, name: &str, value: Value) -> Result<(), HostError> {
        self.sets.push((name.to_string(), value));
        Ok(())
    }
}

fn run_ok(src: &str) -> Mock {
    let mut m = Mock::default();
    run(src, &mut m).unwrap_or_else(|e| panic!("{e}\n{src}"));
    m
}

#[test]
fn flags_are_distinct_bits() {
    let mut seen = 0i64;
    for f in RS_FLAG_NAMES {
        let v = flag_value(f).unwrap();
        assert_eq!(v.count_ones(), 1);
        assert_eq!(seen & v, 0);
        seen |= v;
    }
    assert_eq!(flag_value("rs_bookmark"), flag_value("RS_BOOKMARK"));
    assert!(has_flag(flag_value("RS_UP").unwrap() | flag_value("RS_FILE").unwrap(), "RS_FILE"));
}

#[test]
fn login_examples_from_help() {
    let m = run_ok(
        "RushApp.FTP.Login('FTP1','',0);
         RushApp.FTP.Login('FTP1;FTP2','public',RS_BOOKMARK or RS_CHECKBM);
         RushApp.FTP.Login('FTP1','/pub/',0)",
    );
    assert_eq!(m.calls.len(), 3);
    assert_eq!(m.calls[0].0, "RushApp.FTP.Login");
    assert_eq!(m.calls[1].1[0], Value::Str("FTP1;FTP2".into()));
    let flags = m.calls[1].1[2].as_int().unwrap();
    assert!(has_flag(flags, "RS_BOOKMARK") && has_flag(flags, "RS_CHECKBM") && !has_flag(flags, "RS_POPUP"));
}

#[test]
fn transfer_with_21_arguments() {
    let m = run_ok(
        "RushApp.FTP.Transfer(0, 'FTP1', '/pictures', '', 'FTP2', '/test', '',
           RS_DIRDES or RS_DIRSRC, '', '', '', '', '', 0, 0, 0, RS_SORTDATE or RS_SORTDES, 0, 0, 1);",
    );
    let (name, args) = &m.calls[0];
    assert_eq!(name, "RushApp.FTP.Transfer");
    assert_eq!(args.len(), 20);
    assert!(has_flag(args[7].as_int().unwrap(), "RS_DIRSRC"));
    assert!(has_flag(args[16].as_int().unwrap(), "RS_SORTDES"));
}

#[test]
fn assignment_variables_and_conditions() {
    let m = run_ok(
        "uses rfScriptEnv, System;
         var i, total: integer; nom: string;
         begin
           RushApp.FTP.Splitter := '|';
           total := 0;
           for i := 1 to 4 do total := total + i;
           nom := 'site' + IntToStr(total);
           if RushApp.Language = 'French' then
             RushApp.FTP.RAW(nom, 'SITE WHO', RS_LOGIN or RS_LOGOUT)
           else
             RushApp.FTP.RAW('autre', 'NOOP', 0);
           while total > 7 do total := total - 1;
           repeat total := total * 2 until total >= 50;
         end.",
    );
    assert_eq!(m.sets, vec![("RushApp.FTP.Splitter".to_string(), Value::Str("|".into()))]);
    assert_eq!(m.calls[0].1[0], Value::Str("site10".into()));
    assert_eq!(m.calls[0].1[1], Value::Str("SITE WHO".into()));
}

#[test]
fn input_query_example_from_help() {
    let src = "defBookmark := '';
               if inputQuery('Input bookmark', 'bookmark', defBookmark) then
                 RushApp.FTP.CWD('', defBookmark, RS_BOOKMARK);";
    let mut m = Mock { input: Some("pub".into()), ..Mock::default() };
    run(src, &mut m).unwrap();
    let cwd = m.calls.iter().find(|c| c.0 == "RushApp.FTP.CWD").unwrap();
    assert_eq!(cwd.1[1], Value::Str("pub".into()));

    let mut cancelled = Mock::default();
    run(src, &mut cancelled).unwrap();
    assert!(!cancelled.calls.iter().any(|c| c.0 == "RushApp.FTP.CWD"), "annulé : pas de CWD");
}

#[test]
fn invoke_expressions() {
    let mut m = Mock::default();
    assert_eq!(evaluate("RushApp.TotalDownloadSize", &mut m).unwrap(), Value::Int(1234));
    // L'exemple mIRC de l'aide termine certaines expressions par « ; ».
    assert_eq!(evaluate("RushApp.Containers.ContainerCount;", &mut m).unwrap().to_text(), "3");
    assert_eq!(evaluate("rushapp.version", &mut m).unwrap().to_text(), "UltraRushFTP 0.1.0");
    assert_eq!(
        evaluate("RushApp.Inconnu", &mut m).unwrap_err().kind,
        ScriptErrorKind::Host(HostError::UnknownProperty("RushApp.Inconnu".into()))
    );
    assert!(evaluate("1 2", &mut m).is_err());
}

#[test]
fn builtins() {
    let mut m = Mock::default();
    let e = |s: &str, m: &mut Mock| evaluate(s, m).unwrap();
    assert_eq!(e("Length('héllo')", &mut m), Value::Int(5));
    assert_eq!(e("Copy('abcdef', 2, 3)", &mut m), Value::Str("bcd".into()));
    assert_eq!(e("Pos('cd', 'abcdef')", &mut m), Value::Int(3));
    assert_eq!(e("UpperCase('rush') + LowerCase('FTP')", &mut m), Value::Str("RUSHftp".into()));
    assert_eq!(e("StrToInt(' 42 ') * 2", &mut m), Value::Int(84));
    assert_eq!(e("StrToIntDef('x', 7)", &mut m), Value::Int(7));
    assert_eq!(e("Trim('  a ')", &mut m), Value::Str("a".into()));
    assert_eq!(e("not (1 > 2) and (3 >= 3)", &mut m), Value::Bool(true));
    assert_eq!(e("17 mod 5 + 17 div 5", &mut m), Value::Int(5));
    run_ok("ShowMessage('bonjour');");
}

#[test]
fn errors_report_lines() {
    let mut m = Mock::default();
    let e = run("x := 1;\ny := 'a' - 2;", &mut m).unwrap_err();
    assert_eq!(e.line, 2);
    assert!(e.to_string().starts_with("line 2: "), "{e}");
    let e = run("RushApp.FTP.Login('a', '', 0)\nRushApp.FTP.Logout('', 0);", &mut m).unwrap_err();
    assert!(matches!(&e.kind, ScriptErrorKind::Syntax(m) if m.contains(";")), "{e}");
    assert_eq!(run("\n\nRushApp.FTP.Fail();", &mut m).unwrap_err().line, 3);
    assert_eq!(run("x := 1 div 0;", &mut m).unwrap_err().kind, ScriptErrorKind::DivisionByZero);
    assert!(matches!(run("procedure P; begin end;", &mut m).unwrap_err().kind, ScriptErrorKind::Unsupported(_)));
    assert!(check("begin if then end").is_err());
    assert!(check("begin RushApp.FTP.Logout('', RS_ASYNC); end.").is_ok());
}

#[test]
fn exit_and_infinite_loop_guard() {
    let m = run_ok("RushApp.FTP.A(); exit; RushApp.FTP.B();");
    assert_eq!(m.calls.len(), 1);
    let mut m = Mock::default();
    assert_eq!(run("while true do x := 1;", &mut m).unwrap_err().kind, ScriptErrorKind::StepLimit);
}
