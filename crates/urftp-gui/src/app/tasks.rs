//! Task window: scheduled queues and scripts (`urftp_core::tasks`), their dialog, and the
//! check that runs them when their time comes.

use eframe::egui::{self, RichText};
use urftp_core::queuestore::SavedTab;
use urftp_core::tasks::{
    self, ALL_DAYS, LastResult, Schedule, Task, TaskAction, TaskBook, format_local, format_minute, parse_local, parse_minute,
};
use urftp_i18n::t;

use super::{Action, LogKind, UltraRushApp, scripting};
use crate::theme;

/// Tasks of the application, and the dialog open on one of them.
#[derive(Default)]
pub(super) struct TasksState {
    pub(super) book: TaskBook,
    form: Option<TaskForm>,
    /// Missed times checked (first frame).
    started: bool,
    /// Read or write error of `tasks.json`, shown in the Task window.
    pub(super) error: Option<String>,
}

impl TasksState {
    pub(super) fn load() -> Self {
        match TaskBook::load(&TaskBook::default_path()) {
            Ok(book) => Self { book, ..Self::default() },
            Err(e) => Self { error: Some(t!("tasks.unreadable", error = e)), ..Self::default() },
        }
    }

    fn save(&mut self) {
        self.error = self.book.save(&TaskBook::default_path()).err().map(|e| t!("tasks.save_failed", error = e));
    }
}

/// Local time of the computer, as the tasks count it.
pub(super) fn now_local() -> i64 {
    tasks::local_now(urftp_sys::win::local_time())
}

/// Fields of the task dialog.
struct TaskForm {
    /// Task edited (`None`: a new one).
    id: Option<u64>,
    name: String,
    enabled: bool,
    kind: Kind,
    at: String,
    minutes: u32,
    from: String,
    time: String,
    days: u8,
    action: TaskAction,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Once,
    Every,
    Daily,
}

impl TaskForm {
    fn of(task: &Task, id: Option<u64>, now: i64) -> Self {
        // Fields of the other kinds keep sensible values when the user switches.
        let soon = format_local(now - now.rem_euclid(60) + 3600);
        let mut f = Self {
            id,
            name: task.name.clone(),
            enabled: task.enabled,
            kind: Kind::Once,
            at: soon.clone(),
            minutes: 60,
            from: soon,
            time: "08:00".into(),
            days: ALL_DAYS,
            action: task.action.clone(),
            error: None,
        };
        // A new task starts from the defaults above.
        if id.is_none() {
            return f;
        }
        match task.schedule {
            Schedule::Once { at } => f.at = format_local(at),
            Schedule::Every { minutes, from } => (f.kind, f.minutes, f.from) = (Kind::Every, minutes, format_local(from)),
            Schedule::Daily { minute, days } => (f.kind, f.time, f.days) = (Kind::Daily, format_minute(minute), days),
        }
        f
    }

    fn schedule(&self) -> Result<Schedule, String> {
        let time = |text: &str| parse_local(text).ok_or_else(|| t!("tasks.bad_date", text = text));
        Ok(match self.kind {
            Kind::Once => Schedule::Once { at: time(&self.at)? },
            Kind::Every => Schedule::Every { minutes: self.minutes.max(1), from: time(&self.from)? },
            Kind::Daily => {
                let minute = parse_minute(&self.time).ok_or_else(|| t!("tasks.bad_time", text = &self.time))?;
                if self.days == 0 {
                    return Err(t!("tasks.no_day"));
                }
                Schedule::Daily { minute, days: self.days }
            }
        })
    }
}

/// Short day names, Monday first.
fn day_name(d: u32) -> String {
    match d {
        0 => t!("tasks.day_0"),
        1 => t!("tasks.day_1"),
        2 => t!("tasks.day_2"),
        3 => t!("tasks.day_3"),
        4 => t!("tasks.day_4"),
        5 => t!("tasks.day_5"),
        _ => t!("tasks.day_6"),
    }
}

/// "Every day at 08:00", "Every 30 min from …", "Once, …".
pub(super) fn describe(s: &Schedule) -> String {
    match *s {
        Schedule::Once { at } => t!("tasks.when_once", at = format_local(at)),
        Schedule::Every { minutes, from } => t!("tasks.when_every", n = minutes, from = format_local(from)),
        Schedule::Daily { minute, days } if days == ALL_DAYS => t!("tasks.when_daily", time = format_minute(minute)),
        Schedule::Daily { minute, days } => {
            let names: Vec<String> = (0..7).filter(|d| days & (1 << d) != 0).map(day_name).collect();
            t!("tasks.when_days", days = names.join(", "), time = format_minute(minute))
        }
    }
}

fn describe_action(a: &TaskAction) -> String {
    match a {
        TaskAction::Queue { queue, .. } => t!("tasks.action_queue", n = queue.items.len()),
        TaskAction::Script { .. } => t!("tasks.action_script"),
    }
}

fn describe_last(task: &Task) -> String {
    let when = task.last_run.map(format_local).unwrap_or_default();
    match &task.last_result {
        None => "—".into(),
        Some(LastResult::Started) => when,
        Some(LastResult::Missed) => t!("tasks.missed"),
        Some(LastResult::Failed(e)) => t!("tasks.failed", when = when, error = e),
    }
}

/// Content of the Task window (a pane of the tab).
pub(super) fn tasks_pane(ui: &mut egui::Ui, state: (&TaskBook, Option<&str>), actions: &mut Vec<Action>) {
    let (book, error) = state;
    ui.horizontal(|ui| {
        if ui.button(t!("tasks.new_script")).clicked() {
            actions.push(Action::TaskNewScript);
        }
        ui.label(RichText::new(t!("tasks.queue_hint")).weak());
    });
    if let Some(e) = error {
        ui.colored_label(theme::colors(ui).error, format!("⚠ {e}"));
    }
    ui.separator();
    if book.tasks.is_empty() {
        ui.label(RichText::new(t!("tasks.none")).italics());
        return;
    }
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("tasks-grid").striped(true).num_columns(7).spacing([14.0, 6.0]).show(ui, |ui| {
            for h in [
                t!("tasks.col_on"),
                t!("tasks.col_name"),
                t!("tasks.col_action"),
                t!("tasks.col_when"),
                t!("tasks.col_next"),
                t!("tasks.col_last"),
                t!("tasks.col_do"),
            ] {
                ui.strong(h);
            }
            ui.end_row();
            for task in &book.tasks {
                let mut on = task.enabled;
                let r = ui.checkbox(&mut on, "");
                theme::name(&r, t!("tasks.toggle_named", name = &task.name));
                if r.changed() {
                    actions.push(Action::TaskToggle(task.id));
                }
                ui.label(&task.name);
                ui.label(describe_action(&task.action));
                ui.label(describe(&task.schedule));
                ui.label(task.next_run.map(format_local).unwrap_or_else(|| "—".into()));
                ui.label(describe_last(task));
                ui.horizontal(|ui| {
                    let run = theme::icon_button(ui, "▶", t!("tasks.run_now"));
                    theme::name(&run, t!("tasks.run_now_named", name = &task.name));
                    if run.clicked() {
                        actions.push(Action::TaskRunNow(task.id));
                    }
                    let edit = theme::icon_button(ui, "✎", t!("tasks.edit"));
                    theme::name(&edit, t!("tasks.edit_named", name = &task.name));
                    if edit.clicked() {
                        actions.push(Action::TaskEdit(task.id));
                    }
                });
                ui.end_row();
            }
        });
    });
}

impl UltraRushApp {
    /// Every frame: marks the times missed while closed (first frame), runs the tasks
    /// whose time has come, and wakes the interface up for the next one.
    pub(super) fn tasks_tick(&mut self, ctx: &egui::Context) {
        let now = now_local();
        if !std::mem::replace(&mut self.tasks.started, true) && self.tasks.book.skip_missed(now) {
            self.tasks.save();
        }
        let due = self.tasks.book.take_due(now);
        if !due.is_empty() {
            for task in due {
                self.run_task(task, ctx);
            }
            self.tasks.save();
        }
        if let Some(next) = self.tasks.book.next_due() {
            let wait = u64::try_from((next - now).clamp(1, 30)).unwrap_or(30);
            ctx.request_repaint_after(std::time::Duration::from_secs(wait));
        }
    }

    /// Runs a task: its queue in a new tab (the tab shown does not change), or its script.
    fn run_task(&mut self, task: Task, ctx: &egui::Context) {
        let a = self.active;
        match task.action {
            TaskAction::Queue { queue, close_tab } => {
                if queue.items.is_empty() {
                    if let Some(t) = self.tasks.book.get_mut(task.id) {
                        t.last_result = Some(LastResult::Failed(t!("tasks.empty_queue")));
                    }
                    return;
                }
                let n = queue.items.len();
                self.new_tab();
                let t = self.tabs.len() - 1;
                self.active = a;
                *self.tabs[t].queue.lock() = queue.into_queue();
                self.tabs[t].close_when_done = close_tab;
                self.tabs[t].push_transfer(0, LogKind::Status, t!("tasks.started_queue", name = &task.name, n = n));
                let (rt, opts, extra) = (self.rt.clone(), self.engine_options(), self.queue_site_profiles(t));
                self.tabs[t].start_queue(&rt, ctx, opts, &extra);
            }
            TaskAction::Script { source } => {
                self.tabs[a].push_log(0, LogKind::Status, t!("tasks.started_script", name = &task.name));
                self.start_script(source, scripting::Origin::Task(task.name.clone()), ctx);
            }
        }
    }

    /// Actions of the Task window and of "Schedule queue to task".
    pub(super) fn task_action(&mut self, action: Action, ctx: &egui::Context) {
        let now = now_local();
        match action {
            Action::TaskNewScript => {
                let task = Task::new(t!("tasks.new_script_name"), Schedule::Once { at: 0 }, TaskAction::Script { source: String::new() });
                self.tasks.form = Some(TaskForm::of(&task, None, now));
            }
            Action::ScheduleQueue => {
                let a = self.active;
                let title = self.tabs[a].title(a + 1);
                let queue = SavedTab::of_queue(title.clone(), &self.tabs[a].queue.lock());
                let task = Task::new(
                    t!("tasks.queue_name", title = title),
                    Schedule::Once { at: 0 },
                    TaskAction::Queue { queue, close_tab: true },
                );
                self.tasks.form = Some(TaskForm::of(&task, None, now));
            }
            Action::TaskEdit(id) => {
                self.tasks.form = self.tasks.book.get(id).map(|task| TaskForm::of(task, Some(id), now));
            }
            Action::TaskToggle(id) => {
                if let Some(task) = self.tasks.book.get_mut(id) {
                    task.enabled = !task.enabled;
                    task.plan(now);
                }
                self.tasks.save();
            }
            Action::TaskRunNow(id) => {
                if let Some(task) = self.tasks.book.get_mut(id) {
                    task.last_run = Some(now);
                    task.last_result = Some(LastResult::Started);
                    let task = task.clone();
                    self.run_task(task, ctx);
                }
                self.tasks.save();
            }
            _ => {}
        }
    }

    /// Dialog of a task: name, schedule, script or queue.
    pub(super) fn task_dialog(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.tasks.form else { return };
        let mut done: Option<Outcome> = None;
        egui::Modal::new(egui::Id::unique("task-dialog")).show(ctx, |ui| {
            ui.set_min_width(560.0);
            ui.heading(if form.id.is_some() { t!("tasks.edit_title") } else { t!("tasks.new_title") });
            ui.add_space(4.0);
            egui::Grid::new("task-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                theme::grid_row(ui, t!("tasks.name"), theme::single_line(&mut form.name).desired_width(320.0));
                ui.label("");
                ui.checkbox(&mut form.enabled, t!("tasks.enabled"));
                ui.end_row();
                ui.label(t!("tasks.when"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut form.kind, Kind::Once, t!("tasks.kind_once"));
                    ui.radio_value(&mut form.kind, Kind::Every, t!("tasks.kind_every"));
                    ui.radio_value(&mut form.kind, Kind::Daily, t!("tasks.kind_daily"));
                });
                ui.end_row();
                match form.kind {
                    Kind::Once => {
                        theme::grid_row(
                            ui,
                            t!("tasks.at"),
                            theme::single_line(&mut form.at).hint_text("2026-12-31 23:30").desired_width(160.0),
                        );
                    }
                    Kind::Every => {
                        theme::grid_row(ui, t!("tasks.every"), egui::DragValue::new(&mut form.minutes).range(1..=10_080).suffix(" min"));
                        theme::grid_row(ui, t!("tasks.from"), theme::single_line(&mut form.from).desired_width(160.0));
                    }
                    Kind::Daily => {
                        theme::grid_row(ui, t!("tasks.time"), theme::single_line(&mut form.time).hint_text("08:00").desired_width(70.0));
                        ui.label(t!("tasks.days"));
                        ui.horizontal(|ui| {
                            for d in 0..7u32 {
                                let mut on = form.days & (1 << d) != 0;
                                if ui.checkbox(&mut on, day_name(d)).changed() {
                                    form.days ^= 1 << d;
                                }
                            }
                        });
                        ui.end_row();
                    }
                }
            });
            ui.add_space(6.0);
            match &mut form.action {
                TaskAction::Queue { queue, close_tab } => {
                    ui.label(t!("tasks.queue_items", n = queue.items.len(), title = &queue.title));
                    ui.checkbox(close_tab, t!("tasks.close_tab"));
                    ui.label(RichText::new(t!("tasks.queue_note")).weak());
                }
                TaskAction::Script { source } => {
                    ui.label(t!("tasks.script"));
                    let r = ui.add(egui::TextEdit::multiline(source).code_editor().desired_rows(8).desired_width(f32::INFINITY));
                    theme::name(&r, t!("tasks.script"));
                }
            }
            if let Some(e) = &form.error {
                ui.colored_label(theme::colors(ui).error, format!("⚠ {e}"));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(t!("common.ok")).clicked() {
                    done = Some(Outcome::Save);
                }
                if ui.button(t!("common.cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(Outcome::Cancel);
                }
                if form.id.is_some() {
                    ui.add_space(24.0);
                    if ui.button(t!("tasks.delete")).clicked() {
                        done = Some(Outcome::Delete);
                    }
                }
            });
        });
        match done {
            Some(Outcome::Save) => {
                let schedule = match form.schedule() {
                    Ok(s) => s,
                    Err(e) => {
                        form.error = Some(e);
                        return;
                    }
                };
                let now = now_local();
                let name = if form.name.trim().is_empty() { t!("tasks.unnamed") } else { form.name.trim().to_owned() };
                match form.id.and_then(|id| self.tasks.book.get_mut(id)) {
                    Some(task) => {
                        (task.name, task.enabled, task.schedule, task.action) = (name, form.enabled, schedule, form.action.clone());
                        task.plan(now);
                    }
                    None => {
                        let mut task = Task::new(name, schedule, form.action.clone());
                        task.enabled = form.enabled;
                        self.tasks.book.add(task, now);
                    }
                }
                self.tasks.form = None;
                self.tasks.save();
            }
            Some(Outcome::Delete) => {
                if let Some(id) = form.id {
                    self.tasks.book.remove(id);
                }
                self.tasks.form = None;
                self.tasks.save();
            }
            Some(Outcome::Cancel) => self.tasks.form = None,
            None => {}
        }
    }

    /// The script of a task stopped on an error: written as its last result.
    pub(super) fn task_script_failed(&mut self, name: &str, error: &str) {
        if let Some(task) = self.tasks.book.tasks.iter_mut().rev().find(|t| t.name == name && matches!(t.action, TaskAction::Script { .. }))
        {
            task.last_result = Some(LastResult::Failed(error.to_owned()));
            self.tasks.save();
        }
    }

    /// A task dialog is open (the window shortcuts wait).
    pub(super) fn task_dialog_open(&self) -> bool {
        self.tasks.form.is_some()
    }
}

enum Outcome {
    Save,
    Cancel,
    Delete,
}
