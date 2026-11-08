//! Tests with the examples of the FTPRush v2 help.

use std::sync::Arc;

use super::*;

/// Test host: records the calls and answers a few properties.
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
    assert!(!cancelled.calls.iter().any(|c| c.0 == "RushApp.FTP.CWD"), "cancelled: no CWD");
}

#[test]
fn invoke_expressions() {
    let mut m = Mock::default();
    assert_eq!(evaluate("RushApp.TotalDownloadSize", &mut m).unwrap(), Value::Int(1234));
    // The mIRC example of the help ends some expressions with ";".
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
    assert!(matches!(run("procedure P; procedure Q; begin end; begin end;", &mut m).unwrap_err().kind, ScriptErrorKind::Unsupported(_)));
    assert!(check("begin x := 1; end; procedure P; begin end;").is_err(), "declarations come first");
    assert!(check("procedure P; begin end; procedure p; begin end;").is_err(), "declared twice");
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

fn global(src: &str, name: &str) -> Value {
    let mut m = Mock::default();
    let prog = parser::parse_program(lexer::tokenize(src).unwrap()).unwrap_or_else(|e| panic!("{e}\n{src}"));
    let mut it = interp::Interpreter::new(&mut m, Arc::new(prog));
    it.run().unwrap_or_else(|e| panic!("{e}\n{src}"));
    it.take_var(name).unwrap_or(Value::Nil)
}

#[test]
fn indexes_and_arrays_from_help() {
    let src = "NewArray := [ 2,4,6,8 ];
               Num := NewArray[1];
               MultiArray := [ ['green','red','blue'] , ['apple','orange','lemon'] ];
               Str := MultiArray[0,2];
               MultiArray[1,1] := 'new orange';
               Other := MultiArray[1][1];
               MyStr := 'abc'; MyStr[1] := 'A'; MyChar := MyStr[2];";
    assert_eq!(global(src, "num"), Value::Int(4));
    assert_eq!(global(src, "str"), Value::Str("blue".into()));
    assert_eq!(global(src, "other"), Value::Str("new orange".into()));
    assert_eq!(global(src, "mystr"), Value::Str("Abc".into()));
    assert_eq!(global(src, "mychar"), Value::Str("b".into()));
    let mut m = Mock::default();
    let e = |s: &str, m: &mut Mock| evaluate(s, m).unwrap();
    assert_eq!(e("Length([1, 2, 3]) + High([1, 2, 3]) + Low([1])", &mut m), Value::Int(5));
    assert_eq!(e("3 in [1, 2, 3]", &mut m), Value::Bool(true));
    assert_eq!(e("'x' in ['a', 'b']", &mut m), Value::Bool(false));
    assert_eq!(e("[1] + [2]", &mut m).to_text(), "[1, 2]");
    assert_eq!(e("Chr(65) + IntToStr(Ord('a')) + IntToStr(Abs(-3))", &mut m), Value::Str("A973".into()));
    assert_eq!(e("Odd(3)", &mut m), Value::Bool(true));
    assert!(run("a := [1, 2]; b := a[2];", &mut m).unwrap_err().to_string().contains("out of bounds"));
    assert!(run("s := 'ab'; s[0] := 'x';", &mut m).is_err(), "strings are 1-based");
    assert!(run("n := 5; x := n[0];", &mut m).is_err());
    assert_eq!(global("SetLength(a, 3); a[2] := 7; n := Length(a);", "n"), Value::Int(3));
}

#[test]
fn case_from_help() {
    let src = |fruit: &str| {
        format!(
            "Fruit := '{fruit}';
             case lowercase(Fruit) of
               'lime': c := 'green';
               'orange': c := 'orange';
               'apple', 'cherry': c := 'red';
             else
               c := 'black';
             end;"
        )
    };
    assert_eq!(global(&src("Lime"), "c"), Value::Str("green".into()));
    assert_eq!(global(&src("cherry"), "c"), Value::Str("red".into()));
    assert_eq!(global(&src("kiwi"), "c"), Value::Str("black".into()));
    let ranges = "n := 7; case n of 1..5: r := 'low'; 6..9: r := 'high' end";
    assert_eq!(global(ranges, "r"), Value::Str("high".into()));
    assert_eq!(global("case 3 of 1: r := 1 end; r := 2;", "r"), Value::Int(2), "no branch, no else");
}

#[test]
fn procedures_and_functions_from_help() {
    let src = "function Max(A,B);
               begin
                 if A>B then
                   result:=A
                 else
                   result:=B;
               end;
               procedure UpcaseMessage(Msg);
               begin
                 ShowMessage(Uppercase(Msg));
               end;
               procedure SwapValues(var A, B);
               var Temp;
               begin
                 Temp:=A;
                 A:=B;
                 B:=Temp;
               end;
               function Fact(n: integer): integer;
               begin
                 if n <= 1 then Fact := 1 else Fact := n * Fact(n - 1);
               end;
               procedure HelloWord;
               begin
                 ShowMessage('Hello world!');
                 exit;
                 ShowMessage('never');
               end;
               var x, y;
               begin
                 x := 1; y := 2;
                 SwapValues(x, y);
                 m := Max(3, 9);
                 f := Fact(5);
                 UpcaseMessage('hi');
                 HelloWord;
               end.";
    assert_eq!(global(src, "x"), Value::Int(2));
    assert_eq!(global(src, "y"), Value::Int(1));
    assert_eq!(global(src, "m"), Value::Int(9));
    assert_eq!(global(src, "f"), Value::Int(120));
    assert_eq!(global(src, "temp"), Value::Nil, "locals stay local");
    let m = run_ok(src);
    let shown: Vec<_> = m.calls.iter().filter(|c| c.0 == "ShowMessage").map(|c| c.1[0].to_text()).collect();
    assert_eq!(shown, ["HI", "Hello world!"]);

    let mut m = Mock::default();
    assert!(run("procedure P(a); begin end; P(1, 2);", &mut m).unwrap_err().to_string().contains("expects 1"));
    assert!(run("procedure P(var a); begin end; P(1);", &mut m).unwrap_err().to_string().contains("must be a variable"));
    assert!(run("procedure P; begin P; end; P;", &mut m).unwrap_err().to_string().contains("recursion"));
    // Global variables are seen and changed by procedures.
    assert_eq!(global("procedure Inc2; begin total := total + 2; end; total := 1; Inc2; Inc2;", "total"), Value::Int(5));
    // Constants, untyped var declarations (`var S;`) and implicit variables.
    assert_eq!(global("const Base = 10; var S; S := Base * 2;", "s"), Value::Int(20));
}

#[test]
fn break_and_continue() {
    let src = "total := 0;
               for i := 1 to 10 do begin
                 if i = 3 then continue;
                 if i > 5 then break;
                 total := total + i;
               end;
               n := 0;
               while true do begin n := n + 1; if n = 4 then break; end;
               repeat n := n + 1; if n < 10 then continue; break; until false;";
    assert_eq!(global(src, "total"), Value::Int(1 + 2 + 4 + 5));
    assert_eq!(global(src, "n"), Value::Int(10));
    let mut m = Mock::default();
    assert!(run("break;", &mut m).is_err());
}

/// Host whose selection holds one file.
struct Sel;

impl Host for Sel {
    fn call(&mut self, name: &str, _: &[Value]) -> Result<Value, HostError> {
        Err(HostError::UnknownFunction(name.into()))
    }
    fn get(&mut self, name: &str) -> Result<Value, HostError> {
        match name {
            "RushApp.UI.Selection.Items" => {
                Ok(Value::Array(vec![Value::Record(vec![("Name".into(), Value::Str("a.txt".into())), ("Size".into(), Value::Int(12))])]))
            }
            _ => Err(HostError::UnknownProperty(name.into())),
        }
    }
    fn set(&mut self, name: &str, _: Value) -> Result<(), HostError> {
        Err(HostError::ReadOnlyProperty(name.into()))
    }
}

#[test]
fn events_and_objects() {
    let mut m = run_ok(
        "procedure OnLayout(Sender);
         begin
           RushApp.FTP.RAW('', 'NOOP', 0);
         end;
         RushApp.UI.OnSwitchLayout := OnLayout;",
    );
    let (name, Value::Event(handler)) = m.sets.remove(0) else { panic!("an event handler is given to the host") };
    assert_eq!(name, "RushApp.UI.OnSwitchLayout");
    assert_eq!(handler.name(), "OnLayout");
    assert!(m.calls.is_empty(), "the handler is not run when it is set");
    handler.fire(&mut m).unwrap();
    assert_eq!(m.calls[0].0, "RushApp.FTP.RAW");

    let e = "RushApp.UI.Selection.Items[0].name + '/' + IntToStr(RushApp.UI.Selection.Items[0].Size)";
    assert_eq!(evaluate(e, &mut Sel).unwrap().to_text(), "a.txt/12");
    assert!(evaluate("RushApp.UI.Selection.Items[0].Owner", &mut Sel).is_err());
}
