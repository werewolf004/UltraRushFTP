//! Interface tests: the real main window driven through `egui_kittest`, checked through its
//! AccessKit tree (what screen readers see) and with keyboard input only.
//!
//! The application runs on scratch folders (data and local panels), without the mIRC
//! link and without network.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use eframe::egui::{self, Key, Modifiers, accesskit::Role};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use urftp_i18n::{Lang, t};

use super::{Side, UltraRushApp};
use crate::cmdline::StartupOptions;

/// Local panels start here (see `home_dir`).
pub(crate) static HOME: OnceLock<PathBuf> = OnceLock::new();

const WINDOW: [f32; 2] = [1320.0, 820.0];

/// Scratch folder of this test process: `data/` for the settings, `home/` with demo files.
fn scratch() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("urftp-ui-tests-{}", std::process::id()));
        let home = root.join("home");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(home.join("docs")).unwrap();
        for (name, len) in [("alpha.txt", 10), ("beta.bin", 2048), ("gamma.log", 300)] {
            std::fs::write(home.join(name), vec![b'x'; len]).unwrap();
        }
        urftp_core::sitestore::set_data_dir(root.join("data"));
        HOME.get_or_init(|| home);
        root
    })
}

/// Running application and the runtime it uses.
struct Ui {
    harness: Harness<'static, UltraRushApp>,
    _rt: tokio::runtime::Runtime,
    _lock: parking_lot::MutexGuard<'static, ()>,
}

impl Ui {
    fn start(lang: Lang, dark: bool) -> Self {
        let lock = crate::TEST_LOCK.lock();
        scratch();
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let mut app = UltraRushApp::new(rt.handle().clone(), StartupOptions::default());
        app.ipc_enabled = false;
        urftp_i18n::set_language(lang);
        let harness = Harness::builder().with_size(WINDOW).build_eframe(move |cc| {
            crate::theme::install_fonts(&cc.egui_ctx);
            crate::theme::apply(&cc.egui_ctx, dark);
            app
        });
        let mut ui = Self { harness, _rt: rt, _lock: lock };
        ui.run();
        ui
    }

    fn app(&mut self) -> &mut UltraRushApp {
        self.harness.state_mut()
    }

    /// A few frames: enough for actions, dialogs and focus changes to settle.
    fn run(&mut self) {
        self.harness.run_steps(4);
    }

    fn key(&mut self, key: Key) {
        self.harness.key_press(key);
        self.run();
    }

    fn key_with(&mut self, m: Modifiers, key: Key) {
        self.harness.key_press_modifiers(m, key);
        self.run();
    }

    fn has_label(&self, label: &str) -> bool {
        self.harness.query_all_by_label(label).next().is_some()
    }

    fn focused_label(&self) -> Option<String> {
        self.harness.root().children_recursive().find(|n| n.accesskit_node().is_focused()).and_then(|n| n.accesskit_node().label())
    }

    fn widget_focused(&self) -> bool {
        self.harness.ctx.memory(|m| m.focused().is_some())
    }

    /// Names of the left panel rows, in display order.
    fn left_rows(&mut self) -> Vec<String> {
        self.app().tabs[0].sides[0].rows().iter().map(|r| r.name.clone()).collect()
    }

    fn left_selection(&mut self) -> Vec<String> {
        let side = &self.app().tabs[0].sides[0];
        side.picked().into_iter().map(|(n, _)| n).collect()
    }

    fn left_path(&mut self) -> PathBuf {
        match &self.app().tabs[0].sides[0] {
            Side::Local(l) => l.path.clone(),
            Side::Remote(_) => PathBuf::new(),
        }
    }

    /// Controls that a user operates (and that therefore need a name).
    fn controls(&self) -> Vec<(Role, Option<String>, egui::Rect)> {
        self.harness
            .root()
            .children_recursive()
            .filter(|n| !n.accesskit_node().is_hidden())
            .filter(|n| {
                matches!(
                    n.accesskit_node().role(),
                    Role::Button
                        | Role::CheckBox
                        | Role::RadioButton
                        | Role::ComboBox
                        | Role::TextInput
                        | Role::MultilineTextInput
                        | Role::PasswordInput
                        | Role::SpinButton
                        | Role::Slider
                )
            })
            .map(|n| (n.accesskit_node().role(), n.accesskit_node().label(), n.rect()))
            .collect()
    }
}

/// Every control has a name made of words, not only a symbol (WCAG 4.1.2).
#[track_caller]
fn assert_all_named(ui: &Ui, screen: &str) {
    let controls = ui.controls();
    assert!(controls.len() > 5, "{screen}: too few controls found ({})", controls.len());
    let unnamed: Vec<_> = controls
        .iter()
        .filter(|(_, label, _)| !label.as_deref().is_some_and(|l| l.chars().any(char::is_alphanumeric)))
        .map(|(role, label, rect)| format!("{role:?} {label:?} at {:?}", rect.min))
        .collect();
    assert!(unnamed.is_empty(), "{screen}: controls without an accessible name:\n{}", unnamed.join("\n"));
}

/// Pointer targets of at least 24 × 24 px, or else spaced like WCAG 2.5.8 allows: a 24 px
/// circle centred on an undersized target touches no other target, nor the circle of
/// another undersized one.
#[track_caller]
fn assert_targets(ui: &Ui, screen: &str) {
    const MIN: f32 = 24.0 - 0.5;
    const RADIUS: f32 = 12.0;
    let rects: Vec<(String, egui::Rect)> = ui.controls().into_iter().map(|(role, label, r)| (format!("{role:?} {label:?}"), r)).collect();
    let small = |r: &egui::Rect| r.width() < MIN || r.height() < MIN;
    let mut failures = Vec::new();
    for (i, (name, r)) in rects.iter().enumerate().filter(|(_, (_, r))| small(r)) {
        let c = r.center();
        let clash = rects.iter().enumerate().filter(|(j, _)| *j != i).find(|(_, (_, o))| {
            // Same control seen twice (a combo box and its button share the rectangle).
            if o == r {
                return false;
            }
            let touches_target = o.distance_to_pos(c) < RADIUS;
            let touches_circle = small(o) && o.center().distance(c) < 2.0 * RADIUS;
            touches_target || touches_circle
        });
        if let Some((_, (other, _))) = clash {
            failures.push(format!("{name}: {:.0}×{:.0}, too close to {other}", r.width(), r.height()));
        }
    }
    assert!(
        failures.is_empty(),
        "{screen}: targets smaller than 24×24 px and not spaced enough:
{}",
        failures.join(
            "
"
        )
    );
}

#[test]
fn main_window_controls_are_named_and_large_enough() {
    for lang in [Lang::En, Lang::Fr] {
        let ui = Ui::start(lang, true);
        assert_all_named(&ui, "main window");
        assert_targets(&ui, "main window");
    }
}

#[test]
fn options_pages_controls_are_named() {
    let mut ui = Ui::start(Lang::En, false);
    ui.app().options.open = true;
    ui.run();
    for page in crate::options::Page::ALL {
        ui.harness.get_by_role_and_label(Role::Button, &page.label()).click();
        ui.run();
        assert_all_named(&ui, &format!("options, page {}", page.label()));
        assert_targets(&ui, &format!("options, page {}", page.label()));
    }
}

#[test]
fn site_manager_controls_are_named() {
    let mut ui = Ui::start(Lang::Fr, true);
    let site = urftp_core::site::Site { name: "Demo".into(), host: "ftp.example.com".into(), ..Default::default() };
    ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Demo").click();
    ui.run();
    assert!(ui.has_label(&t!("site.connection")), "the site form is shown");
    assert_all_named(&ui, "site manager");
}

#[test]
fn shortcuts_window_opens_with_f1_and_closes_with_escape() {
    let mut ui = Ui::start(Lang::En, true);
    ui.key(Key::F1);
    assert!(ui.has_label(&t!("shortcuts.title")));
    assert!(ui.has_label(&t!("shortcuts.f6")));
    ui.key(Key::Escape);
    assert!(!ui.has_label(&t!("shortcuts.title")));
}

#[test]
fn arrows_move_the_cursor_and_shift_extends_the_selection() {
    let mut ui = Ui::start(Lang::En, true);
    // Folders first, then names in order.
    assert_eq!(ui.left_rows(), ["docs", "alpha.txt", "beta.bin", "gamma.log"]);
    ui.key(Key::ArrowDown);
    assert_eq!(ui.left_selection(), ["docs"]);
    ui.key(Key::ArrowDown);
    assert_eq!(ui.left_selection(), ["alpha.txt"]);
    ui.key_with(Modifiers::SHIFT, Key::ArrowDown);
    assert_eq!(ui.left_selection(), ["alpha.txt", "beta.bin"]);
    ui.key_with(Modifiers::COMMAND, Key::Space);
    assert_eq!(ui.left_selection(), ["alpha.txt"], "Ctrl+Space unselects the row under the cursor");
    ui.key(Key::End);
    assert_eq!(ui.left_selection(), ["gamma.log"]);
    ui.key(Key::Home);
    assert_eq!(ui.left_selection(), ["docs"]);
    ui.key_with(Modifiers::COMMAND, Key::A);
    assert_eq!(ui.left_selection().len(), 4);
}

#[test]
fn enter_opens_a_folder_and_backspace_goes_back() {
    let mut ui = Ui::start(Lang::En, true);
    let home = ui.left_path();
    ui.key(Key::ArrowDown);
    ui.key(Key::Enter);
    assert_eq!(ui.left_path(), home.join("docs"));
    ui.key(Key::Backspace);
    assert_eq!(ui.left_path(), home);
}

#[test]
fn f6_switches_the_active_panel() {
    let mut ui = Ui::start(Lang::En, true);
    assert_eq!(ui.app().tabs[0].focus, 0);
    ui.key(Key::F6);
    assert_eq!(ui.app().tabs[0].focus, 1);
    ui.key(Key::ArrowDown);
    assert!(ui.left_selection().is_empty(), "the keys go to the right panel");
    ui.key_with(Modifiers::SHIFT, Key::F6);
    assert_eq!(ui.app().tabs[0].focus, 0);
}

#[test]
fn tab_reaches_the_controls_and_escape_gives_the_keys_back_to_the_list() {
    let mut ui = Ui::start(Lang::En, true);
    ui.key(Key::Tab);
    assert!(ui.widget_focused(), "Tab focuses a control");
    let first = ui.focused_label();
    ui.key(Key::Tab);
    assert_ne!(ui.focused_label(), first, "Tab moves to the next control");
    ui.key(Key::ArrowDown);
    assert!(ui.left_selection().is_empty(), "while a control has the focus, arrows do not move the list");
    ui.key(Key::Escape);
    assert!(!ui.widget_focused());
    ui.key(Key::ArrowDown);
    assert_eq!(ui.left_selection(), ["docs"]);
}

#[test]
fn delete_dialog_starts_on_cancel_and_follows_the_language() {
    for (lang, cancel) in [(Lang::En, "Cancel"), (Lang::Fr, "Annuler")] {
        let mut ui = Ui::start(lang, true);
        ui.key(Key::End);
        ui.key(Key::Delete);
        assert!(ui.has_label(&t!("prompt.delete")), "the confirmation is shown");
        assert_eq!(ui.focused_label().as_deref(), Some(cancel), "the safe choice has the focus");
        // Enter activates the focused button: nothing is deleted.
        ui.key(Key::Enter);
        assert!(!ui.has_label(&t!("prompt.delete")));
        assert!(scratch().join("home/gamma.log").exists());
    }
}

#[test]
fn new_folder_dialog_types_into_its_field() {
    let mut ui = Ui::start(Lang::En, true);
    ui.key(Key::F7);
    assert!(ui.widget_focused(), "the name field has the focus");
    ui.key_with(Modifiers::COMMAND, Key::A);
    ui.harness.event(egui::Event::Text("kbd-made".into()));
    ui.run();
    ui.key(Key::Enter);
    let made = scratch().join("home/kbd-made");
    assert!(made.is_dir(), "the folder was created from the keyboard");
    std::fs::remove_dir(&made).unwrap();
    // Escape closes without creating anything.
    ui.key(Key::F7);
    ui.key(Key::Escape);
    assert!(!ui.has_label(&t!("prompt.new_folder")));
}

#[test]
fn password_field_does_not_expose_its_value() {
    let mut ui = Ui::start(Lang::En, true);
    let label = t!("common.password");
    let field = ui.harness.get_by_role_and_label(Role::PasswordInput, &label);
    field.focus();
    field.type_text("hunter2");
    ui.run();
    assert_eq!(ui.app().quick.pass, "hunter2");
    let field = ui.harness.get_by_role(Role::PasswordInput);
    let exposed = field.accesskit_node().value().unwrap_or_default();
    assert!(!exposed.contains("hunter2"), "the password is masked for assistive technologies: {exposed:?}");
}

#[test]
fn transfer_button_is_disabled_without_selection() {
    let mut ui = Ui::start(Lang::En, true);
    let button = |ui: &Ui| ui.harness.get_by_label(&t!("files.transfer_right")).accesskit_node().is_disabled();
    assert!(button(&ui));
    ui.key(Key::ArrowDown);
    assert!(!button(&ui));
}

#[test]
fn tabs_open_switch_and_close_from_the_keyboard() {
    let mut ui = Ui::start(Lang::En, true);
    ui.key_with(Modifiers::COMMAND, Key::T);
    assert_eq!((ui.app().tabs.len(), ui.app().active), (2, 1));
    ui.key_with(Modifiers::COMMAND, Key::Tab);
    assert_eq!(ui.app().active, 0);
    ui.key_with(Modifiers::COMMAND | Modifiers::SHIFT, Key::Tab);
    assert_eq!(ui.app().active, 1);
    ui.key_with(Modifiers::COMMAND, Key::W);
    assert_eq!(ui.app().tabs.len(), 1);
    ui.key_with(Modifiers::COMMAND, Key::W);
    assert_eq!(ui.app().tabs.len(), 1, "the last tab stays");
}

#[test]
fn availability_test_marks_an_unreachable_site() {
    let mut ui = Ui::start(Lang::En, true);
    // A local port nobody listens on: the test fails quickly.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let site = urftp_core::site::Site { name: "Down".into(), host: "127.0.0.1".into(), port, ..Default::default() };
    let id = ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Down").click();
    ui.run();
    ui.harness.get_by_label(&t!("sites.test")).click();
    ui.run();
    let mut result = None;
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        ui.run();
        result = ui.app().sites.probes.lock().get(&id).cloned();
        if matches!(result, Some(crate::sites::Probe::Down(_))) {
            break;
        }
    }
    assert!(matches!(result, Some(crate::sites::Probe::Down(_))), "{result:?}");
    assert_eq!(ui.harness.query_all_by_label("✖").count(), 2, "the tree and the form show the failure");
}

#[test]
fn other_addresses_are_edited_one_per_line() {
    let mut ui = Ui::start(Lang::En, true);
    let site = urftp_core::site::Site { name: "Multi".into(), host: "a.example.com".into(), ..Default::default() };
    let id = ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Multi").click();
    ui.run();
    let label = t!("site.alt_hosts");
    let field = ui.harness.get_by_role_and_label(Role::MultilineTextInput, &label);
    field.focus();
    field.type_text("b.example.com:2121");
    ui.run();
    assert_eq!(ui.app().sites.book.find(id).map(|s| s.alt_hosts.clone()), Some(vec!["b.example.com:2121".to_string()]));
}

#[test]
fn shift_and_ctrl_pick_the_login_variant() {
    use crate::sites::LoginVariant;
    assert_eq!(LoginVariant::from_modifiers(Modifiers::NONE), LoginVariant::default());
    assert_eq!(LoginVariant::from_modifiers(Modifiers::SHIFT), LoginVariant { ghost: true, quiet: false });
    assert_eq!(LoginVariant::from_modifiers(Modifiers::COMMAND), LoginVariant { ghost: false, quiet: true });
}
