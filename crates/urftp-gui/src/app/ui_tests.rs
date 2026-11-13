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

use super::{Action, Pane, Side, UltraRushApp, tasks};
use crate::cmdline::StartupOptions;
use urftp_core::queue::{Endpoint, ItemState, Queue};
use urftp_proto::session::SiteProfile;

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
        // Tasks of a previous test would run or be listed.
        let _ = std::fs::remove_file(urftp_core::tasks::TaskBook::default_path());
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
    assert!(ui.has_label(&t!("keys.switch_panel")) && ui.has_label("F6, Shift+F6"), "commands listed with their keys");
    assert!(ui.has_label(&t!("shortcuts.title")));
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
    // Screen readers hear the row under the cursor (live region), the focus stays put.
    let told: Vec<String> = ui
        .harness
        .root()
        .children_recursive()
        .filter(|n| n.accesskit_node().role() == Role::Status)
        .filter_map(|n| n.accesskit_node().label())
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].starts_with("alpha.txt, selected, ") && told[0].ends_with("2 of 4"), "{}", told[0]);
    assert!(!ui.widget_focused());
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
    let site = urftp_core::site::Site { name: "Unreachable".into(), host: "127.0.0.1".into(), port, ..Default::default() };
    let id = ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.harness.get_by_label_contains("Unreachable").click();
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
    // The result may have come after the last frame: draw it.
    ui.run();
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
    assert_eq!(tab.title(1), "Bar site", "the tab keeps the name of the site of its connection");

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
static POWER_ASKED: parking_lot::Mutex<Option<urftp_core::settings::QueueEnd>> = parking_lot::Mutex::new(None);

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

#[test]
fn raw_dialog_opens_with_ctrl_r_and_site_commands_fill_it() {
    let mut ui = Ui::start(Lang::En, true);
    ui.key_with(Modifiers::COMMAND, Key::R);
    assert!(ui.app().raw.open && ui.has_label(&t!("raw.title")));
    assert_all_named(&ui, "RAW dialog");
    ui.app().raw.open = false;
    let i = ui.app().options.settings.site_commands.iter().position(|c| c.lines.contains("%1")).unwrap();
    act(&mut ui, super::Action::SiteCommand(i));
    assert!(ui.app().raw.open);
    assert_eq!(ui.app().raw.text, "SITE SEARCH %1", "a command with a parameter is put in the dialog");
    // Nothing connected: the command is refused with a message.
    assert_eq!(ui.app().send_raw("SITE WHO", super::raw::RawTarget::All), 0);
    assert!(ui.app().tabs[0].log.iter().any(|e| e.text == t!("raw.no_connection")));
}

#[test]
fn recent_sites_quick_site_and_manual_download() {
    let mut ui = Ui::start(Lang::Fr, true);
    let id = connect_left_panel(&mut ui, "Recent site");
    assert_eq!(ui.app().options.settings.recent_sites.first(), Some(&id), "the connected site comes first");

    ui.app().quick.host = "ftp.example.org".into();
    ui.app().quick.user = "bob".into();
    ui.run();
    ui.harness.get_by_label(&t!("fav.add_quick")).click();
    ui.run();
    assert!(ui.app().sites.book.find_by_name("bob@ftp.example.org:21").is_some(), "quick connection saved as a site");

    // A port nobody listens on: the queue fails, but the items and their site are there.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let target = scratch().join("manual");
    let ctx = ui.harness.ctx.clone();
    let n = ui.app().queue_addresses(&format!("ftp://127.0.0.1:{port}/pub/f[1-3].bin"), &target, &ctx);
    assert_eq!(n, 3);
    let tab = &ui.app().tabs[0];
    let site = format!("anonymous@127.0.0.1:{port}");
    assert!(tab.adhoc.iter().any(|p| p.name == site), "profile kept for the engine");
    let items = tab.queue.lock().items().iter().map(|i| (i.src.site.clone().unwrap_or_default(), i.src.path.clone())).collect::<Vec<_>>();
    assert!(items.contains(&(site.clone(), "/pub/f2.bin".into())), "{items:?}");
}

#[test]
fn chosen_keys_replace_the_defaults_and_busy_tabs_ask_before_closing() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().options.settings.interface.shortcuts.insert("filter".into(), "Alt+F".into());
    ui.key_with(Modifiers::COMMAND, Key::F);
    assert!(!ui.app().tabs[0].sides[0].list().filter_open, "Ctrl+F no longer filters");
    ui.key_with(Modifiers::ALT, Key::F);
    assert!(ui.app().tabs[0].sides[0].list().filter_open, "Alt+F does");

    // The first tab has waiting items: closing it is asked first.
    ui.app().new_tab();
    fill_queue(&mut ui);
    ui.run();
    ui.app().request_close_tab(0);
    ui.run();
    assert!(matches!(ui.app().confirm, Some(super::confirm::Confirm::CloseTab(_))));
    let before = ui.app().tabs.len();
    ui.harness.get_by_label(&t!("confirm.close")).click();
    ui.run();
    assert_eq!(ui.app().tabs.len(), before - 1);

    // Queue removals wait for the answer when asked for.
    ui.app().active = 0;
    ui.app().options.settings.interface.confirm_queue_remove = true;
    let ids = fill_queue(&mut ui);
    act(&mut ui, super::Action::QueueRemove(ids[0]));
    assert!(ui.app().tabs[0].queue.lock().get(ids[0]).is_some(), "not removed yet");
    ui.run();
    ui.harness.get_by_label(&t!("confirm.remove")).click();
    ui.run();
    assert!(ui.app().tabs[0].queue.lock().get(ids[0]).is_none());
}

#[test]
fn events_show_a_pop_up_and_go_to_the_system_log() {
    let mut ui = Ui::start(Lang::En, true);
    use urftp_core::settings::NotifyEvent;
    ui.app().tabs[0].events.push((NotifyEvent::QueueFinished, "3 item(s) done".into()));
    ui.app().tabs[0].events.push((NotifyEvent::Connected, "Demo".into()));
    ui.run();
    assert!(ui.has_label("3 item(s) done"), "pop-up in the window (no tray icon in the tests)");
    assert!(!ui.has_label("Demo"), "no pop-up for a connection by default");
    let log: Vec<String> = ui.app().tabs[0].log.iter().filter(|e| e.channel == super::LogChannel::System).map(|e| e.text.clone()).collect();
    assert!(log.iter().any(|l| l.ends_with("3 item(s) done")) && log.iter().any(|l| l.ends_with("Demo")), "{log:?}");
}

#[test]
fn layouts_are_saved_and_given_to_new_tabs_and_sort_options_apply() {
    let mut ui = Ui::start(Lang::En, true);
    // A layout with the queue alone at the bottom.
    let mut dock = egui_dock::DockState::new(vec![super::Pane::Left]);
    let surface = dock.main_surface_mut();
    let [top, _] = surface.split_below(egui_dock::NodeIndex::root(), 0.7, vec![super::Pane::Queue]);
    surface.split_right(top, 0.5, vec![super::Pane::Right]);
    ui.app().tabs[0].dock = dock;
    ui.app().save_layout("Queue only");
    assert!(urftp_core::sitestore::data_dir().join("layouts.json").exists());
    ui.app().options.settings.interface.default_layout = "Queue only".into();
    ui.app().new_tab();
    ui.run();
    let tabs: Vec<super::Pane> = ui.app().tabs.last().unwrap().dock.iter_all_tabs().map(|(_, t)| *t).collect();
    assert!(!tabs.contains(&super::Pane::Log), "the new tab took the saved layout: {tabs:?}");
    ui.app().layouts.clear();
    let _ = std::fs::remove_file(urftp_core::sitestore::data_dir().join("layouts.json"));

    // Largest first on the first click of the size column.
    let s = urftp_core::settings::InterfaceSettings { sort_desc_first: true, dirs_first: false, ..Default::default() };
    super::look::configure(&s);
    ui.app().active = 0;
    act(&mut ui, super::Action::Sort(0, super::SortCol::Size));
    let rows = ui.left_rows();
    super::look::configure(&urftp_core::settings::InterfaceSettings::default());
    assert_eq!(rows.first().map(String::as_str), Some("beta.bin"), "{rows:?}");
}

#[test]
fn master_password_guards_the_site_manager_and_the_lock() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().options.settings.master_hash = urftp_core::master::hash_password("open sesame").unwrap();
    ui.app().sites.open = true;
    ui.run();
    assert!(!ui.app().sites.open && ui.has_label(&t!("lock.prompt")), "the password is asked first");
    ui.app().unlock.as_mut().unwrap().password = "wrong".into();
    ui.run();
    ui.harness.get_by_label(&t!("lock.unlock")).click();
    ui.run();
    assert!(ui.has_label(&t!("lock.wrong")) && !ui.app().sites.open);
    ui.app().unlock.as_mut().unwrap().password = "open sesame".into();
    ui.run();
    ui.harness.get_by_label(&t!("lock.unlock")).click();
    ui.run();
    assert!(ui.app().sites.open, "unlocked: the site manager opens");

    // List mode of the site manager, with every control named.
    ui.harness.get_by_label(&t!("sites.list_mode")).click();
    ui.run();
    assert_all_named(&ui, "site manager, list mode");
    ui.app().sites.open = false;
    ui.run();

    // Locked window: no Cancel, the password alone brings it back.
    let ctx = ui.harness.ctx.clone();
    ui.app().lock(&ctx);
    ui.run();
    assert!(ui.has_label(&t!("lock.locked")) && !ui.has_label(&t!("common.cancel")));
    ui.key_with(Modifiers::COMMAND, Key::T);
    assert_eq!(ui.app().tabs.len(), 1, "no shortcut while locked");
    ui.app().unlock.as_mut().unwrap().password = "open sesame".into();
    ui.run();
    ui.harness.get_by_label(&t!("lock.unlock")).click();
    ui.run();
    assert!(ui.app().unlock.is_none());
}

/// Frames until no script runs any more (a few seconds at most).
fn wait_scripts(ui: &mut Ui) {
    for _ in 0..500 {
        if ui.app().scripts.running() == 0 {
            ui.run();
            return;
        }
        ui.harness.step();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the scripts did not finish: {:?}", ui.app().tabs[0].script_out);
}

/// Frames until a label is shown (a few seconds at most), then a few more: a dialog is
/// centred once its size is known.
fn wait_label(ui: &mut Ui, label: &str) {
    for _ in 0..500 {
        if ui.has_label(label) {
            ui.run();
            return;
        }
        ui.harness.step();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("\"{label}\" not shown");
}

#[test]
fn console_scripts_run_beside_the_interface_and_ask_for_input() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().tabs[0].script = "procedure SetSplit(v);
         begin
           RushApp.FTP.Splitter := v;
         end;
         s := 'x';
         if InputQuery('Bookmark', 'Name of the bookmark', s) then SetSplit(s + '|');"
        .into();
    act(&mut ui, super::Action::RunScript);
    wait_label(&mut ui, "Name of the bookmark");
    let id = ui.app().tabs[0].id;
    assert!(ui.app().scripts.console_running(id));
    assert_all_named(&ui, "script input");
    ui.harness.get_by_label(&t!("common.validate")).click();
    wait_scripts(&mut ui);
    assert_eq!(ui.app().splitter, "x|", "the answer reaches the script");
    assert_eq!(ui.app().tabs[0].script_out.last(), Some(&(true, t!("script.done"))));

    // A program to start: asked, refused, ShellExecuteCommand returns False.
    ui.app().tabs[0].script = "if not ShellExecuteCommand('urftp-test-tool.exe', '-x', '', 0) then RushApp.FTP.Splitter := 'no';".into();
    act(&mut ui, super::Action::RunScript);
    wait_label(&mut ui, &t!("script.shell_title"));
    assert!(ui.has_label("urftp-test-tool.exe"));
    assert_all_named(&ui, "program confirmation");
    ui.harness.get_by_label(&t!("script.shell_refuse")).click();
    wait_scripts(&mut ui);
    assert_eq!(ui.app().splitter, "no");
    // Forbidden by the options: not even asked.
    ui.app().options.settings.script_shell = urftp_core::settings::ShellPolicy::Deny;
    ui.app().splitter = ";".into();
    act(&mut ui, super::Action::RunScript);
    wait_scripts(&mut ui);
    assert_eq!(ui.app().splitter, "no");
    assert!(!ui.has_label(&t!("script.shell_title")));

    // A script that does not end is stopped from the console.
    ui.app().tabs[0].script = "while true do RushApp.FTP.Splitter := ';';".into();
    act(&mut ui, super::Action::RunScript);
    ui.run();
    let id = ui.app().tabs[0].id;
    assert!(ui.app().scripts.console_running(id), "still running");
    act(&mut ui, super::Action::StopScript);
    wait_scripts(&mut ui);
    let (ok, msg) = ui.app().tabs[0].script_out.last().cloned().unwrap();
    assert!(!ok && msg.contains("stopped"), "{msg}");
}

#[test]
fn scripts_read_the_selection_match_texts_and_answer_the_layout_event() {
    let mut ui = Ui::start(Lang::En, true);
    let ctx = ui.harness.ctx.clone();
    // The right panel is local: select everything in it and make it active.
    act(&mut ui, super::Action::SelectAll(1));
    ui.app().tabs[0].focus = 1;
    let rows = ui.app().tabs[0].sides[1].rows().len();
    ui.app()
        .run_script(
            "RushApp.UI.Selection.Prepare;
             s := IntToStr(RushApp.UI.Selection.Count);
             if not RushApp.UI.Selection.IsRemoteSide then s := s + ',local';
             for i := 0 to RushApp.UI.Selection.Count - 1 do
               if RushApp.UI.Selection.Items[i].IsFolder then s := s + ',' + RushApp.UI.Selection.Items[i].Name;
             RushApp.FTP.Splitter := s;",
            &ctx,
        )
        .unwrap();
    assert_eq!(ui.app().splitter, format!("{rows},local,docs"));
    assert_eq!(ui.app().invoke("MatchString('Release.Name-GRP', '*name*')", &ctx).unwrap(), "True");
    assert_eq!(ui.app().invoke("MatchExpr('allen', '^al+en$')", &ctx).unwrap(), "True");
    assert_eq!(ui.app().invoke("MatchExpr('bob', '[allen]')", &ctx).unwrap(), "False");
    assert!(ui.app().invoke("MatchExpr('x', '(')", &ctx).is_err());
    assert!(ui.app().invoke("RushApp.UI.Bar", &ctx).unwrap_err().to_string().contains(&t!("host.ui_undocumented")));

    ui.app()
        .run_script(
            "procedure Switched(Sender);
             begin
               RushApp.FTP.Splitter := 'layout';
             end;
             RushApp.UI.OnSwitchLayout := Switched;",
            &ctx,
        )
        .unwrap();
    assert_eq!(ui.app().splitter, format!("{rows},local,docs"), "not run when it is set");
    ui.app().fire_switch_layout(&ctx);
    wait_scripts(&mut ui);
    assert_eq!(ui.app().splitter, "layout");
    ui.app().run_script("RushApp.UI.OnSwitchLayout := nil;", &ctx).unwrap();
    assert!(ui.app().scripts.on_switch_layout.is_none());
}

/// A site of the site manager on a local port nobody listens on.
fn unreachable_profile(ui: &mut Ui, name: &str) -> SiteProfile {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let site = urftp_core::site::Site { name: name.into(), host: "127.0.0.1".into(), port, ..Default::default() };
    ui.app().site_profile(&site)
}

#[test]
fn new_tab_of_current_keeps_the_connections_and_folders() {
    let mut ui = Ui::start(Lang::En, true);
    let id = connect_left_panel(&mut ui, "Dup site");
    let right = match &ui.app().tabs[0].sides[1] {
        Side::Local(l) => l.path.clone(),
        Side::Remote(_) => panic!("right panel local"),
    };
    ui.key_with(Modifiers::COMMAND | Modifiers::SHIFT, Key::T);
    assert_eq!((ui.app().tabs.len(), ui.app().active), (2, 1));
    let copy = &ui.app().tabs[1];
    assert!(matches!(&copy.sides[0], Side::Remote(r) if r.site_id == Some(id) && r.profile.name == "Dup site"));
    assert!(matches!(&copy.sides[1], Side::Local(l) if l.path == right));
    assert!(ui.app().tabs[1].queue.lock().items().is_empty(), "the queue is not copied");
}

#[test]
fn window_list_shows_the_tabs_and_switches_to_one() {
    let mut ui = Ui::start(Lang::En, true);
    ui.key_with(Modifiers::COMMAND, Key::T);
    assert_eq!(ui.app().active, 1);
    ui.harness.get_by_label(&t!("menu.view")).click();
    ui.run();
    ui.harness.get_by_label(&t!("pane.windows")).click();
    ui.run();
    assert!(ui.app().pane_shown(Pane::Windows));
    let first = t!("tabs.default_title", n = 1);
    assert!(ui.has_label(&t!("windows.action_idle")));
    ui.harness.get_by_label(&t!("windows.show_named", title = first)).click();
    ui.run();
    assert_eq!(ui.app().active, 0);
    assert!(!ui.app().pane_shown(Pane::Windows), "the first tab has its own layout");
    ui.app().toggle_pane(Pane::Tasks);
    assert!(ui.app().pane_shown(Pane::Tasks));
    ui.run();
    assert!(ui.has_label(&t!("tasks.none")));
    ui.app().toggle_pane(Pane::Tasks);
    assert!(!ui.app().pane_shown(Pane::Tasks));
}

#[test]
fn a_scheduled_queue_runs_in_a_new_tab_when_its_time_comes() {
    use urftp_core::tasks::LastResult;
    let mut ui = Ui::start(Lang::En, true);
    // An item whose site does not exist: it fails at once, without network.
    ui.app().tabs[0].queue.lock().append([(
        Endpoint { site: Some("Nowhere".into()), path: "/a.bin".into() },
        Endpoint::local("C:/nowhere/a.bin"),
        false,
        Some(10),
        None,
        ItemState::Waiting,
    )]);
    let ctx = ui.harness.ctx.clone();
    ui.app().task_action(Action::ScheduleQueue, &ctx);
    ui.run();
    assert!(ui.has_label(&t!("tasks.new_title")));
    ui.harness.get_by_label(&t!("common.ok")).click();
    ui.run();
    assert!(!ui.app().task_dialog_open());
    let task = ui.app().tasks.book.tasks.first().cloned().unwrap();
    assert!(task.enabled && task.next_run.is_some());
    assert!(matches!(&task.action, urftp_core::tasks::TaskAction::Queue { queue, close_tab: true } if queue.items.len() == 1));

    // Its time comes.
    let now = tasks::now_local();
    ui.app().tasks.book.tasks[0].next_run = Some(now - 5);
    ui.run();
    assert_eq!((ui.app().tabs.len(), ui.app().active), (2, 0), "a new tab, the tab shown does not change");
    assert_eq!(ui.app().tabs[1].queue.lock().items().len(), 1);
    let task = &ui.app().tasks.book.tasks[0];
    assert_eq!((task.enabled, task.last_result.clone(), task.next_run), (false, Some(LastResult::Started), None));
    let saved = urftp_core::tasks::TaskBook::load(&urftp_core::tasks::TaskBook::default_path()).unwrap();
    assert_eq!(saved.tasks.len(), 1, "written to tasks.json");
}

#[test]
fn both_panels_remote_puts_the_second_site_on_the_left() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().options.settings.new_tab_both_remote = true;
    let ctx = ui.harness.ctx.clone();
    for name in ["First", "Second", "Third"] {
        let p = unreachable_profile(&mut ui, name);
        ui.app().connect_profile(p, None, None, &ctx);
        ui.run();
    }
    let tab = &ui.app().tabs[0];
    assert!(matches!(&tab.sides[1], Side::Remote(r) if r.profile.name == "First"));
    assert!(matches!(&tab.sides[0], Side::Remote(r) if r.profile.name == "Second"));
    assert_eq!(ui.app().tabs.len(), 2, "both panels busy: a new tab");
    assert!(matches!(&ui.app().tabs[1].sides[1], Side::Remote(r) if r.profile.name == "Third"));
}

#[test]
fn window_list_task_window_and_task_dialog_controls_are_named() {
    use urftp_core::tasks::{Schedule, Task, TaskAction};
    let mut ui = Ui::start(Lang::Fr, false);
    let now = tasks::now_local();
    let task = Task::new("Nightly", Schedule::Daily { minute: 120, days: 0b001_1111 }, TaskAction::Script { source: "x := 1;".into() });
    let id = ui.app().tasks.book.add(task, now);
    ui.key_with(Modifiers::COMMAND, Key::T);
    ui.app().toggle_pane(Pane::Windows);
    ui.app().toggle_pane(Pane::Tasks);
    ui.run();
    assert!(ui.has_label("Nightly"));
    assert_all_named(&ui, "window list and task window");
    let ctx = ui.harness.ctx.clone();
    ui.app().task_action(Action::TaskEdit(id), &ctx);
    ui.run();
    assert_all_named(&ui, "task dialog");
    ui.key(Key::Escape);
    assert!(!ui.app().task_dialog_open());
}

#[test]
fn file_sizes_follow_the_display_option() {
    use urftp_core::settings::SizeDisplay;
    let _ui = Ui::start(Lang::En, true);
    let mut s = urftp_core::settings::InterfaceSettings::default();
    for (mode, expected) in [(SizeDisplay::Bytes, "1,234,567 B"), (SizeDisplay::KBytes, "1,206 KB"), (SizeDisplay::Dynamic, "1.2 MB")] {
        s.size_display = mode;
        super::look::configure(&s);
        assert_eq!(super::look::size_text(1_234_567), expected);
    }
    urftp_i18n::set_language(Lang::Fr);
    s.size_display = SizeDisplay::Bytes;
    super::look::configure(&s);
    assert_eq!(super::look::size_text(1_234_567), "1\u{202F}234\u{202F}567 o");
    s.size_display = SizeDisplay::Dynamic;
    super::look::configure(&s);
}

#[test]
fn hidden_local_files_can_be_left_out() {
    let mut ui = Ui::start(Lang::En, true);
    let dir = scratch().join("hidden-lab");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("shown.txt"), b"a").unwrap();
    std::fs::write(dir.join("secret.txt"), b"b").unwrap();
    let status = std::process::Command::new("attrib").arg("+h").arg(dir.join("secret.txt")).status().unwrap();
    assert!(status.success());
    ui.app().tabs[0].sides[0] = Side::Local(super::LocalView::new(dir.clone()));
    assert_eq!(ui.left_rows(), ["secret.txt", "shown.txt"]);
    let ctx = ui.harness.ctx.clone();
    ui.app().options.settings.interface.local_show_hidden = false;
    ui.app().apply_settings(&ctx);
    assert_eq!(ui.left_rows(), ["shown.txt"], "listed again without the hidden file");
    ui.app().options.settings.interface.local_show_hidden = true;
    ui.app().apply_settings(&ctx);
    assert_eq!(ui.left_rows().len(), 2);
}

#[test]
fn speed_limits_toggle_from_the_status_bar() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().options.settings.transfer.download_limit_kib = 100;
    ui.app().apply_limits();
    ui.run();
    assert_eq!(ui.app().download_limit.rate(), 100 * 1024);
    let name = t!("status.limit_on", what = t!("status.limit_down"));
    ui.harness.get_by_label(&name).click();
    ui.run();
    assert!(!ui.app().options.settings.transfer.download_limit_on);
    assert_eq!(ui.app().download_limit.rate(), 0, "unlimited at once, during transfers too");
    ui.harness.get_by_label(&t!("status.limit_off", what = t!("status.limit_down"))).click();
    ui.run();
    assert_eq!(ui.app().download_limit.rate(), 100 * 1024);
    // No limit set: the upload toggle cannot be used.
    let up_name = format!("{}: {}", t!("status.limit_up"), t!("status.no_limit"));
    assert!(ui.harness.get_by_label(&up_name).accesskit_node().is_disabled());
}

#[test]
fn items_the_skip_list_would_skip_are_told() {
    use urftp_core::settings::SkipRuleSetting;
    let mut ui = Ui::start(Lang::En, true);
    let mut rule = SkipRuleSetting::default();
    rule.mask.mask = "*.log".into();
    ui.app().options.settings.skiplist.rules = vec![rule];
    ui.run();
    let spoken = |ui: &Ui, name: &str| {
        ui.harness.root().children_recursive().filter_map(|n| n.accesskit_node().label()).find(|l| l.starts_with(&format!("{name},")))
    };
    let log = spoken(&ui, "gamma.log").unwrap();
    assert!(log.ends_with(&t!("files.skipped")), "{log}");
    assert!(!spoken(&ui, "alpha.txt").unwrap().contains(&t!("files.skipped")));
    ui.app().options.settings.skiplist.show_skipped = false;
    ui.run();
    assert!(!spoken(&ui, "gamma.log").unwrap().contains(&t!("files.skipped")));
}

#[test]
fn ip_address_translate_window() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().ip_tool = Some("C0A80101".into());
    ui.run();
    assert!(ui.has_label("192.168.1.1"));
    assert_all_named(&ui, "IP address translate");
    ui.app().ip_tool = Some("not an address".into());
    ui.run();
    assert!(ui.harness.query_by_label_contains(&t!("ip.invalid")).is_some());
}

#[test]
fn columns_sized_to_their_content_keep_the_panels_usable() {
    let mut ui = Ui::start(Lang::Fr, false);
    let ctx = ui.harness.ctx.clone();
    let s = &mut ui.app().options.settings.interface;
    (s.autosize_local, s.autosize_remote, s.autosize_queue) = (true, true, true);
    ui.app().apply_settings(&ctx);
    ui.run();
    assert_eq!(ui.left_rows().len(), 4);
    assert_all_named(&ui, "columns sized to their content");
    let s = &mut ui.app().options.settings.interface;
    (s.autosize_local, s.autosize_remote, s.autosize_queue) = (false, false, false);
    ui.app().apply_settings(&ctx);
}

impl Ui {
    /// Double-click at the centre of the control named `label`.
    fn double_click(&mut self, label: &str) {
        // Idle frames first: windows still settling move their content.
        self.harness.run_steps(12);
        let pos = self.harness.get_by_label(label).rect().center();
        self.double_click_at(pos);
    }

    /// Each event of the harness takes a frame of 0.25 s: the double-click delay is
    /// lengthened meanwhile, after three idle seconds (no triple click with earlier clicks).
    fn double_click_at(&mut self, pos: egui::Pos2) {
        self.harness.run_steps(12);
        self.harness.ctx.options_mut(|o| o.input_options.max_double_click_delay = 1.0);
        self.harness.hover_at(pos);
        for pressed in [true, false, true, false] {
            self.harness.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::default(),
            });
        }
        self.run();
        self.harness.ctx.options_mut(|o| o.input_options.max_double_click_delay = 0.3);
    }
}

#[test]
fn double_click_on_a_site_connects() {
    let mut ui = Ui::start(Lang::En, true);
    // A local port nobody listens on: no network access.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let site = urftp_core::site::Site { name: "Twice".into(), host: "127.0.0.1".into(), port, ..Default::default() };
    let id = ui.app().sites.book.add_site(&[], site);
    ui.app().sites.open = true;
    ui.run();
    ui.double_click("🌐 Twice");
    assert!(!ui.app().sites.open, "the site manager closes on the connection");
    let connected = ui.app().tabs.iter().flat_map(|t| t.sides.iter()).any(|s| matches!(s, Side::Remote(r) if r.site_id == Some(id)));
    assert!(connected);
}

#[test]
fn tabs_wrap_and_a_double_click_duplicates_one() {
    use urftp_core::settings::TabDoubleClick;
    let mut ui = Ui::start(Lang::En, true);
    let s = &mut ui.app().options.settings.interface;
    (s.tabs_multiline, s.tab_min_width, s.double_click_tab) = (true, 240, TabDoubleClick::Duplicate);
    for _ in 0..5 {
        ui.key_with(Modifiers::COMMAND, Key::T);
    }
    assert_eq!(ui.app().tabs.len(), 6);
    // Six tabs of 240 points do not fit in one line of the window.
    let tops: std::collections::BTreeSet<i32> =
        (1..=6).map(|n| ui.harness.get_by_label(&t!("tabs.default_title", n = n)).rect().top().round() as i32).collect();
    assert!(tops.len() > 1, "{tops:?}");
    ui.double_click(&t!("tabs.default_title", n = 1));
    assert_eq!(ui.app().tabs.len(), 7);
    assert_all_named(&ui, "tabs on several lines");
}

#[test]
fn double_click_on_a_queue_item_or_a_panel_title() {
    use urftp_core::settings::{QueueDoubleClick, SideDoubleClick};
    let mut ui = Ui::start(Lang::En, true);
    let ctx = ui.harness.ctx.clone();
    let s = &mut ui.app().options.settings.interface;
    (s.double_click_queue, s.double_click_side) = (QueueDoubleClick::Edit, SideDoubleClick::Parent);
    ui.app().apply_settings(&ctx);
    let id = ui.app().tabs[0].queue.lock().push(
        Endpoint::remote("Demo", "/pub/twice.bin"),
        Endpoint::local(r"C:\dl\twice.bin"),
        false,
        Some(5),
        None,
    );
    ui.run();
    ui.double_click("twice.bin");
    assert_eq!(ui.app().tabs[0].queue_edit.as_ref().map(|e| e.id), Some(id));
    ui.key(Key::Escape);
    let before = ui.left_path();
    ui.double_click(&format!("{}: {}{}", t!("pane.side_left"), t!("pane.local"), t!("pane.side_active")));
    assert_eq!(Some(ui.left_path().as_path()), before.parent());
    let s = &mut ui.app().options.settings.interface;
    (s.double_click_queue, s.double_click_side) = (QueueDoubleClick::Nothing, SideDoubleClick::Nothing);
    ui.app().apply_settings(&ctx);
}

#[test]
fn links_are_listed_apart() {
    let _ui = Ui::start(Lang::En, true);
    let row = |name: &str, is_dir: bool, is_link: bool| super::FileRow {
        name: name.into(),
        is_dir,
        is_link,
        size: 0,
        modified: String::new(),
        mtime: 0,
        epoch: None,
        perms: String::new(),
        owner: String::new(),
        group: String::new(),
    };
    let mut rows = vec![row("b.txt", false, false), row("link", false, true), row("dir", true, false), row("a.txt", false, false)];
    let names = |rows: &[super::FileRow]| rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>();
    super::sort_rows(&mut rows, super::Sort::default());
    assert_eq!(names(&rows), ["dir", "a.txt", "b.txt", "link"]);
    let mut s = urftp_core::settings::InterfaceSettings { links_apart: true, ..Default::default() };
    super::look::configure(&s);
    super::sort_rows(&mut rows, super::Sort::default());
    assert_eq!(names(&rows), ["dir", "link", "a.txt", "b.txt"]);
    s.dirs_first = false;
    super::look::configure(&s);
    super::sort_rows(&mut rows, super::Sort::default());
    assert_eq!(names(&rows), ["a.txt", "b.txt", "dir", "link"]);
    super::look::configure(&urftp_core::settings::InterfaceSettings::default());
}

#[test]
fn quick_connections_are_remembered_without_their_password() {
    let mut ui = Ui::start(Lang::En, true);
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let ctx = ui.harness.ctx.clone();
    let app = ui.app();
    app.options.settings.quick_history.clear();
    (app.quick.host, app.quick.port, app.quick.user, app.quick.pass) =
        ("127.0.0.1".into(), port.to_string(), "bob".into(), "secret".into());
    app.quick_connect(&ctx);
    let history = app.options.settings.quick_history.clone();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].label(), format!("bob@127.0.0.1:{port}"));
    assert!(!serde_json::to_string(&app.options.settings).unwrap().contains("secret"));
    // Chosen again: the fields are filled, the password field waits for the password.
    app.quick.host.clear();
    app.quick.pass = "other".into();
    app.use_quick(history[0].clone(), &ctx);
    assert_eq!((app.quick.host.as_str(), app.quick.user.as_str(), app.quick.pass.as_str()), ("127.0.0.1", "bob", ""));
    ui.run();
    assert_eq!(ui.focused_label().as_deref(), Some(t!("common.password").as_str()));
    // Split Favourites button: one click connects to the last site.
    let site = urftp_core::site::Site { name: "Last".into(), host: "127.0.0.1".into(), port, ..Default::default() };
    let id = ui.app().sites.book.add_site(&[], site);
    ui.app().options.settings.recent_sites = vec![id];
    ui.app().options.settings.interface.favorites_split = true;
    ui.run();
    ui.harness.get_by_label(&t!("fav.split_connect", site = "Last")).click();
    ui.run();
    let connected = ui.app().tabs.iter().flat_map(|t| t.sides.iter()).any(|s| matches!(s, Side::Remote(r) if r.site_id == Some(id)));
    assert!(connected);
    assert_all_named(&ui, "split Favourites button");
    ui.app().options.settings.interface.favorites_split = false;
}

#[test]
fn fonts_large_buttons_and_progress_colour() {
    let mut ui = Ui::start(Lang::Fr, false);
    let ctx = ui.harness.ctx.clone();
    assert!(!crate::theme::system_fonts().is_empty(), "the Windows fonts folder is read");
    let s = &mut ui.app().options.settings.interface;
    (s.font_lists, s.large_icons, s.custom_progress) = ("no-such-font.ttf".into(), true, true);
    s.progress_light = "#AA0000".into();
    ui.app().apply_settings(&ctx);
    ui.run();
    let failed = t!("look.font_failed", font = "no-such-font.ttf");
    assert!(ui.app().tabs[0].log.iter().any(|e| e.text == failed), "an unreadable font is told");
    assert_eq!(super::look::progress_fill(false), Some(egui::Color32::from_rgb(0xAA, 0, 0)));
    assert_eq!(super::look::progress_fill(true), crate::theme::parse_color("#2F6FDB"));
    assert_all_named(&ui, "large buttons");
    assert_targets(&ui, "large buttons");
    let s = &mut ui.app().options.settings.interface;
    (s.font_lists, s.large_icons, s.custom_progress) = (String::new(), false, false);
    ui.app().apply_settings(&ctx);
}

#[test]
fn checklogin_leaves_out_the_sites_that_cannot_log_in() {
    let mut ui = Ui::start(Lang::En, true);
    // A local port nobody listens on: the login fails quickly.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let site = urftp_core::site::Site { name: "Closed".into(), host: "127.0.0.1".into(), port, ..Default::default() };
    ui.app().sites.book.add_site(&[], site);
    let ctx = ui.harness.ctx.clone();
    let script = "begin RushApp.FTP.Transfer(0, '', 'C:\\up', 'a.bin', 'Closed', '/in', '', \
                  RS_UP or RS_FILE or RS_DIRSRC or RS_DIRDES or RS_CHECKLOGIN, '', '', '', '', '', 0, 0, 0, 0, 0, 0, 0); end.";
    let failed = |ui: &mut Ui| {
        let start = t!("host.login_failed", site = "Closed", error = "\u{1}");
        let start = start.split('\u{1}').next().unwrap_or_default().to_string();
        ui.app().tabs.iter().flat_map(|t| t.log.iter()).filter(|e| e.text.contains(&start)).count()
    };
    // On the interface thread: checked at once, the pair left out.
    assert!(ui.app().run_script(script, &ctx).is_err(), "no transfer left");
    assert_eq!(failed(&mut ui), 1);
    assert!(ui.app().tabs.iter().all(|t| t.queue.lock().items().is_empty()));
    // In a script thread: checked beside the interface, then the call goes on.
    ui.app().start_script(script.to_string(), super::scripting::Origin::Env, &ctx);
    wait_scripts(&mut ui);
    assert_eq!(failed(&mut ui), 2);
    assert!(ui.app().tabs.iter().all(|t| t.queue.lock().items().is_empty()));
}

#[test]
fn help_window_searches_the_documentation() {
    let mut ui = Ui::start(Lang::En, true);
    ui.app().open_help(1);
    ui.run();
    assert_all_named(&ui, "help window");
    let search = t!("help.search");
    let field = ui.harness.get_by_label(&search);
    field.focus();
    field.type_text("MODE Z");
    ui.run();
    assert!(ui.has_label("    Advanced transfer"), "the matching section is listed");
    assert!(!ui.has_label("    Shortcuts"), "the others are not");
    ui.harness.get_by_label("    Advanced transfer").click();
    ui.run();
    assert!(ui.harness.query_by_label_contains("Compress the transfers (MODE Z)").is_some(), "its table is shown");
    ui.key(Key::Escape);
    assert!(!ui.app().shortcuts_open, "Escape closes the help");
}

#[test]
fn script_console_opens_and_saves_files() {
    let mut ui = Ui::start(Lang::En, true);
    let file = scratch().join("console.pas");
    std::fs::write(&file, "begin RushApp.FTP.RAW('', 'NOOP', 0); end.").unwrap();
    let tab = &mut ui.app().tabs[0];
    tab.open_script(&file);
    assert!(tab.script.contains("NOOP") && tab.script_file.as_deref() == Some(file.as_path()));
    tab.script.push_str("\n// changed");
    let copy = scratch().join("copy.pas");
    tab.save_script(&copy);
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), tab.script);
    assert_eq!(tab.script_saved, tab.script);
    tab.open_script(&scratch().join("missing.pas"));
    assert!(tab.script_out.last().is_some_and(|(ok, _)| !ok), "an unreadable file is told");
}

impl Ui {
    /// Drags the control named `from` and drops it on the control named `to`.
    fn drag_onto(&mut self, from: &str, to: &str) {
        self.harness.run_steps(12);
        let start = self.harness.get_by_label(from).rect().center();
        let end = self.harness.get_by_label(to).rect().center();
        self.harness.hover_at(start);
        self.harness.drag_at(start);
        self.run();
        for k in 1..=4 {
            self.harness.hover_at(start + (end - start) * (k as f32 / 4.0));
            self.run();
        }
        self.harness.drop_at(end);
        self.run();
    }
}

#[test]
fn sites_and_folders_are_dragged_in_the_site_manager() {
    use urftp_core::site::{Site, SiteGroup};
    let mut ui = Ui::start(Lang::En, true);
    // An empty book: the other tests leave their sites in the shared data folder.
    ui.app().sites.book = urftp_core::site::SiteBook::default();
    let book = &mut ui.app().sites.book;
    book.root.groups.push(SiteGroup::new("Archive"));
    book.root.groups.push(SiteGroup::new("Other"));
    let anchor = book.add_site(&[0], Site { name: "Anchor".into(), host: "a.example.com".into(), ..Default::default() });
    let mover = book.add_site(&[], Site { name: "Mover".into(), host: "m.example.com".into(), ..Default::default() });
    ui.app().sites.open = true;
    ui.run();
    // A site onto a folder: at its end.
    ui.drag_onto("🌐 Mover", "📁 Archive");
    let ids = |ui: &mut Ui, g: usize| ui.app().sites.book.root.groups[g].sites.iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(ids(&mut ui, 0), [anchor, mover]);
    // A site onto another site: before it.
    ui.drag_onto("🌐 Mover", "🌐 Anchor");
    assert_eq!(ids(&mut ui, 0), [mover, anchor]);
    // A folder onto another folder: inside it.
    ui.drag_onto("📁 Archive", "📁 Other");
    let root = &ui.app().sites.book.root;
    assert_eq!((root.groups.len(), root.groups[0].groups[0].name.as_str()), (1, "Archive"));
    assert_eq!(ui.app().sites.book.group_path_of(anchor), Some(vec![0, 0]));
    assert!(ui.app().sites.dirty, "saved after the move");
    assert_all_named(&ui, "site tree after drag and drop");
}

#[test]
fn right_click_menus_select_everything() {
    let mut ui = Ui::start(Lang::En, true);
    assert!(ui.left_rows().len() > 2);
    ui.harness.run_steps(12);
    let row = ui
        .harness
        .query_all_by_label_contains("alpha.txt")
        .filter(|n| n.accesskit_node().bounding_box().is_some())
        .map(|n| n.rect())
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap()
        .center();
    ui.harness.hover_at(row);
    ui.harness.event(egui::Event::PointerButton {
        pos: row,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    ui.harness.event(egui::Event::PointerButton {
        pos: row,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    ui.run();
    ui.harness.get_by_label(&t!("ops.m_select_all")).click();
    ui.run();
    let n = ui.left_rows().len();
    assert_eq!(ui.app().tabs[0].sides[0].selected_mut().len(), n, "every file of the panel");

    let ids = fill_queue(&mut ui);
    act(&mut ui, super::Action::QueueSelectAll);
    assert_eq!(ui.app().tabs[0].queue_sel.iter().copied().collect::<Vec<_>>(), ids);
}
