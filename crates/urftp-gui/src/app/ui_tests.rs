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
use urftp_core::queue::{Endpoint, ItemState, Queue};

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
        // A queue saved by a previous test would be offered at startup.
        let _ = std::fs::remove_file(urftp_core::queuestore::SavedQueues::default_path());
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

#[test]
fn site_own_settings_start_from_the_global_ones() {
    let mut ui = Ui::start(Lang::En, true);
    let site = urftp_core::site::Site { name: "Own".into(), host: "a.example.com".into(), ..Default::default() };
    let id = ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Own").click();
    ui.run();
    ui.harness.get_by_label(&t!("site.own_retries")).click_accesskit();
    ui.harness.get_by_label(&t!("site.upload_account")).click_accesskit();
    ui.run();
    let global = ui.app().options.settings.transfer.retries;
    let s = ui.app().sites.book.find(id).cloned().unwrap();
    assert_eq!(s.settings.retries, Some(global));
    assert!(s.settings.upload_account.is_some() && s.settings.download_account.is_none());
    assert_all_named(&ui, "site manager, own settings");
}

#[test]
fn shift_on_connect_asks_for_a_ghost_login() {
    let mut ui = Ui::start(Lang::En, true);
    // A local port nobody listens on: no network access.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let site = urftp_core::site::Site { name: "Ghost".into(), host: "127.0.0.1".into(), port, ..Default::default() };
    let id = ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Ghost").click();
    ui.run();
    ui.harness.get_by_label(&t!("sites.connect")).click_modifiers(Modifiers::SHIFT);
    ui.run();
    // The panel connects with the ghost login.
    let app = ui.app();
    let user = app.tabs.iter().flat_map(|t| t.sides.iter()).find_map(|s| match s {
        Side::Remote(r) if r.site_id == Some(id) => Some(r.profile.user.clone()),
        _ => None,
    });
    assert_eq!(user.as_deref(), Some("!anonymous"));
}

#[test]
fn new_sites_start_from_the_model_of_the_options() {
    let mut ui = Ui::start(Lang::En, true);
    let model = &mut ui.app().options.settings.new_site;
    model.user = "model-user".into();
    model.max_transfers = 7;
    model.listing.charset = "windows-1251".into();
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label(&t!("sites.new_site")).click();
    ui.run();
    let app = ui.app();
    let sites = app.sites.book.root.sites_recursive();
    let s = sites.iter().find(|s| s.name == t!("sites.new_site_default")).expect("new site");
    assert_eq!((s.user.as_str(), s.max_transfers, s.listing.charset.as_str()), ("model-user", 7, "windows-1251"));
    assert!(s.host.is_empty());
}

#[test]
fn listing_section_is_shown_for_ftp_only() {
    let mut ui = Ui::start(Lang::Fr, true);
    let ftp = urftp_core::site::Site { name: "Plain".into(), host: "ftp.example.com".into(), ..Default::default() };
    let sftp = urftp_core::site::Site {
        name: "Secure".into(),
        host: "ssh.example.com".into(),
        protocol: urftp_core::site::SiteProtocol::Sftp,
        ..Default::default()
    };
    ui.app().sites.book.add_site(&[], ftp);
    ui.app().sites.book.add_site(&[], sftp);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Plain").click();
    ui.run();
    assert!(ui.has_label(&t!("site.charset")), "FTP: listing settings shown");
    ui.harness.get_by_label_contains("Secure").click();
    ui.run();
    assert!(!ui.has_label(&t!("site.charset")), "SFTP: no listing settings");
}

#[test]
fn an_invalid_address_is_flagged_on_the_connection_page() {
    let mut ui = Ui::start(Lang::En, false);
    ui.app().options.settings.network.external_ip = "not-an-ip".into();
    ui.app().options.open = true;
    ui.run();
    ui.harness.get_by_role_and_label(Role::Button, &crate::options::Page::Connection.label()).click();
    ui.run();
    assert!(ui.harness.query_by_label_contains(&t!("options.invalid_ip")).is_some(), "warning shown");
}

#[test]
fn named_proxies_are_added_in_the_options_and_chosen_in_a_site() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().options.open = true;
    ui.run();
    ui.harness.get_by_role_and_label(Role::Button, &crate::options::Page::Proxy.label()).click();
    ui.run();
    ui.harness.get_by_label(&t!("options.add_proxy")).click_accesskit();
    ui.run();
    assert_eq!(ui.app().options.settings.proxies.len(), 1);
    assert_all_named(&ui, "options, named proxy");
    ui.app().options.open = false;

    let site = urftp_core::site::Site { name: "Behind".into(), host: "ftp.example.com".into(), ..Default::default() };
    let id = ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Behind").click();
    ui.run();
    ui.harness.get_by_label(&t!("site.proxy")).click_accesskit();
    ui.run();
    ui.harness.get_by_label("Proxy 1").click_accesskit();
    ui.run();
    let s = ui.app().sites.book.find(id).cloned().unwrap();
    assert_eq!(s.proxy, urftp_core::site::ProxyChoice::Named("Proxy 1".into()));
}

#[test]
fn sftp_record_with_several_keys_is_named() {
    let mut ui = Ui::start(Lang::Fr, true);
    let site = urftp_core::site::Site {
        name: "Keys".into(),
        host: "ssh.example.com".into(),
        protocol: urftp_core::site::SiteProtocol::Sftp,
        ssh_key_file: Some("a.ppk".into()),
        ssh_more_keys: vec!["b.ppk".into()],
        description: "Two keys".into(),
        ..Default::default()
    };
    ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Keys").click();
    ui.run();
    assert!(ui.harness.query_by_label(&t!("site.more_key", n = 2)).is_some());
    assert_all_named(&ui, "site manager, SFTP keys");
}

#[test]
fn unfinished_queues_of_several_tabs_are_offered_one_by_one() {
    use urftp_core::queuestore::SavedQueues;
    let mut ui = Ui::start(Lang::Fr, true);
    let mut first = Queue::new();
    first.push(Endpoint::remote("Demo", "/pub/a.bin"), Endpoint::local(r"C:\dl\a.bin"), false, Some(5), None);
    let id = first.push(Endpoint::remote("Demo", "/pub/b.bin"), Endpoint::local(r"C:\dl\b.bin"), false, Some(7), None);
    first.set_state(id, ItemState::Stopped);
    let mut second = Queue::new();
    second.push(Endpoint::local(r"C:\up\c.zip"), Endpoint::remote("Mirror", "/in/c.zip"), false, Some(9), None);
    let mut third = Queue::new();
    third.push(Endpoint::remote("Other", "/x.rar"), Endpoint::remote("Mirror", "/x.rar"), false, None, None);
    let path = SavedQueues::default_path();
    let three = [("demo.example.com".to_string(), &first), ("mirror.example.net".to_string(), &second), ("Onglet 3".to_string(), &third)];
    SavedQueues::snapshot(three.clone()).save(&path).unwrap();

    ui.app().load_saved_queue();
    ui.run();
    assert!(ui.has_label(&t!("queue.saved_title")), "the dialog is shown");
    // While the question is open, the file is left alone.
    ui.harness.step();
    assert!(path.exists());
    // Every queue is ticked: untick the second one, then load.
    ui.harness.get_by_label_contains("mirror.example.net").click();
    ui.run();
    ui.harness.get_by_label(&t!("queue.load_selected")).click();
    ui.run();
    let paths = |ui: &mut Ui, tab: usize| -> Vec<(String, ItemState)> {
        ui.app().tabs[tab].queue.lock().items().iter().map(|i| (i.src.path.clone(), i.state.clone())).collect()
    };
    assert_eq!(ui.app().tabs.len(), 2, "one tab per loaded queue");
    assert_eq!(paths(&mut ui, 0), [("/pub/a.bin".to_string(), ItemState::Waiting), ("/pub/b.bin".to_string(), ItemState::Waiting)]);
    assert_eq!(paths(&mut ui, 1), [("/x.rar".to_string(), ItemState::Waiting)]);
    // The file now holds the loaded queues only.
    ui.app().save_queue();
    let kept = SavedQueues::load(&path).unwrap().expect("saved");
    assert_eq!(kept.tabs.len(), 2);

    // Emptied queues: the file goes away at the next save.
    for t in &ui.app().tabs {
        t.queue.lock().remove_where(|_| true);
    }
    ui.app().save_queue();
    assert!(!path.exists());

    // "Discard everything": nothing is loaded and the file is removed.
    SavedQueues::snapshot(three).save(&path).unwrap();
    ui.app().load_saved_queue();
    ui.run();
    ui.harness.get_by_label(&t!("queue.discard_all")).click();
    ui.run();
    assert!(ui.app().tabs.iter().all(|t| t.queue.lock().items().is_empty()));
    assert!(!path.exists());
}

#[test]
fn about_window_shows_the_version() {
    let mut ui = Ui::start(Lang::Fr, false);
    ui.harness.get_by_label(&t!("menu.help")).click();
    ui.run();
    ui.harness.get_by_label(&t!("menu.about")).click();
    ui.run();
    assert!(ui.has_label(&t!("about.version", version = env!("CARGO_PKG_VERSION"))));
    ui.key(Key::Escape);
    assert!(!ui.has_label(&t!("about.version", version = env!("CARGO_PKG_VERSION"))));
}

/// Clicks the only enabled control with this name (each panel has the same buttons).
fn click_enabled(ui: &mut Ui, label: &str) {
    {
        let nodes: Vec<_> = ui.harness.get_all_by_label(label).filter(|n| !n.accesskit_node().is_disabled()).collect();
        assert_eq!(nodes.len(), 1, "enabled controls named {label}");
        nodes[0].click();
    }
    ui.run();
}

/// Remote panel of the first tab, from the connection bar of the left panel.
fn connect_left_panel(ui: &mut Ui, name: &str) -> u64 {
    // A local port nobody listens on: no network access.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let site = urftp_core::site::Site { name: name.into(), host: "127.0.0.1".into(), port, ..Default::default() };
    let book = &mut ui.app().sites.book;
    book.root.groups.push(urftp_core::site::SiteGroup::new("Lab"));
    let g = book.root.groups.len() - 1;
    let id = book.add_site(&[g], site);
    ui.harness.get_all_by_label(&t!("conn.connect")).next().unwrap().click();
    ui.run();
    // A group opens its sub-menu when the pointer is over it.
    ui.harness.get_by_label_contains("📁 Lab").hover();
    ui.run();
    ui.harness.get_by_label(&format!("🌐 {name}")).click();
    ui.run();
    id
}

#[test]
fn connection_bar_connects_the_panel_and_switches_to_local_and_back() {
    let mut ui = Ui::start(Lang::En, true);
    let id = connect_left_panel(&mut ui, "Bar site");
    let tab = &ui.app().tabs[0];
    assert!(matches!(&tab.sides[0], Side::Remote(r) if r.site_id == Some(id)), "left panel connected");
    assert!(matches!(tab.parked[0], Some(Side::Local(_))), "its local folder kept behind");
    assert!(matches!(tab.sides[1], Side::Local(_)), "right panel untouched");

    // To the local folder: the connection stays open behind it.
    click_enabled(&mut ui, &t!("conn.to_local"));
    let tab = &ui.app().tabs[0];
    assert!(matches!(tab.sides[0], Side::Local(_)));
    assert!(matches!(&tab.parked[0], Some(Side::Remote(r)) if r.site_id == Some(id)));
    assert_eq!(tab.title(1), "127.0.0.1", "the tab keeps the name of its connection");

    // And back to the connection.
    ui.harness.get_by_label(&t!("conn.to_remote", site = "Bar site")).click();
    ui.run();
    assert!(matches!(&ui.app().tabs[0].sides[0], Side::Remote(r) if r.site_id == Some(id)));
}

#[test]
fn disconnect_behind_the_local_folder_forgets_the_connection() {
    let mut ui = Ui::start(Lang::Fr, false);
    connect_left_panel(&mut ui, "Parked site");
    click_enabled(&mut ui, &t!("conn.to_local"));
    click_enabled(&mut ui, &t!("files.disconnect"));
    let tab = &ui.app().tabs[0];
    assert!(matches!(tab.sides[0], Side::Local(_)));
    assert!(tab.parked[0].is_none());
    assert!(tab.switch_label(0).is_none(), "nothing left to switch to");
}

#[test]
fn connection_menu_acts_on_the_active_panel() {
    let mut ui = Ui::start(Lang::En, true);
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let site = urftp_core::site::Site { name: "Menu site".into(), host: "127.0.0.1".into(), port, ..Default::default() };
    ui.app().sites.book.add_site(&[], site);
    ui.app().tabs[0].focus = 1;
    ui.harness.get_by_label(&t!("menu.connection")).click();
    ui.run();
    ui.harness.get_by_label("🌐 Menu site").click();
    ui.run();
    let tab = &ui.app().tabs[0];
    assert!(matches!(tab.sides[1], Side::Remote(_)), "right panel, the active one, connected");
    assert!(matches!(tab.sides[0], Side::Local(_)));
}

/// Runs an action of the interface, as a click would.
fn act(ui: &mut Ui, action: super::Action) {
    let ctx = ui.harness.ctx.clone();
    ui.app().apply(vec![action], &ctx);
}

/// Queue of the first tab with three downloads and an FXP item.
fn fill_queue(ui: &mut Ui) -> Vec<u64> {
    let mut q = ui.app().tabs[0].queue.lock();
    let mut ids: Vec<u64> = ["a", "b", "c"]
        .iter()
        .map(|n| {
            q.push(Endpoint::remote("Demo", format!("/pub/{n}.bin")), Endpoint::local(format!(r"C:\dl\{n}.bin")), false, Some(10), None)
        })
        .collect();
    ids.push(q.push(Endpoint::remote("Demo", "/rel/x"), Endpoint::remote("Mirror", "/in/x"), false, Some(5), None));
    ids
}

#[test]
fn queue_rows_are_selected_with_ctrl_and_shift_then_marked_failed() {
    let mut ui = Ui::start(Lang::En, true);
    let ids = fill_queue(&mut ui);
    let tab = &mut ui.app().tabs[0];
    tab.select_queue_row(ids[0], false, false);
    tab.select_queue_row(ids[2], false, true);
    assert_eq!(tab.queue_sel.iter().copied().collect::<Vec<_>>(), ids[..3]);
    tab.select_queue_row(ids[1], true, false);
    assert_eq!(tab.queue_sel.iter().copied().collect::<Vec<_>>(), [ids[0], ids[2]]);

    let sel: Vec<u64> = ui.app().tabs[0].queue_sel.iter().copied().collect();
    act(&mut ui, super::Action::QueueMarkFailed(sel));
    let states: Vec<bool> = ui.app().tabs[0].queue.lock().items().iter().map(|i| matches!(i.state, ItemState::Failed(_))).collect();
    assert_eq!(states, [true, false, true, false]);
    act(&mut ui, super::Action::QueueClearFailed);
    assert_eq!(ui.app().tabs[0].queue.lock().ids(), [ids[1], ids[3]]);
    assert!(ui.app().tabs[0].queue_sel.is_empty(), "removed rows leave the selection");
}

#[test]
fn queue_item_is_edited_and_fxp_inverted_in_the_dialog() {
    let mut ui = Ui::start(Lang::Fr, false);
    let ids = fill_queue(&mut ui);
    act(&mut ui, super::Action::QueueEdit(ids[3]));
    ui.run();
    assert!(ui.has_label(&t!("queue.edit_title")));
    ui.harness.get_by_label(&t!("queue.invert")).click();
    ui.run();
    ui.harness.get_by_label(&t!("common.validate")).click();
    ui.run();
    let it = ui.app().tabs[0].queue.lock().get(ids[3]).cloned().unwrap();
    assert_eq!((it.src.site.as_deref(), it.src.path.as_str(), it.dst.site.as_deref()), (Some("Mirror"), "/in/x", Some("Demo")));
    assert!(ui.app().tabs[0].queue_edit.is_none(), "the dialog closes");

    // Both ends local: refused, the dialog stays.
    act(&mut ui, super::Action::QueueEdit(ids[0]));
    ui.app().tabs[0].queue_edit.as_mut().unwrap().src_site.clear();
    ui.run();
    ui.harness.get_by_label(&t!("common.validate")).click();
    ui.run();
    assert!(ui.has_label(&t!("queue.edit_local_to_local")));
    assert_eq!(ui.app().tabs[0].queue.lock().get(ids[0]).unwrap().src.site.as_deref(), Some("Demo"));
}

#[test]
fn queue_file_round_trip_and_information_window() {
    let mut ui = Ui::start(Lang::En, true);
    fill_queue(&mut ui);
    let path = scratch().join("tab.urq");
    ui.app().save_queue_file(&path);
    ui.app().tabs[0].queue.lock().remove_where(|_| true);
    ui.app().load_queue_file(&path);
    assert_eq!(ui.app().tabs[0].queue.lock().items().len(), 4);
    ui.app().load_queue_file(&scratch().join("missing.urq"));
    assert_eq!(ui.app().tabs[0].queue.lock().items().len(), 4, "a missing file changes nothing");

    ui.harness.get_by_label(&t!("queue.more")).click();
    ui.run();
    ui.harness.get_by_label(&t!("queue.info")).click();
    ui.run();
    assert!(ui.has_label(&t!("queue.info_title")));
    assert!(ui.has_label(&t!("queue.info_items_value", files = 4, folders = 0)));
}

/// Last end-of-queue action asked from the system by the tests (never really run).
static POWER_ASKED: parking_lot::Mutex<Option<urftp_core::settings::QueueEnd>> = parking_lot::const_mutex(None);

fn fake_power(action: urftp_core::settings::QueueEnd) -> Result<(), String> {
    *POWER_ASKED.lock() = Some(action);
    Ok(())
}

#[test]
fn end_of_queue_asks_with_a_countdown_then_runs_or_is_cancelled() {
    use urftp_core::settings::QueueEnd;
    let mut ui = Ui::start(Lang::Fr, true);
    ui.app().power = fake_power;
    ui.app().options.settings.transfer.on_queue_end = QueueEnd::Sleep;
    ui.app().options.settings.transfer.queue_end_countdown_secs = 60;
    *POWER_ASKED.lock() = None;

    ui.app().tabs[0].just_finished = true;
    ui.run();
    assert!(ui.has_label(&t!("queue_end.title")));
    ui.harness.get_by_label(&t!("common.cancel")).click();
    ui.run();
    assert!(!ui.has_label(&t!("queue_end.title")));
    assert_eq!(*POWER_ASKED.lock(), None, "cancelled: nothing asked");

    ui.app().tabs[0].just_finished = true;
    ui.run();
    ui.harness.get_by_label(&t!("queue_end.now")).click();
    ui.run();
    assert_eq!(*POWER_ASKED.lock(), Some(QueueEnd::Sleep));

    // A queue still to do: nothing is asked.
    *POWER_ASKED.lock() = None;
    fill_queue(&mut ui);
    ui.app().tabs[0].just_finished = true;
    ui.run();
    assert!(!ui.has_label(&t!("queue_end.title")));
}

#[test]
fn end_of_queue_logs_out_and_logout_clears_the_waiting_items() {
    use urftp_core::settings::QueueEnd;
    let mut ui = Ui::start(Lang::En, true);
    connect_left_panel(&mut ui, "First");
    ui.app().options.settings.transfer.on_queue_end = QueueEnd::Disconnect;
    ui.app().tabs[0].just_finished = true;
    ui.run();
    assert!(matches!(ui.app().tabs[0].sides[0], Side::Local(_)), "logged out at the end of the queue");

    connect_left_panel(&mut ui, "Demo");
    ui.app().options.settings.transfer.clear_queue_on_logout = true;
    let ids = fill_queue(&mut ui);
    ui.app().tabs[0].queue.lock().set_state(ids[0], ItemState::Failed("x".into()));
    act(&mut ui, super::Action::Disconnect(0));
    assert_eq!(ui.app().tabs[0].queue.lock().ids(), [ids[0]], "only the waiting items of the site go away");
}

#[test]
fn script_transfer_spreads_tabs_and_carries_its_options() {
    let mut ui = Ui::start(Lang::En, true);
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    for name in ["FTP1", "FTP2", "FTP3"] {
        let site = urftp_core::site::Site { name: name.into(), host: "127.0.0.1".into(), port, ..Default::default() };
        ui.app().sites.book.add_site(&[], site);
    }
    let ctx = ui.harness.ctx.clone();
    // Two targets without RS_APPEND: one new tab each, run at the same time.
    let parallel = "begin RushApp.FTP.Transfer(0, 'FTP1', '/pub', 'rel', 'FTP2;FTP3', '/in', '', RS_DIRDES or RS_DIRSRC, \
                    '', 'Sample', '', '*.nfo', '', 0, 2, 0, 0, RS_SORTSIZE or RS_SORTDES, 3, 5); end.";
    ui.app().run_script(parallel, &ctx).unwrap();
    assert_eq!(ui.app().tabs.len(), 2);
    let item = ui.app().tabs[1].queue.lock().items()[0].clone();
    assert_eq!(item.dst.site.as_deref(), Some("FTP3"));
    let o = item.options.expect("options of the script");
    assert_eq!((o.depth, o.sub_count, o.retries, o.file_sort.descending), (Some(3), 5, Some(2), true));
    assert!(!o.filter.allows("Sample", true) && !o.filter.allows("x.nfo", false));

    // RS_APPEND and RS_NOCIRCLE: one tab, the second pair after the first.
    let before = ui.app().tabs.len();
    let ordered = "begin RushApp.FTP.Transfer(0, 'FTP1', '/pub', 'rel', 'FTP2;FTP3', '/in', '', \
                   RS_DIRDES or RS_DIRSRC or RS_APPEND or RS_NOCIRCLE or RS_LOGOUT, '', '', '', '', '', 0, 0, 0, 0, 0, 0, 0); end.";
    ui.app().run_script(ordered, &ctx).unwrap();
    assert_eq!(ui.app().tabs.len(), before, "no new tab");
    let t = ui.app().active;
    let seqs: Vec<Option<u64>> = ui.app().tabs[t].queue.lock().items().iter().rev().take(2).map(|i| i.sequence).collect();
    assert!(seqs[0].is_some() && seqs[1].is_some() && seqs[0] > seqs[1]);
    assert!(ui.app().tabs[t].logout_when_done);

    let bad =
        "begin RushApp.FTP.Transfer(0, 'FTP1', '/pub', 'rel', 'FTP2', '/in', '', RS_EXPR, '(', '', '', '', '', 0, 0, 0, 0, 0, 0, 0); end.";
    assert!(ui.app().run_script(bad, &ctx).is_err(), "an invalid expression is refused");
}

#[test]
fn conditional_transfer_filters_the_selection_and_saves_its_scheme() {
    let mut ui = Ui::start(Lang::En, true);
    connect_left_panel(&mut ui, "Cond site");
    // The right panel is local (alpha.txt 10 B, beta.bin 2 KiB, gamma.log, docs/).
    act(&mut ui, super::Action::SelectAll(1));
    act(&mut ui, super::Action::Conditional(1));
    ui.run();
    assert!(ui.has_label(&t!("cond.title")));
    {
        let form = &mut ui.app().tabs[0].condition.as_mut().unwrap().form;
        form.file_exclude = "*.log".into();
        form.size_range = true;
        form.min_kib = 1;
        form.name = "Big files".into();
    }
    ui.run();
    ui.harness.get_by_label(&t!("cond.save")).click();
    ui.run();
    assert_eq!(ui.app().options.settings.condition_schemes.len(), 1);

    // A date that cannot be read: the dialog stays with its error.
    ui.app().tabs[0].condition.as_mut().unwrap().form.date_between = true;
    ui.app().tabs[0].condition.as_mut().unwrap().form.date_from = "yesterday".into();
    ui.run();
    ui.harness.get_by_label(&t!("cond.add")).click();
    ui.run();
    assert!(ui.app().tabs[0].condition.as_ref().is_some_and(|d| d.error.is_some()));
    ui.app().tabs[0].condition.as_mut().unwrap().form.date_between = false;
    ui.run();
    ui.harness.get_by_label(&t!("cond.add")).click();
    ui.run();
    assert!(ui.app().tabs[0].condition.is_none());
    let queued: Vec<(String, bool)> = ui.app().tabs[0]
        .queue
        .lock()
        .items()
        .iter()
        .map(|i| (Path::new(&i.src.path).file_name().unwrap().to_string_lossy().into_owned(), i.options.is_some()))
        .collect();
    assert_eq!(queued, [("docs".to_string(), true), ("beta.bin".to_string(), true)], "folders kept, small and excluded files dropped");
}

#[test]
fn sfv_file_is_written_in_the_background_and_mode_is_edited() {
    let mut ui = Ui::start(Lang::En, true);
    // Right panel: the local demo folder; select alpha.txt and beta.bin.
    for name in ["alpha.txt", "beta.bin"] {
        let i = ui.app().tabs[0].sides[1].rows().iter().position(|r| r.name == name).unwrap();
        ui.app().tabs[0].sides[1].selected_mut().insert(i);
    }
    act(&mut ui, super::Action::Checksums(1, false));
    let sfv = scratch().join("home").join("home.sfv");
    for _ in 0..100 {
        ui.run();
        if sfv.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let entries = urftp_core::checksum::parse_sfv(&std::fs::read_to_string(&sfv).unwrap());
    assert_eq!(entries.get("alpha.txt"), Some(&crc32fast::hash(&[b'x'; 10])));
    assert_eq!(entries.len(), 2);
    std::fs::remove_file(&sfv).unwrap();

    let ids = fill_queue(&mut ui);
    act(&mut ui, super::Action::QueueEdit(ids[0]));
    ui.app().tabs[0].queue_edit.as_mut().unwrap().mode = urftp_core::TransferMode::Ascii;
    ui.run();
    ui.harness.get_by_label(&t!("common.validate")).click();
    ui.run();
    assert_eq!(ui.app().tabs[0].queue.lock().get(ids[0]).unwrap().mode, urftp_core::TransferMode::Ascii);
}

#[test]
fn a_new_folder_is_entered_when_asked() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().options.settings.transfer.enter_new_folder = true;
    ui.key(Key::F7);
    ui.key_with(Modifiers::COMMAND, Key::A);
    ui.harness.event(egui::Event::Text("entered".into()));
    ui.run();
    ui.key(Key::Enter);
    let made = scratch().join("home/entered");
    assert_eq!(ui.left_path(), made, "the panel is now in the new folder");
    std::fs::remove_dir(&made).unwrap();
}

// ---------------------------------------------------------------- block I

/// Puts panel `side` of the first tab in a new scratch folder holding `files` (name, size).
fn panel_in(ui: &mut Ui, side: usize, dir: &str, files: &[(&str, usize)]) -> PathBuf {
    let path = scratch().join(dir);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    for (name, len) in files {
        if let Some(sub) = name.strip_suffix('/') {
            std::fs::create_dir_all(path.join(sub)).unwrap();
        } else {
            std::fs::write(path.join(name), vec![b'x'; *len]).unwrap();
        }
    }
    if let Side::Local(l) = &mut ui.app().tabs[0].sides[side] {
        l.path = path.clone();
        l.reload();
    }
    ui.run();
    path
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

fn op(ui: &mut Ui, side: usize, op: super::Op) {
    act(ui, super::Action::Op(side, op));
    ui.run();
}

#[test]
fn quick_filter_keeps_matching_names_and_f5_brings_everything_back() {
    let mut ui = Ui::start(Lang::En, true);
    ui.key_with(Modifiers::COMMAND, Key::F);
    assert!(ui.app().tabs[0].sides[0].list().filter_open, "the filter bar is open");
    assert!(ui.has_label(&t!("ops.filter")), "and named");
    ui.harness.event(egui::Event::Text("a -bin".into()));
    ui.run();
    assert_eq!(ui.left_rows(), ["alpha.txt", "gamma.log"], "words kept, -word left out, folders filtered too");
    ui.key(Key::Escape);
    assert_eq!(ui.left_rows().len(), 4, "Escape closes the filter");

    ui.key_with(Modifiers::COMMAND, Key::F);
    ui.harness.event(egui::Event::Text("*.log".into()));
    ui.run();
    assert_eq!(ui.left_rows(), ["gamma.log"]);
    // Back to the list, then F5.
    ui.harness.ctx.memory_mut(|m| m.stop_text_input());
    ui.harness.ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    ui.run();
    ui.key(Key::F5);
    assert_eq!(ui.left_rows().len(), 4);
    assert!(!ui.app().tabs[0].sides[0].list().filter_open);
}

#[test]
fn marks_select_by_mask_and_copy_names() {
    let mut ui = Ui::start(Lang::Fr, false);
    ui.key(Key::Home);
    ui.key_with(Modifiers::COMMAND, Key::M);
    let first = ui.left_rows()[0].clone();
    assert!(ui.app().tabs[0].sides[0].list().marks.contains(&first), "the row under the cursor is marked");
    ui.key(Key::End);
    op(&mut ui, 0, super::Op::SelectMarked);
    assert_eq!(ui.left_selection(), std::slice::from_ref(&first));

    ui.key(Key::Plus);
    assert!(matches!(ui.app().tabs[0].tools.dialog, Some(super::fileops::Dialog::Mask { select: true, .. })));
    if let Some(super::fileops::Dialog::Mask { masks, .. }) = &mut ui.app().tabs[0].tools.dialog {
        *masks = "*.log; *.bin".into();
    }
    ui.run();
    ui.harness.get_by_label(&t!("common.validate")).click();
    ui.run();
    let mut sel = ui.left_selection();
    sel.sort();
    assert_eq!(sel, ["beta.bin", "docs", "gamma.log"]);

    op(&mut ui, 0, super::Op::UnmarkAll);
    assert!(ui.app().tabs[0].sides[0].list().marks.is_empty());
}

#[test]
fn several_local_files_are_renamed_with_a_counter() {
    let mut ui = Ui::start(Lang::En, true);
    let dir = panel_in(&mut ui, 0, "rename", &[("a.TXT", 1), ("b.txt", 1)]);
    ui.key_with(Modifiers::COMMAND, Key::A);
    op(&mut ui, 0, super::Op::RenameMany);
    if let Some(super::fileops::Dialog::Rename { form, .. }) = &mut ui.app().tabs[0].tools.dialog {
        form.template = "%c-%n.%e".into();
        form.case = urftp_core::NameCase::Lower;
    } else {
        panic!("no rename dialog");
    }
    ui.run();
    assert!(ui.has_label("01-a.txt"), "the preview shows the new names");
    ui.harness.get_by_label(&t!("common.validate")).click();
    ui.run();
    assert_eq!(names(&dir), ["01-a.txt", "02-b.txt"]);
}

#[test]
fn local_items_are_moved_into_a_folder() {
    let mut ui = Ui::start(Lang::En, true);
    let dir = panel_in(&mut ui, 0, "move", &[("x.bin", 3), ("y.bin", 3), ("into/", 0)]);
    let rows = ui.left_rows();
    for (i, n) in rows.iter().enumerate() {
        if n.ends_with(".bin") {
            ui.app().tabs[0].sides[0].selected_mut().insert(i);
        }
    }
    op(&mut ui, 0, super::Op::MoveTo);
    if let Some(super::fileops::Dialog::Move { target, .. }) = &mut ui.app().tabs[0].tools.dialog {
        *target = "into".into();
    }
    ui.run();
    ui.harness.get_by_label(&t!("common.validate")).click();
    ui.run();
    assert_eq!(names(&dir), ["into"]);
    assert_eq!(names(&dir.join("into")), ["x.bin", "y.bin"]);
}

#[test]
fn comparison_hides_the_same_items_and_selects_the_others() {
    let mut ui = Ui::start(Lang::En, true);
    panel_in(&mut ui, 0, "cmp-l", &[("same.bin", 5), ("size.bin", 5), ("only-left.txt", 1)]);
    panel_in(&mut ui, 1, "cmp-r", &[("same.bin", 5), ("size.bin", 9), ("only-right.txt", 1)]);
    ui.app().tabs[0].tools.compare =
        super::fileops::CompareForm { by: urftp_core::browsing::CompareBy::NameSize, partial: false, hide_same: true, select: true };
    op(&mut ui, 0, super::Op::Compare);
    let right: Vec<String> = ui.app().tabs[0].sides[1].rows().iter().map(|r| r.name.clone()).collect();
    assert_eq!(ui.left_rows(), ["only-left.txt", "size.bin"], "same.bin hidden");
    assert_eq!(right, ["only-right.txt", "size.bin"]);
    let mut sel = ui.left_selection();
    sel.sort();
    assert_eq!(sel, ["only-left.txt", "size.bin"], "what differs is selected");
    ui.key(Key::F5);
    assert_eq!(ui.left_rows().len(), 3, "F5 shows everything again");
}

#[test]
fn shift_double_click_opens_the_folder_in_both_panels() {
    let mut ui = Ui::start(Lang::En, true);
    let home = scratch().join("home");
    op(&mut ui, 0, super::Op::OpenBoth("docs".into()));
    let right = match &ui.app().tabs[0].sides[1] {
        Side::Local(l) => l.path.clone(),
        Side::Remote(_) => PathBuf::new(),
    };
    assert_eq!(ui.left_path(), home.join("docs"));
    assert_eq!(right, home.join("docs"));
}

/// Waits for the walk windows of the first tab to finish.
fn wait_walks(ui: &mut Ui) {
    for _ in 0..200 {
        ui.run();
        if ui.app().tabs[0].tools.walks_done() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the walk never finished");
}

#[test]
fn space_used_and_directory_tree_of_a_local_folder() {
    let mut ui = Ui::start(Lang::En, true);
    let dir = panel_in(&mut ui, 0, "walk", &[("a/", 0), ("top.bin", 100)]);
    std::fs::write(dir.join("a").join("in.bin"), vec![0u8; 50]).unwrap();
    op(&mut ui, 0, super::Op::SpaceUsed);
    ui.harness.get_by_label(&t!("common.validate")).click();
    wait_walks(&mut ui);
    assert!(ui.has_label(&t!("ops.space_total", files = 2, folders = 1, size = crate::theme::human_size(150))), "totals shown");

    op(&mut ui, 0, super::Op::ExportTree);
    if let Some(super::fileops::Dialog::Walk { tree_opts, .. }) = &mut ui.app().tabs[0].tools.dialog {
        tree_opts.indent = "-".into();
    }
    ui.run();
    ui.harness.get_by_label(&t!("common.validate")).click();
    wait_walks(&mut ui);
    let text = ui.app().tabs[0].tools.last_tree_text();
    assert!(text.ends_with("-a/\n--in.bin\n-top.bin\n"), "{text}");
}

// ---------------------------------------------------------------- block J

#[test]
fn logs_are_split_and_written_to_daily_files() {
    let mut ui = Ui::start(Lang::En, true);
    let dir = scratch().join("logs-test");
    let _ = std::fs::remove_dir_all(&dir);
    let settings = urftp_core::settings::InterfaceSettings {
        log_to_disk: true,
        log_dir: dir.to_string_lossy().into_owned(),
        log_lines: 100,
        ..Default::default()
    };
    super::logs::configure(&settings);
    let tab = &mut ui.app().tabs[0];
    tab.push_ftp(0, super::LogKind::Command, "PWD");
    tab.push_transfer(0, super::LogKind::Status, "queue done");
    tab.push_log(0, super::LogKind::Error, "disk full");
    for i in 0..150 {
        tab.push_ftp(1, super::LogKind::Reply, format!("line {i}"));
    }
    let channels: Vec<_> = tab.log.iter().map(|e| e.channel).collect();
    assert!(channels.iter().filter(|c| **c == super::LogChannel::Ftp).count() <= 100, "each log keeps its own number of lines");
    assert_eq!(channels.iter().filter(|c| **c == super::LogChannel::Transfer).count(), 1, "the other logs are not trimmed");
    super::logs::flush().unwrap();
    let files = names(&dir);
    assert_eq!(files.len(), 3, "{files:?}");
    let ftp = files.iter().find(|f| f.ends_with("-ftp.log")).unwrap();
    let text = std::fs::read_to_string(dir.join(ftp)).unwrap();
    assert!(text.contains("PWD") && text.contains("line 149"), "{text}");
    super::logs::configure(&urftp_core::settings::InterfaceSettings::default());
    std::fs::remove_dir_all(&dir).unwrap();
}
