//! Every key the window answers to, said once.
//!
//! A key used to be written in up to five places: an arm of one of the
//! results page's three key controllers (or the large view's, the setup
//! page's, the folder list's, the window's F1), a line of the F1 list, the
//! menu's table (one per view), the `gio` action behind it and the mnemonic
//! test's list of labels. Nothing tied them together, so a key could do
//! something the F1 list did not say, or the reverse, and still compile.
//!
//! Now each part of the window that takes keys is a `Section`: its F1
//! heading and its `Row`s, each row a line of the F1 list (the keys as a
//! person reads them, and what they do) and the bindings it describes. A key
//! controller asks its section what a key means (`Section::find`) and does
//! it; the F1 list is the sections, read out (`show_help`). A row with no
//! bindings says what a widget does by itself (Tab, the strip's Up and Down,
//! Shift and a click). The results page's menu items are its mnemonics: Alt
//! and the underlined letter of their label, which the row spells out.
//!
//! **Adding a key**: a binding in the row that describes it, and an arm
//! where the section's commands are done. A new command is a variant of the
//! section's enum; the tests below fail until it is bound and described.

use crate::labels as l;
use crate::results::View;
use gtk::gdk::{self, Key as K};
use gtk::prelude::*;

/// Modifiers, as a chord asks for them: a set of the three a key is told
/// apart by. Caps Lock, Num Lock and the rest are never asked about.
type Mods = u8;
pub const SHIFT: Mods = 1;
pub const CTRL: Mods = 2;
pub const ALT: Mods = 4;
const ANY: Mods = SHIFT | CTRL | ALT;

fn held(m: gdk::ModifierType) -> Mods {
    let mut h = 0;
    for (mask, bit) in [(gdk::ModifierType::SHIFT_MASK, SHIFT), (gdk::ModifierType::CONTROL_MASK, CTRL), (gdk::ModifierType::ALT_MASK, ALT)] {
        if m.contains(mask) {
            h |= bit;
        }
    }
    h
}

/// Any of `keys`, held with exactly the modifiers `need` and any of
/// `ignore`. Shift is mostly ignored: it is what reaches `+` on most
/// layouts, and what the page's arrows read for themselves.
pub struct Chord {
    pub keys: &'static [K],
    need: Mods,
    ignore: Mods,
}

impl Chord {
    fn matches(&self, key: K, mods: Mods) -> bool {
        self.keys.contains(&key) && mods & !self.ignore == self.need
    }
}

const fn plain(keys: &'static [K]) -> Chord {
    Chord { keys, need: 0, ignore: SHIFT }
}

const fn ctrl(keys: &'static [K]) -> Chord {
    Chord { keys, need: CTRL, ignore: SHIFT | ALT }
}

const fn any(keys: &'static [K]) -> Chord {
    Chord { keys, need: 0, ignore: ANY }
}

pub enum Bind<C: 'static> {
    Key(Chord, C),
    /// Alt and the command's mnemonic letter (`Command::mnemonic`).
    Mnemonic(C),
}

/// A line of the F1 list and what it describes.
pub struct Row<C: 'static> {
    /// The keys as a person reads them; empty for a row of mnemonics, which
    /// spells them from the labels.
    keys: &'static str,
    what: &'static str,
    binds: &'static [Bind<C>],
}

pub struct Section<C: 'static> {
    title: &'static str,
    rows: &'static [Row<C>],
}

/// A command a section's keys stand for.
pub trait Command: Copy + PartialEq + 'static {
    /// The label whose underlined letter, with Alt, is the command.
    fn mnemonic(self) -> Option<&'static str> {
        None
    }
}

impl<C: Command> Section<C> {
    fn binds(&self) -> impl Iterator<Item = &Bind<C>> {
        self.rows.iter().flat_map(|r| r.binds.iter())
    }

    /// What `key`, held with `mods`, means here.
    pub fn find(&self, key: K, mods: gdk::ModifierType) -> Option<C> {
        let mods = held(mods);
        let letter = (mods == ALT).then(|| key.to_lower().to_unicode()).flatten();
        self.binds().find_map(|b| match b {
            Bind::Key(chord, c) => chord.matches(key, mods).then_some(*c),
            Bind::Mnemonic(c) => (letter.is_some() && c.mnemonic().map(l::letter) == letter).then_some(*c),
        })
    }

    /// The chord bound to `c`, for a key that is not the page's own (the
    /// application's accelerators).
    pub fn chord(&self, c: C) -> Option<&Chord> {
        self.binds().find_map(|b| match b {
            Bind::Key(chord, d) if *d == c => Some(chord),
            _ => None,
        })
    }

    /// The section as the F1 list says it.
    fn lines(&self) -> Vec<(String, &'static str)> {
        let spelt = |r: &Row<C>| {
            let alt = r.binds.iter().filter_map(|b| match b {
                Bind::Mnemonic(c) => c.mnemonic().map(|m| format!("Alt+{}", l::letter(m).to_ascii_uppercase())),
                Bind::Key(..) => None,
            });
            alt.collect::<Vec<_>>().join(", ")
        };
        self.rows.iter().map(|r| (if r.keys.is_empty() { spelt(r) } else { r.keys.to_string() }, r.what)).collect()
    }
}

// ------------------------------------------------------------ the window

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Window {
    Help,
    Quit,
}

impl Command for Window {}

pub static WINDOW: Section<Window> = Section {
    title: "Everywhere",
    rows: &[
        Row { keys: "Alt + underlined letter", what: "Use that button or field", binds: &[] },
        Row { keys: "Tab, Shift+Tab", what: "Move between controls", binds: &[] },
        Row { keys: "F1", what: "This list", binds: &[Bind::Key(any(&[K::F1]), Window::Help)] },
        Row {
            keys: "Ctrl+Q, Ctrl+W",
            what: "Quit",
            binds: &[Bind::Key(Chord { keys: &[K::q, K::w], need: CTRL, ignore: 0 }, Window::Quit)],
        },
    ],
};

/// `c`'s chord as the application's accelerators spell it: one each key.
/// Spelt here rather than by `gtk::accelerator_name`, which wants GTK
/// started, and the accelerators are set before it is.
pub fn accels(c: Window) -> Vec<String> {
    let Some(chord) = WINDOW.chord(c) else { return Vec::new() };
    let mods: String = [(SHIFT, "<Shift>"), (CTRL, "<Control>"), (ALT, "<Alt>")].iter().filter(|(b, _)| chord.need & b != 0).map(|(_, n)| *n).collect();
    chord.keys.iter().filter_map(|k| k.name()).map(|n| format!("{mods}{n}")).collect()
}

// ------------------------------------------------------------ the scan page

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Scan {
    Start,
    Cancel,
    /// Remove the folder selected in a folder list (that list's own key).
    Remove,
}

impl Command for Scan {}

pub static SCAN: Section<Scan> = Section {
    title: "Scan",
    rows: &[
        Row { keys: "Ctrl+Enter", what: "Start the scan", binds: &[Bind::Key(ctrl(&[K::Return, K::KP_Enter]), Scan::Start)] },
        Row { keys: "Escape", what: "Cancel the scan", binds: &[Bind::Key(any(&[K::Escape]), Scan::Cancel)] },
        Row {
            keys: "Delete",
            what: "Remove the selected folder from a list",
            binds: &[Bind::Key(any(&[K::Delete, K::KP_Delete]), Scan::Remove)],
        },
    ],
};

// ------------------------------------------------------------ the results page

/// What the results page can be told to do, by a key, its menu or a button.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    // The selected image: moved by the layout, from anywhere on the page.
    Left,
    Right,
    Up,
    Down,
    First,
    Last,
    Mark,
    Enlarge,
    Menu,
    // The menu's.
    Open,
    ShowFolder,
    MarkOthers,
    MarkFolder,
    Unmark,
    UnmarkAll,
    SwitchView,
    // The header's.
    Next,
    Previous,
    Larger,
    Smaller,
    Fit,
}

impl Page {
    /// Its `gio` action, for the menu.
    pub fn action(self) -> &'static str {
        match self {
            Page::Enlarge => "enlarge",
            Page::Open => "open",
            Page::ShowFolder => "folder",
            Page::MarkOthers => "mark-others",
            Page::MarkFolder => "mark-folder",
            Page::Unmark => "unmark-group",
            Page::UnmarkAll => "unmark-all",
            Page::SwitchView => "toggle-view",
            _ => "",
        }
    }

    /// Its menu label in `view`; `None` where the view's menu does not
    /// have it. The group's items are the folder's in the tree view, with
    /// the same letters.
    pub fn label(self, view: View) -> Option<&'static str> {
        let tree = view == View::Folders;
        Some(match self {
            Page::Enlarge => l::ENLARGE,
            Page::Open => l::OPEN,
            Page::ShowFolder => l::SHOW_FOLDER,
            Page::MarkOthers => l::MARK_OTHERS,
            Page::MarkFolder if tree => l::MARK_FOLDER,
            Page::Unmark if tree => l::UNMARK_FOLDER,
            Page::Unmark => l::UNMARK_GROUP,
            Page::UnmarkAll if tree => l::UNMARK_ALL_FOLDERS,
            Page::UnmarkAll => l::UNMARK_ALL,
            Page::SwitchView if tree => l::TO_GROUPS,
            Page::SwitchView => l::TO_TREE,
            _ => return None,
        })
    }

    /// Whether it acts on the selected image, and so waits for one.
    pub fn needs_image(self) -> bool {
        matches!(self, Page::Mark | Page::Enlarge | Page::Menu | Page::Open | Page::ShowFolder | Page::MarkOthers)
    }
}

impl Command for Page {
    fn mnemonic(self) -> Option<&'static str> {
        self.label(View::Groups).or(self.label(View::Folders))
    }
}

/// The menu, in sections; each view shows the items it has a label for.
pub const MENU: &[&[Page]] = &[
    &[Page::Enlarge, Page::Open, Page::ShowFolder],
    &[Page::MarkOthers, Page::MarkFolder, Page::Unmark, Page::UnmarkAll],
    &[Page::SwitchView],
];

pub static RESULTS: Section<Page> = Section {
    title: "Results",
    rows: &[
        Row {
            keys: "Arrow keys, Home, End",
            what: "Select an image of the group or folder, from anywhere on the page",
            binds: &[
                Bind::Key(plain(&[K::Left, K::KP_Left]), Page::Left),
                Bind::Key(plain(&[K::Right, K::KP_Right]), Page::Right),
                Bind::Key(plain(&[K::Up, K::KP_Up]), Page::Up),
                Bind::Key(plain(&[K::Down, K::KP_Down]), Page::Down),
                Bind::Key(plain(&[K::Home, K::KP_Home]), Page::First),
                Bind::Key(plain(&[K::End, K::KP_End]), Page::Last),
            ],
        },
        Row { keys: "Up, Down in the groups", what: "Change group", binds: &[] },
        Row { keys: "Enter in the groups", what: "Go to the group's images", binds: &[] },
        Row { keys: "Left, Right in the folders", what: "Close or open a folder", binds: &[] },
        Row {
            keys: "Space or Delete",
            what: "Mark or unmark the selected image for the Trash",
            binds: &[Bind::Key(plain(&[K::space, K::Delete, K::KP_Delete]), Page::Mark)],
        },
        Row { keys: "Shift+arrow key", what: "Move, giving each image passed the mark last given by hand", binds: &[] },
        Row { keys: "Shift+click", what: "Mark or unmark every image from the one last marked or unmarked to this one", binds: &[] },
        Row {
            keys: "Enter, double click",
            what: "Open the selected image large",
            binds: &[Bind::Key(plain(&[K::Return, K::KP_Enter, K::ISO_Enter]), Page::Enlarge)],
        },
        Row {
            keys: "",
            what: "View it large, open it, show it in its folder",
            binds: &[Bind::Mnemonic(Page::Enlarge), Bind::Mnemonic(Page::Open), Bind::Mnemonic(Page::ShowFolder)],
        },
        Row { keys: "", what: "Mark every other image of the group", binds: &[Bind::Mnemonic(Page::MarkOthers)] },
        Row {
            keys: "",
            what: "Unmark the group or folder, unmark everything",
            binds: &[Bind::Mnemonic(Page::Unmark), Bind::Mnemonic(Page::UnmarkAll)],
        },
        Row { keys: "", what: "In the tree view, mark every image in the folder", binds: &[Bind::Mnemonic(Page::MarkFolder)] },
        Row { keys: "", what: "Switch between the groups and the folder tree", binds: &[Bind::Mnemonic(Page::SwitchView)] },
        Row {
            keys: "Menu, Shift+F10, right click",
            what: "The same, as a menu",
            binds: &[Bind::Key(plain(&[K::Menu]), Page::Menu), Bind::Key(Chord { keys: &[K::F10], need: SHIFT, ignore: 0 }, Page::Menu)],
        },
        Row { keys: "Ctrl+Page Down", what: "Next group or folder", binds: &[Bind::Key(ctrl(&[K::Page_Down, K::KP_Page_Down]), Page::Next)] },
        Row { keys: "Ctrl+Page Up", what: "Previous group or folder", binds: &[Bind::Key(ctrl(&[K::Page_Up, K::KP_Page_Up]), Page::Previous)] },
        Row {
            keys: "Ctrl+Plus, Ctrl+Minus",
            what: "Larger or smaller images",
            // Plus is Shift+= on most layouts, so = is plus too.
            binds: &[
                Bind::Key(ctrl(&[K::plus, K::equal, K::KP_Add]), Page::Larger),
                Bind::Key(ctrl(&[K::minus, K::underscore, K::KP_Subtract]), Page::Smaller),
            ],
        },
        Row { keys: "Ctrl+0", what: "Images as large as fit the page", binds: &[Bind::Key(ctrl(&[K::_0, K::KP_0]), Page::Fit)] },
    ],
};

// ------------------------------------------------------------ the large view

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Large {
    Previous,
    Next,
    Mark,
    Close,
}

impl Command for Large {}

pub static LARGE: Section<Large> = Section {
    title: "Large view",
    rows: &[
        Row {
            keys: "Left, Right",
            what: "Previous or next image",
            binds: &[
                Bind::Key(plain(&[K::Left, K::KP_Left]), Large::Previous),
                Bind::Key(plain(&[K::Right, K::KP_Right]), Large::Next),
            ],
        },
        Row { keys: "Space or Delete", what: "Mark or unmark it", binds: &[Bind::Key(plain(&[K::space, K::Delete, K::KP_Delete]), Large::Mark)] },
        Row { keys: "Escape", what: "Close", binds: &[Bind::Key(plain(&[K::Escape]), Large::Close)] },
    ],
};

// ------------------------------------------------------------ the F1 list

/// Every section, as the F1 list says it.
fn help() -> [(&'static str, Vec<(String, &'static str)>); 4] {
    [
        (WINDOW.title, WINDOW.lines()),
        (SCAN.title, SCAN.lines()),
        (RESULTS.title, RESULTS.lines()),
        (LARGE.title, LARGE.lines()),
    ]
}

/// Every key the window answers to. Opened by F1.
pub fn show_help(parent: &gtk::ApplicationWindow) {
    let grid = gtk::Grid::builder()
        .row_spacing(6)
        .column_spacing(24)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(20)
        .margin_end(20)
        .build();
    let mut row = 0;
    for (section, keys) in help() {
        let head = gtk::Label::builder().label(section).xalign(0.0).margin_top(if row == 0 { 0 } else { 10 }).build();
        head.add_css_class("heading");
        grid.attach(&head, 0, row, 2, 1);
        row += 1;
        for (key, what) in keys {
            let k = gtk::Label::builder().label(key).xalign(0.0).build();
            k.add_css_class("monospace");
            grid.attach(&k, 0, row, 1, 1);
            grid.attach(&gtk::Label::builder().label(what).xalign(0.0).build(), 1, row, 1, 1);
            row += 1;
        }
    }
    let win = gtk::Window::builder()
        .title("Keyboard shortcuts")
        .transient_for(parent)
        .modal(true)
        .resizable(false)
        .child(&grid)
        .build();
    crate::close_on_escape(&win);
    win.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: [Page; 21] = [
        Page::Left,
        Page::Right,
        Page::Up,
        Page::Down,
        Page::First,
        Page::Last,
        Page::Mark,
        Page::Enlarge,
        Page::Menu,
        Page::Open,
        Page::ShowFolder,
        Page::MarkOthers,
        Page::MarkFolder,
        Page::Unmark,
        Page::UnmarkAll,
        Page::SwitchView,
        Page::Next,
        Page::Previous,
        Page::Larger,
        Page::Smaller,
        Page::Fit,
    ];

    /// For every key a section binds and every way of holding the
    /// modifiers, the bindings that answer.
    fn answers<C: Command>(s: &Section<C>) -> Vec<(K, Mods, usize)> {
        let keys: Vec<K> = s.binds().filter_map(|b| if let Bind::Key(c, _) = b { Some(c.keys) } else { None }).flatten().copied().collect();
        let mut out = Vec::new();
        for &k in &keys {
            for m in 0..=ANY {
                let n = s.binds().filter(|b| matches!(b, Bind::Key(c, _) if c.matches(k, m))).count();
                out.push((k, m, n));
            }
        }
        out
    }

    fn bound<C: Command>(s: &Section<C>, c: C) -> bool {
        s.binds().any(|b| matches!(b, Bind::Key(_, d) | Bind::Mnemonic(d) if *d == c))
    }

    /// One key, held one way, means one thing on a page: the window's keys
    /// are heard on both pages, so they mean nothing else on either.
    #[test]
    fn no_key_means_two_things() {
        fn one<C: Command>(name: &str, s: &Section<C>) {
            for (k, m, n) in answers(s) {
                assert!(n <= 1, "{name}: {:?} with modifiers {m} is bound {n} times", k.name());
            }
        }
        one("window", &WINDOW);
        one("scan", &SCAN);
        one("results", &RESULTS);
        one("large view", &LARGE);
        for (k, m, _) in answers(&WINDOW).into_iter().filter(|a| a.2 > 0) {
            let mods = [(SHIFT, gdk::ModifierType::SHIFT_MASK), (CTRL, gdk::ModifierType::CONTROL_MASK), (ALT, gdk::ModifierType::ALT_MASK)]
                .iter()
                .filter(|(b, _)| m & b != 0)
                .fold(gdk::ModifierType::empty(), |a, (_, x)| a | *x);
            assert_eq!(SCAN.find(k, mods), None, "{:?} is the window's and the scan page's", k.name());
            assert_eq!(RESULTS.find(k, mods), None, "{:?} is the window's and the results page's", k.name());
        }
    }

    /// Every command of the page is reached from the keyboard, and every
    /// menu item is a mnemonic the F1 list spells.
    #[test]
    fn every_command_is_bound_and_every_mnemonic_is_the_menus() {
        for c in PAGE {
            assert!(bound(&RESULTS, c), "{c:?} has no key");
        }
        let menu: Vec<Page> = MENU.iter().flat_map(|s| s.iter().copied()).collect();
        for c in PAGE {
            let mnemonic = RESULTS.binds().any(|b| matches!(b, Bind::Mnemonic(d) if *d == c));
            assert_eq!(mnemonic, menu.contains(&c), "{c:?}: a menu item is its mnemonic, and only a menu item is one");
            assert_eq!(!c.action().is_empty(), menu.contains(&c), "{c:?}: a menu item has an action");
        }
        for c in [Window::Help, Window::Quit] {
            assert!(bound(&WINDOW, c), "{c:?}");
        }
        for c in [Scan::Start, Scan::Cancel, Scan::Remove] {
            assert!(bound(&SCAN, c), "{c:?}");
        }
        for c in [Large::Previous, Large::Next, Large::Mark, Large::Close] {
            assert!(bound(&LARGE, c), "{c:?}");
        }
    }

    /// A menu item has its letter in either view, so the F1 list's one
    /// spelling is true in both.
    #[test]
    fn a_menu_item_keeps_its_letter_in_both_views() {
        for &c in MENU.iter().flat_map(|s| s.iter()) {
            let letters: Vec<char> = [View::Groups, View::Folders].iter().filter_map(|&v| c.label(v)).map(l::letter).collect();
            assert!(!letters.is_empty(), "{c:?} is in neither view's menu");
            assert!(letters.windows(2).all(|w| w[0] == w[1]), "{c:?} changes letter with the view");
        }
    }

    #[test]
    fn a_key_finds_its_command() {
        let none = gdk::ModifierType::empty();
        let shift = gdk::ModifierType::SHIFT_MASK;
        let ctrl = gdk::ModifierType::CONTROL_MASK;
        let alt = gdk::ModifierType::ALT_MASK;
        assert_eq!(RESULTS.find(K::Left, shift), Some(Page::Left), "Shift and an arrow is the arrow's, read by the page");
        assert_eq!(RESULTS.find(K::Left, ctrl), None);
        assert_eq!(RESULTS.find(K::F10, none), None, "F10 alone is not the menu");
        assert_eq!(RESULTS.find(K::F10, shift), Some(Page::Menu));
        assert_eq!(RESULTS.find(K::equal, ctrl | shift), Some(Page::Larger), "Ctrl+Plus is Ctrl+Shift+= on most layouts");
        assert_eq!(RESULTS.find(K::v, alt), Some(Page::Enlarge));
        assert_eq!(RESULTS.find(K::V, alt), Some(Page::Enlarge), "the letter's case is the keyboard's");
        assert_eq!(RESULTS.find(K::m, alt), Some(Page::MarkFolder));
        assert_eq!(RESULTS.find(K::v, alt | shift), None);
        assert_eq!(SCAN.find(K::Escape, ctrl), Some(Scan::Cancel));
        assert_eq!(SCAN.find(K::Return, ctrl | shift), Some(Scan::Start));
        assert_eq!(SCAN.find(K::Return, none), None);
        assert_eq!(LARGE.find(K::Page_Down, none), None, "only what the F1 list says");
        assert_eq!(SCAN.find(K::BackSpace, none), None);
        assert_eq!(WINDOW.find(K::F1, ctrl), Some(Window::Help));
    }

    #[test]
    fn quit_is_the_applications_accelerators() {
        assert_eq!(accels(Window::Quit), ["<Control>q", "<Control>w"]);
    }

    /// The F1 list as it was written out by hand before it was read off the
    /// sections.
    #[test]
    fn the_f1_list_says_what_it_said() {
        let was: &[(&str, &[(&str, &str)])] = &[
            ("Everywhere", &[
                ("Alt + underlined letter", "Use that button or field"),
                ("Tab, Shift+Tab", "Move between controls"),
                ("F1", "This list"),
                ("Ctrl+Q, Ctrl+W", "Quit"),
            ]),
            ("Scan", &[
                ("Ctrl+Enter", "Start the scan"),
                ("Escape", "Cancel the scan"),
                ("Delete", "Remove the selected folder from a list"),
            ]),
            ("Results", &[
                ("Arrow keys, Home, End", "Select an image of the group or folder, from anywhere on the page"),
                ("Up, Down in the groups", "Change group"),
                ("Enter in the groups", "Go to the group's images"),
                ("Left, Right in the folders", "Close or open a folder"),
                ("Space or Delete", "Mark or unmark the selected image for the Trash"),
                ("Shift+arrow key", "Move, giving each image passed the mark last given by hand"),
                ("Shift+click", "Mark or unmark every image from the one last marked or unmarked to this one"),
                ("Enter, double click", "Open the selected image large"),
                ("Alt+V, Alt+O, Alt+F", "View it large, open it, show it in its folder"),
                ("Alt+E", "Mark every other image of the group"),
                ("Alt+U, Alt+A", "Unmark the group or folder, unmark everything"),
                ("Alt+M", "In the tree view, mark every image in the folder"),
                ("Alt+W", "Switch between the groups and the folder tree"),
                ("Menu, Shift+F10, right click", "The same, as a menu"),
                ("Ctrl+Page Down", "Next group or folder"),
                ("Ctrl+Page Up", "Previous group or folder"),
                ("Ctrl+Plus, Ctrl+Minus", "Larger or smaller images"),
                ("Ctrl+0", "Images as large as fit the page"),
            ]),
            ("Large view", &[
                ("Left, Right", "Previous or next image"),
                ("Space or Delete", "Mark or unmark it"),
                ("Escape", "Close"),
            ]),
        ];
        let now = help();
        assert_eq!(now.len(), was.len());
        for ((title, lines), (t, l)) in now.iter().zip(was) {
            assert_eq!(title, t);
            let l: Vec<(String, &str)> = l.iter().map(|(k, w)| (k.to_string(), *w)).collect();
            assert_eq!(lines, &l, "{t}");
        }
    }
}
