//! Shortcut manager: the commands that can get other keys, their default keys, and the
//! keys chosen in the options (`Ctrl+Shift+F`, several separated by commas).

use std::collections::BTreeMap;

use eframe::egui::{Key, Modifiers};

/// A command with keys of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    NewTab,
    DuplicateTab,
    CloseTab,
    Raw,
    SwitchPanel,
    Refresh,
    Rename,
    NewFolder,
    Delete,
    Transfer,
    Parent,
    SelectAll,
    ToggleCursor,
    Filter,
    Mark,
    SelectMask,
    UnselectMask,
    CopyNames,
    View,
    Edit,
}

impl Command {
    pub(crate) const ALL: [Command; 21] = [
        Command::Help,
        Command::NewTab,
        Command::DuplicateTab,
        Command::CloseTab,
        Command::Raw,
        Command::SwitchPanel,
        Command::Refresh,
        Command::Rename,
        Command::NewFolder,
        Command::Delete,
        Command::Transfer,
        Command::Parent,
        Command::SelectAll,
        Command::ToggleCursor,
        Command::Filter,
        Command::Mark,
        Command::SelectMask,
        Command::UnselectMask,
        Command::CopyNames,
        Command::View,
        Command::Edit,
    ];

    /// Name stored in the options.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Command::Help => "help",
            Command::NewTab => "new_tab",
            Command::DuplicateTab => "duplicate_tab",
            Command::CloseTab => "close_tab",
            Command::Raw => "raw",
            Command::SwitchPanel => "switch_panel",
            Command::Refresh => "refresh",
            Command::Rename => "rename",
            Command::NewFolder => "new_folder",
            Command::Delete => "delete",
            Command::Transfer => "transfer",
            Command::Parent => "parent",
            Command::SelectAll => "select_all",
            Command::ToggleCursor => "toggle_cursor",
            Command::Filter => "filter",
            Command::Mark => "mark",
            Command::SelectMask => "select_mask",
            Command::UnselectMask => "unselect_mask",
            Command::CopyNames => "copy_names",
            Command::View => "view",
            Command::Edit => "edit",
        }
    }

    /// Text key of its name in the interface.
    pub(crate) fn label_key(self) -> String {
        format!("keys.{}", self.id())
    }

    /// Works everywhere outside dialogs (the others need the file list to have the keys).
    pub(crate) fn global(self) -> bool {
        matches!(self, Command::Help | Command::NewTab | Command::DuplicateTab | Command::CloseTab | Command::Raw)
    }

    pub(crate) fn default_keys(self) -> &'static str {
        match self {
            Command::Help => "F1",
            Command::NewTab => "Ctrl+T",
            Command::DuplicateTab => "Ctrl+Shift+T",
            Command::CloseTab => "Ctrl+W",
            Command::Raw => "Ctrl+R",
            Command::SwitchPanel => "F6, Shift+F6",
            Command::Refresh => "F5",
            Command::Rename => "F2",
            Command::NewFolder => "F7",
            Command::Delete => "Delete",
            Command::Transfer => "Space, Insert",
            Command::Parent => "Backspace",
            Command::SelectAll => "Ctrl+A",
            Command::ToggleCursor => "Ctrl+Space",
            Command::Filter => "Ctrl+F",
            Command::Mark => "Ctrl+M",
            Command::SelectMask => "Plus",
            Command::UnselectMask => "Minus",
            Command::CopyNames => "Ctrl+Shift+C",
            Command::View => "F3",
            Command::Edit => "F4",
        }
    }
}

/// A key with its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) mods: Modifiers,
    pub(crate) key: Key,
}

impl Binding {
    fn modifier_count(&self) -> u8 {
        u8::from(self.mods.command || self.mods.ctrl) + u8::from(self.mods.shift) + u8::from(self.mods.alt)
    }
}

/// `Ctrl+Shift+F`, `F5`, `Alt+Delete`... (`Maj` and `Suppr` are accepted too).
pub(crate) fn parse(text: &str) -> Option<Binding> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::COMMAND,
            "shift" | "maj" => mods |= Modifiers::SHIFT,
            "alt" => mods |= Modifiers::ALT,
            "" => key = key.or(Some(Key::Plus)),
            "del" | "suppr" => key = Some(Key::Delete),
            "ins" | "inser" => key = Some(Key::Insert),
            "espace" => key = Some(Key::Space),
            other => {
                let name = other.chars().next().map(|c| c.to_ascii_uppercase().to_string() + &other[c.len_utf8()..]).unwrap_or_default();
                key = Some(Key::from_name(&name).or_else(|| Key::from_name(&other.to_ascii_uppercase()))?);
            }
        }
    }
    key.map(|key| Binding { mods, key })
}

/// The key of a designer command: `None` when the field is empty.
pub(crate) fn parse_optional(text: &str) -> Option<Binding> {
    let t = text.trim();
    if t.is_empty() { None } else { parse(t) }
}

/// Several keys separated by commas; `None` when one of them is not understood.
pub(crate) fn parse_list(text: &str) -> Option<Vec<Binding>> {
    text.split(',').map(str::trim).filter(|t| !t.is_empty()).map(parse).collect()
}

/// `Ctrl+Shift+F`.
pub(crate) fn format(b: &Binding) -> String {
    let mut out = String::new();
    if b.mods.command || b.mods.ctrl {
        out.push_str("Ctrl+");
    }
    if b.mods.shift {
        out.push_str("Shift+");
    }
    if b.mods.alt {
        out.push_str("Alt+");
    }
    out.push_str(b.key.name());
    out
}

/// Keys of a command: the ones chosen in the options, or the default ones.
pub(crate) fn keys_of(cmd: Command, chosen: &BTreeMap<String, String>) -> Vec<Binding> {
    chosen.get(cmd.id()).and_then(|t| parse_list(t)).unwrap_or_else(|| parse_list(cmd.default_keys()).unwrap_or_default())
}

/// Every (command, key), the keys with more modifiers first: a key without modifiers
/// would also take its `Shift` variant.
pub(crate) fn table(chosen: &BTreeMap<String, String>) -> Vec<(Command, Binding)> {
    let mut v: Vec<(Command, Binding)> = Command::ALL.iter().flat_map(|&c| keys_of(c, chosen).into_iter().map(move |b| (c, b))).collect();
    v.sort_by_key(|(_, b)| std::cmp::Reverse(b.modifier_count()));
    v
}

/// Commands that share a key.
pub(crate) fn conflicts(chosen: &BTreeMap<String, String>) -> Vec<(String, Vec<Command>)> {
    let mut by_key: BTreeMap<String, Vec<Command>> = BTreeMap::new();
    for (c, b) in table(chosen) {
        let list = by_key.entry(format(&b)).or_default();
        if !list.contains(&c) {
            list.push(c);
        }
    }
    by_key.into_iter().filter(|(_, v)| v.len() > 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_read_and_written() {
        let b = parse("Ctrl+Shift+F").unwrap();
        assert_eq!((b.key, b.mods.command, b.mods.shift, b.mods.alt), (Key::F, true, true, false));
        assert_eq!(format(&b), "Ctrl+Shift+F");
        assert_eq!(parse("Maj+Suppr").map(|b| format(&b)).as_deref(), Some("Shift+Delete"));
        assert_eq!(parse("f5").map(|b| b.key), Some(Key::F5));
        assert_eq!(parse("Ctrl++").map(|b| format(&b)).as_deref(), Some("Ctrl+Plus"));
        assert_eq!(parse("Ctrl+Nothing"), None);
        assert_eq!(parse_list("Space, Insert").map(|v| v.len()), Some(2));
        assert_eq!(parse_list("Space, ???"), None);
        // An empty key field is no key (`parse` alone would read `+`).
        assert_eq!(parse_optional("  "), None);
        assert_eq!(parse_optional(" Alt+W ").map(|b| format(&b)).as_deref(), Some("Alt+W"));
    }

    #[test]
    fn every_default_key_is_valid_and_unique() {
        for c in Command::ALL {
            assert!(parse_list(c.default_keys()).is_some_and(|v| !v.is_empty()), "{c:?}");
        }
        assert!(conflicts(&BTreeMap::new()).is_empty(), "{:?}", conflicts(&BTreeMap::new()));
        let mut chosen = BTreeMap::new();
        chosen.insert("refresh".to_string(), "F2".to_string());
        assert_eq!(conflicts(&chosen), vec![("F2".to_string(), vec![Command::Refresh, Command::Rename])]);
        assert_eq!(keys_of(Command::Refresh, &chosen).len(), 1);
        chosen.insert("rename".to_string(), "garbage".to_string());
        assert_eq!(keys_of(Command::Rename, &chosen).first().map(format).as_deref(), Some("F2"), "an invalid choice keeps the default");
    }
}
