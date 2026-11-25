//! Scripts running beside the interface: each script runs in a thread of its own and asks
//! the interface for every `RushApp.*` call, so that `InputQuery` can wait for the user,
//! `ShellExecuteCommand` for a confirmation, and a long script does not freeze the window.
//! The selection object (`RushApp.UI.Selection`) and the layout event
//! (`RushApp.UI.OnSwitchLayout`) are kept here too.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use urftp_core::settings::ShellPolicy;
use urftp_script::{EventHandler, Host, HostError, ScriptError, ScriptErrorKind, Value};

use super::*;

/// Where a script came from: where its result is written.
#[derive(Debug, Clone)]
pub(super) enum Origin {
    /// Script console of the tab with this id.
    Console(u32),
    /// `RushScript` / `RushCommand` of `rushmirc.dll`, with the window for the RAW replies.
    Mirc { what: &'static str, callback: Option<isize> },
    /// `-s` at startup.
    Startup(PathBuf),
    /// `URUSHFTP_SCRIPT`.
    Env,
    /// Handler of an event (`OnSwitchLayout`).
    Event(&'static str),
    /// Scheduled task with this name.
    Task(String),
}

impl Origin {
    fn label(&self) -> String {
        match self {
            Origin::Console(_) => t!("script.origin_console"),
            Origin::Mirc { what, .. } => format!("mIRC ({what})"),
            Origin::Startup(p) => p.display().to_string(),
            Origin::Env => "URUSHFTP_SCRIPT".into(),
            Origin::Event(e) => (*e).into(),
            Origin::Task(name) => t!("tasks.origin", name = name),
        }
    }
}

enum Ask {
    Call(String, Vec<Value>),
    Get(String),
    Set(String, Value),
}

type Reply = Sender<Result<Value, HostError>>;

enum Msg {
    Request {
        job: u64,
        ask: Ask,
        reply: Reply,
    },
    Done(u64, Result<(), ScriptError>),
    /// The logins asked by `RS_CHECKLOGIN` were tried: the call goes on, without the
    /// sites that failed.
    Checked {
        job: u64,
        name: String,
        args: Vec<Value>,
        reply: Reply,
        failed: Vec<(String, String)>,
    },
}

struct Job {
    id: u64,
    origin: Origin,
    cancel: Arc<AtomicBool>,
}

/// `InputQuery` waiting for the user.
pub(super) struct InputAsk {
    job: u64,
    title: String,
    prompt: String,
    value: String,
    reply: Reply,
    /// The field got the keyboard focus when the question appeared.
    focused: bool,
}

/// `ShellExecuteCommand` waiting for a confirmation.
pub(super) struct ShellAsk {
    job: u64,
    origin: String,
    program: String,
    params: String,
    dir: String,
    reply: Reply,
}

/// What `RushApp.UI.Selection.Prepare` collected.
#[derive(Debug, Clone, Default)]
pub(super) struct SelectionSnapshot {
    pub(super) remote: bool,
    pub(super) connected: bool,
    pub(super) ftp_name: String,
    pub(super) items: Vec<Value>,
}

/// Scripts running, their questions, and the script objects kept between scripts.
pub(super) struct Scripts {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    jobs: Vec<Job>,
    next: u64,
    inputs: VecDeque<InputAsk>,
    shells: VecDeque<ShellAsk>,
    pub(super) selection: Option<SelectionSnapshot>,
    pub(super) on_switch_layout: Option<EventHandler>,
}

impl Default for Scripts {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            jobs: Vec::new(),
            next: 1,
            inputs: VecDeque::new(),
            shells: VecDeque::new(),
            selection: None,
            on_switch_layout: None,
        }
    }
}

impl Scripts {
    /// A script started from the console of this tab is running.
    pub(super) fn console_running(&self, tab: u32) -> bool {
        self.jobs.iter().any(|j| matches!(j.origin, Origin::Console(t) if t == tab))
    }

    /// Number of scripts running.
    pub(super) fn running(&self) -> usize {
        self.jobs.len()
    }

    /// A question of a script is shown (the shortcuts of the window are off).
    pub(super) fn asking(&self) -> bool {
        self.waiting() > 0
    }

    fn waiting(&self) -> usize {
        self.inputs.len() + self.shells.len()
    }
}

/// The host seen by a script thread: every call goes to the interface and waits.
struct ProxyHost {
    job: u64,
    tx: Sender<Msg>,
    ctx: egui::Context,
    cancel: Arc<AtomicBool>,
}

impl ProxyHost {
    fn ask(&mut self, ask: Ask) -> Result<Value, HostError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(HostError::Cancelled);
        }
        let (reply, answer) = mpsc::channel();
        self.tx.send(Msg::Request { job: self.job, ask, reply }).map_err(|_| HostError::Cancelled)?;
        self.ctx.request_repaint();
        answer.recv().map_err(|_| HostError::Cancelled)?
    }
}

impl Host for ProxyHost {
    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, HostError> {
        self.ask(Ask::Call(name.to_string(), args.to_vec()))
    }
    fn get(&mut self, name: &str) -> Result<Value, HostError> {
        self.ask(Ask::Get(name.to_string()))
    }
    fn set(&mut self, name: &str, value: Value) -> Result<(), HostError> {
        self.ask(Ask::Set(name.to_string(), value)).map(drop)
    }
}

/// Starts a program for `ShellExecuteCommand`: `params` are given as written (Windows
/// command line), `dir` is the working folder (empty: the current one).
pub(super) fn launch(program: &str, params: &str, dir: &str) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if !params.trim().is_empty() {
            cmd.raw_arg(params);
        }
    }
    #[cfg(not(windows))]
    cmd.args(params.split_whitespace());
    if !dir.trim().is_empty() {
        cmd.current_dir(dir);
    }
    cmd.spawn().map(drop).map_err(|e| e.to_string())
}

impl UltraRushApp {
    /// Runs a script in its own thread; its result is written where it came from.
    pub(super) fn start_script(&mut self, source: String, origin: Origin, ctx: &egui::Context) {
        self.start_job(origin, ctx, move |host| urftp_script::run(&source, host));
    }

    fn start_job(
        &mut self,
        origin: Origin,
        ctx: &egui::Context,
        f: impl FnOnce(&mut dyn Host) -> Result<(), ScriptError> + Send + 'static,
    ) {
        let id = self.scripts.next;
        self.scripts.next += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut host = ProxyHost { job: id, tx: self.scripts.tx.clone(), ctx: ctx.clone(), cancel: Arc::clone(&cancel) };
        let spawned = std::thread::Builder::new().name(format!("script-{id}")).stack_size(8 << 20).spawn(move || {
            let r = f(&mut host);
            let _ = host.tx.send(Msg::Done(host.job, r));
            host.ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.scripts.jobs.push(Job { id, origin, cancel }),
            Err(e) => self.script_finished(&origin, Err(ScriptError::new(0, ScriptErrorKind::Runtime(e.to_string())))),
        }
    }

    /// Stops the scripts started from the console of this tab (at their next call to the
    /// application; a question waiting for the user is dropped).
    pub(super) fn stop_console_script(&mut self, tab: u32) {
        let ids: Vec<u64> = self.scripts.jobs.iter().filter(|j| matches!(j.origin, Origin::Console(t) if t == tab)).map(|j| j.id).collect();
        for j in self.scripts.jobs.iter().filter(|j| ids.contains(&j.id)) {
            j.cancel.store(true, Ordering::Relaxed);
        }
        self.scripts.inputs.retain(|q| !ids.contains(&q.job));
        self.scripts.shells.retain(|q| !ids.contains(&q.job));
    }

    /// Fires `RushApp.UI.OnSwitchLayout` (a saved layout was loaded).
    pub(super) fn fire_switch_layout(&mut self, ctx: &egui::Context) {
        if let Some(h) = self.scripts.on_switch_layout.clone() {
            self.start_job(Origin::Event("OnSwitchLayout"), ctx, move |host| h.fire(host));
        }
    }

    /// Answers the scripts: called every frame. A script making many calls is served
    /// within the same frame for up to 15 ms.
    pub(super) fn pump_scripts(&mut self, ctx: &egui::Context) {
        let deadline = Instant::now() + Duration::from_millis(15);
        loop {
            let msg = match self.scripts.rx.try_recv() {
                Ok(m) => m,
                Err(_) if self.scripts.running() > self.scripts.waiting() && Instant::now() < deadline => {
                    match self.scripts.rx.recv_timeout(Duration::from_millis(2)) {
                        Ok(m) => m,
                        Err(_) => continue,
                    }
                }
                Err(_) => break,
            };
            match msg {
                Msg::Done(id, r) => {
                    if let Some(i) = self.scripts.jobs.iter().position(|j| j.id == id) {
                        let job = self.scripts.jobs.remove(i);
                        self.script_finished(&job.origin, r);
                    }
                }
                Msg::Request { job, ask, reply } => self.answer_script(job, ask, reply, ctx),
                Msg::Checked { job, name, args, reply, failed } => {
                    self.checked_logins = Some(failed);
                    self.answer_script(job, Ask::Call(name, args), reply, ctx);
                    self.checked_logins = None;
                }
            }
        }
        if self.scripts.running() > 0 {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn answer_script(&mut self, id: u64, ask: Ask, reply: Reply, ctx: &egui::Context) {
        let Some(job) = self.scripts.jobs.iter().find(|j| j.id == id) else {
            let _ = reply.send(Err(HostError::Cancelled));
            return;
        };
        if job.cancel.load(Ordering::Relaxed) {
            let _ = reply.send(Err(HostError::Cancelled));
            return;
        }
        let origin = job.origin.clone();
        if let Ask::Call(name, args) = &ask {
            // RS_CHECKLOGIN: the logins are tried beside the interface, then the call goes on.
            if self.checked_logins.is_none()
                && let Some(sites) = self.checklogin_sites(name, args)
            {
                let check = self.login_checks(&sites);
                let (tx, wake) = (self.scripts.tx.clone(), ctx.clone());
                let (name, args) = (name.clone(), args.clone());
                self.rt.spawn(async move {
                    let failed = check.await;
                    let _ = tx.send(Msg::Checked { job: id, name, args, reply, failed });
                    wake.request_repaint();
                });
                return;
            }
            let arg = |i: usize| args.get(i).map(Value::to_text).unwrap_or_default();
            if name.eq_ignore_ascii_case("InputQuery") {
                self.scripts.inputs.push_back(InputAsk { job: id, title: arg(0), prompt: arg(1), value: arg(2), reply, focused: false });
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                return;
            }
            if name.eq_ignore_ascii_case("ShellExecuteCommand") {
                if args.len() < 2 {
                    let _ = reply.send(Err(HostError::Arity { name: name.clone(), expected: 2, got: args.len() }));
                    return;
                }
                let (program, params, dir) = (arg(0), arg(1), arg(2));
                if self.shell_decision(&program).is_some() {
                    let _ = reply.send(Ok(self.shell_now(&program, &params, &dir)));
                } else {
                    self.scripts.shells.push_back(ShellAsk { job: id, origin: origin.label(), program, params, dir, reply });
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                }
                return;
            }
        }
        self.ipc_callback = match origin {
            Origin::Mirc { callback, .. } => callback,
            _ => None,
        };
        let r = {
            let mut host = rushapp::AppHost::new(self, ctx);
            match ask {
                Ask::Call(name, args) => host.call(&name, &args),
                Ask::Get(name) => host.get(&name),
                Ask::Set(name, value) => host.set(&name, value).map(|()| Value::Nil),
            }
        };
        self.ipc_callback = None;
        let _ = reply.send(r);
    }

    /// `Some(true)`: start the program without asking, `Some(false)`: refuse it, `None`: ask.
    pub(super) fn shell_decision(&self, program: &str) -> Option<bool> {
        let s = &self.options.settings;
        match s.script_shell {
            ShellPolicy::Allow => Some(true),
            ShellPolicy::Deny => Some(false),
            ShellPolicy::Ask => s.script_programs.iter().any(|p| p.eq_ignore_ascii_case(program.trim())).then_some(true),
        }
    }

    /// Starts the program when the rules allow it without asking; the result of
    /// `ShellExecuteCommand`.
    pub(super) fn shell_now(&mut self, program: &str, params: &str, dir: &str) -> Value {
        match self.shell_decision(program) {
            Some(true) => self.shell_run(program, params, dir),
            Some(false) => {
                self.script_log(LogKind::Error, t!("script.shell_denied", program = program));
                Value::Bool(false)
            }
            None => {
                self.script_log(LogKind::Error, t!("script.shell_needs_confirmation", program = program));
                Value::Bool(false)
            }
        }
    }

    fn shell_run(&mut self, program: &str, params: &str, dir: &str) -> Value {
        match launch(program, params, dir) {
            Ok(()) => {
                self.script_log(LogKind::Status, t!("script.shell_started", program = program, params = params));
                Value::Bool(true)
            }
            Err(e) => {
                self.script_log(LogKind::Error, t!("script.shell_failed", program = program, error = e));
                Value::Bool(false)
            }
        }
    }

    fn script_log(&mut self, kind: LogKind, msg: String) {
        let t = self.active;
        self.tabs[t].push_log(0, kind, t!("log.script", message = msg));
    }

    /// Writes the result of a script where it came from.
    fn script_finished(&mut self, origin: &Origin, r: Result<(), ScriptError>) {
        let note = match origin {
            Origin::Console(_) => match r {
                Ok(()) => (true, t!("script.done")),
                Err(e) => (false, e.to_string()),
            },
            Origin::Mirc { what, .. } => match r {
                Ok(()) => (true, t!("script.mirc_done", what = what)),
                Err(e) => (false, t!("script.mirc_failed", what = what, error = e)),
            },
            Origin::Startup(path) => match r {
                Ok(()) => (true, t!("startup.script_done", path = path.display())),
                Err(e) => (false, t!("startup.script_failed", path = path.display(), error = e)),
            },
            Origin::Env => match r {
                Ok(()) => (true, t!("startup.env_script_done")),
                Err(e) => (false, t!("startup.env_script_failed", error = e)),
            },
            Origin::Event(name) => match r {
                Ok(()) => return,
                Err(e) => (false, t!("script.event_failed", event = name, error = e)),
            },
            Origin::Task(name) => match r {
                Ok(()) => (true, t!("tasks.script_done", name = name)),
                Err(e) => {
                    self.task_script_failed(name, &e.to_string());
                    (false, t!("tasks.script_failed", name = name, error = e))
                }
            },
        };
        let tab = match origin {
            Origin::Console(id) => self.tabs.iter().position(|t| t.id == *id).unwrap_or(self.active),
            _ => self.active,
        };
        let out = &mut self.tabs[tab].script_out;
        out.push(note);
        if out.len() > 200 {
            out.remove(0);
        }
    }

    /// `RushApp.UI.Selection.Prepare`: the selection of the active panel (or the row under
    /// the cursor when nothing is selected).
    pub(super) fn prepare_selection(&mut self) {
        let tab = &self.tabs[self.active];
        let side = &tab.sides[tab.focus.min(1)];
        let mut rows: Vec<usize> = side.selected().iter().copied().collect();
        if rows.is_empty() {
            rows.extend(side.cursor().row);
        }
        let (remote, connected, ftp_name) = match side {
            Side::Remote(r) => (true, r.state == RemoteState::Connected, r.profile.name.clone()),
            Side::Local(_) => (false, false, String::new()),
        };
        let items = rows
            .into_iter()
            .filter_map(|i| side.rows().get(i))
            .map(|f| {
                let full = match side {
                    Side::Remote(r) => rpath::join(&r.dir, &f.name),
                    Side::Local(l) => l.path.join(&f.name).display().to_string(),
                };
                Value::Record(vec![
                    ("Name".into(), Value::Str(f.name.clone())),
                    ("Size".into(), Value::Int(i64::try_from(f.size).unwrap_or(i64::MAX))),
                    ("Date".into(), Value::Str(f.modified.clone())),
                    ("Attr".into(), Value::Str(f.perms.clone())),
                    ("Owner".into(), Value::Str(f.owner.clone())),
                    ("Group".into(), Value::Str(f.group.clone())),
                    ("IsFolder".into(), Value::Bool(f.is_dir)),
                    ("FullPath".into(), Value::Str(full)),
                ])
            })
            .collect();
        self.scripts.selection = Some(SelectionSnapshot { remote, connected, ftp_name, items });
    }

    /// Questions of the scripts: `InputQuery`, then the programs to confirm.
    pub(super) fn script_dialogs(&mut self, ctx: &egui::Context) {
        if self.unlock.is_some() {
            return;
        }
        if let Some(q) = self.scripts.inputs.front_mut() {
            let mut answer: Option<Option<String>> = None;
            egui::Modal::new(egui::Id::unique(("script-input", q.job))).show(ctx, |ui| {
                ui.set_min_width(360.0);
                let title = if q.title.trim().is_empty() { t!("script.input_title") } else { q.title.clone() };
                ui.heading(title);
                ui.add_space(4.0);
                let r = theme::labelled(ui, &q.prompt, theme::single_line(&mut q.value).desired_width(f32::INFINITY));
                if !q.focused {
                    q.focused = true;
                    r.request_focus();
                }
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, t!("common.validate")).clicked() || enter {
                        answer = Some(Some(q.value.clone()));
                    }
                    if ui.button(t!("common.cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        answer = Some(None);
                    }
                });
            });
            if let Some(a) = answer
                && let Some(q) = self.scripts.inputs.pop_front()
            {
                let _ = q.reply.send(Ok(a.map_or(Value::Nil, Value::Str)));
            }
            return;
        }
        if let Some(q) = self.scripts.shells.front() {
            // 0 = refuse, 1 = run once, 2 = always allow this program.
            let mut choice: Option<u8> = None;
            egui::Modal::new(egui::Id::unique(("script-shell", q.job))).show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.heading(t!("script.shell_title"));
                ui.add_space(4.0);
                ui.label(t!("script.shell_question", origin = q.origin));
                egui::Grid::new("script-shell-grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    ui.label(t!("script.shell_program"));
                    ui.label(RichText::new(&q.program).monospace().strong());
                    ui.end_row();
                    ui.label(t!("script.shell_params"));
                    ui.label(RichText::new(&q.params).monospace());
                    ui.end_row();
                    ui.label(t!("script.shell_dir"));
                    ui.label(RichText::new(if q.dir.is_empty() { "—" } else { &q.dir }).monospace());
                    ui.end_row();
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t!("script.shell_run")).clicked() {
                        choice = Some(1);
                    }
                    if ui.button(t!("script.shell_always")).clicked() {
                        choice = Some(2);
                    }
                    if theme::primary_button(ui, t!("script.shell_refuse")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        choice = Some(0);
                    }
                });
            });
            if let Some(c) = choice
                && let Some(q) = self.scripts.shells.pop_front()
            {
                let result = match c {
                    0 => {
                        self.script_log(LogKind::Status, t!("script.shell_refused", program = q.program));
                        Value::Bool(false)
                    }
                    _ => {
                        if c == 2 && !self.options.settings.script_programs.iter().any(|p| p.eq_ignore_ascii_case(q.program.trim())) {
                            self.options.settings.script_programs.push(q.program.trim().to_string());
                            self.options.save();
                        }
                        self.shell_run(&q.program, &q.params, &q.dir)
                    }
                };
                let _ = q.reply.send(Ok(result));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_reports_a_missing_program() {
        assert!(launch("urftp-no-such-program-4f2a.exe", "", "").is_err());
    }
}
