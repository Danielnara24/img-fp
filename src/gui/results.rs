//! The second page: the groups a scan found, for choosing what to move to the
//! Trash.
//!
//! Nothing is marked to begin with. What a group says is that every file in
//! it was matched to the file at its head (its reference image); which of them
//! to keep is left to the person looking.
//!
//! **The pictures take the page, and the words wait to be asked for.** A
//! group's images fill the space beside the strip of groups, each at its own
//! shape (`mosaic.rs`), and nothing is written on them but what a person needs
//! to see at a glance: which one is the reference, which are to be looked at, and
//! which are marked. Pointing at an image, or moving to it with the keyboard,
//! puts its details in the bar at the bottom of the page, so the picture being
//! judged is never covered.
//!
//! **The scan's suggestion is shown and offered, never applied.** The bar says
//! what img-fp suggests for the image (keep, delete, or review), and one to
//! review has a yellow corner. *Mark suggested deletions* makes the marks
//! exactly the files suggested for deletion, in every group, unmarking any
//! other, by the user's decision; the person still moves them to the Trash
//! themselves. *Suggestion rule* chooses the rule, and changes the suggestions
//! at once: the scan's own (`Mode::Content`) came with the report, and the
//! other two read only the groups.
//!
//! Two things about groups decide how marks work here:
//!
//! - **Groups overlap.** A file matched by two reference images is in both
//!   groups, so a mark is on the *file*: marked in one group, it shows as
//!   marked in every group it is in.
//! - **A group is not an all-pairs claim.** Its members matched the reference
//!   image, not necessarily each other. So when the reference image goes to
//!   the Trash, what is left of its group is still shown, and says so.
//!
//! **The tree view shows the same files by where they are**
//! (`folders.rs`): in place of the strip, the folders the scan was given and
//! every folder below them that holds a grouped image, and the page shows
//! every image under the folder selected, its subfolders' included, in path
//! order. A file is shown once there however many groups it is in, with what
//! its first group says of it. *Mark current folder* marks every image under
//! it. The top folder holds every grouped file, 27,000 on IMGS-ALL, which the
//! mosaic can show because only the images near the screen are on a tile.
//!
//! Keyboard: arrows move between images, Space or Delete marks the one under
//! the keyboard, Shift and an arrow marks the images it passes as the last
//! one marked by hand was (Shift and a click, the range from it), Enter opens it large, the Menu key (or Shift+F10, or a right
//! click) offers the rest, Ctrl+Page Down / Ctrl+Page Up change group,
//! Ctrl+Plus / Ctrl+Minus / Ctrl+0 zoom the images (`Mosaic::zoom`), and
//! every button has an Alt mnemonic.

use crate::commands::{self, Large, Page, MENU};
use crate::folders::{self, Tree};
use crate::labels as l;
use crate::last;
use crate::mosaic::Mosaic;

use crate::still::Still;
use crate::thumbs::{show_on, Key, Latest, Shown, Size, Thumbs};
use crate::App;
use img_fp::outcome::Place;
use img_fp::{Action, Evidence, Lapse, Outcome, Relation, SuggestMode};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// Long side of the large view's picture.
const LARGE: u32 = 1600;
/// Cards of the next group decoded ahead, for paging.
const AHEAD: usize = 16;
/// A group's picture in the strip.
const STRIP_W: i32 = 96;
const STRIP_H: i32 = 72;
/// Bytes of the strip's pictures kept: at 96 x 72, some six hundred groups.
const STRIP_KEEP: usize = 16 << 20;
/// The tree's width.
const TREE_W: i32 = 280;

/// Long side a group's pictures are decoded at before the page has been laid
/// out, which is the only time their tiles' sizes are not known: a group of a
/// few fills the page with each, one of dozens shares it out.
fn card_px(n: usize) -> u32 {
    match n {
        0..=4 => 1024,
        5..=12 => 720,
        13..=30 => 480,
        _ => 320,
    }
}

const CSS: &str = "
overlay.tile { border-radius: 4px; border: 3px solid transparent; background-color: alpha(@theme_fg_color, 0.07); }
overlay.tile:hover { outline: 2px solid alpha(@theme_selected_bg_color, 0.6); outline-offset: 2px; }
overlay.tile.picked { outline: 3px solid @theme_selected_bg_color; outline-offset: 2px; }
overlay.tile.marked { border-color: @error_color; }
overlay.tile box.veil { background-color: transparent; }
overlay.tile.marked box.veil { background-color: alpha(@theme_bg_color, 0.6); }
overlay.tile button.tick { min-width: 22px; min-height: 22px; padding: 0; margin: 0; border-radius: 999px; border: 2px solid white;
  background: alpha(black, 0.3); color: transparent; box-shadow: 0 1px 3px alpha(black, 0.5); opacity: 0; }
overlay.tile:hover button.tick, overlay.tile.picked button.tick, overlay.tile.marked button.tick { opacity: 1; }
overlay.tile.marked button.tick { background: @error_color; border-color: @error_color; color: white; }
overlay.tile .look { min-width: 20px; min-height: 20px; background-image: linear-gradient(to bottom left, @warning_color 50%, transparent 50%); }
overlay.tile label.pill, .strip label.badge { background-color: alpha(black, 0.68); color: white; border-radius: 999px; padding: 0 7px; font-size: smaller; font-weight: bold; }
.strip label.badge.marked, .tree label.badge.marked { background-color: @error_color; }
.tree label.badge { border-radius: 999px; padding: 0 7px; font-size: smaller; font-weight: bold; color: white; }
.tree row { padding: 2px 4px; }
.strip row { padding: 4px 8px; }
.strip row still { border-radius: 4px; }
label.suggest { border-radius: 999px; padding: 1px 9px; font-weight: bold; font-size: smaller; }
label.suggest.keep { background-color: alpha(@success_color, 0.2); color: mix(@success_color, @theme_fg_color, 0.55); }
label.suggest.delete { background-color: alpha(@error_color, 0.18); color: mix(@error_color, @theme_fg_color, 0.55); }
label.suggest.review { background-color: alpha(@warning_color, 0.25); color: mix(@warning_color, @theme_fg_color, 0.5); }
";

/// One file of one group, as the page shows it: the file, and its pair with
/// the group's reference image (`None` for the reference itself).
#[derive(Clone, Debug)]
pub struct Member {
    /// The file's number, the same in every group it is in.
    pub id: u32,
    pub path: PathBuf,
    pub dims: Option<(u32, u32)>,
    pub bytes: Option<u64>,
    pub damaged: bool,
    pub with: Option<Evidence>,
    /// Where the reference image lies in it (`outcome::Member::at`).
    pub at: Option<img_fp::outcome::Place>,
    /// What the scan suggested for it, by the content rule.
    pub suggested: Option<Action>,
}

impl Member {
    /// The file every other member of its group was matched against.
    pub fn is_representative(&self) -> bool {
        self.with.is_none()
    }
}

struct GroupState {
    files: Vec<Member>,
    /// Its reference image has been moved to the Trash.
    reference_gone: bool,
}

/// What the page knows, with every file by its number (`Member::id`) rather
/// than its path: marking all of IMGS-ALL's suggestions is 17,850 files, and
/// hashing their paths was most of what that cost once it was one pass.
#[derive(Default)]
struct State {
    /// The groups as the page shows them: less the files gone to the Trash.
    groups: Vec<GroupState>,
    /// The scan, which says what each rule suggests as files go or change
    /// (`Outcome::lapse`).
    scan: Outcome,
    marked: Vec<bool>,
    /// How many are marked, and their bytes.
    marked_n: usize,
    marked_bytes: u64,
    /// For each file, the groups it is in.
    member_of: Vec<Vec<usize>>,
    /// What each rule suggests for each file, in `MODES`' order: the scan's
    /// `actions`, asked again whenever a file goes or changes.
    by_mode: Vec<Vec<Option<Action>>>,
    /// The rule chosen: an index into `MODES`.
    mode: usize,
    /// The time, in nanoseconds since 1970, a file changed after is not the file
    /// the scan judged (`last::Notes::changed_after`): what the Trash and the
    /// suggestions check each file against.
    changed_after: Option<u64>,
    current: usize,
    problems: bool,
    analysed: usize,
    has_results: bool,
    /// The scan kept on disk for the next window, whose marks are kept with
    /// it (`last.rs`); `None` when this one could not be kept.
    kept: Option<u64>,
    /// The folders the scan was given, which the tree view starts from.
    roots: Vec<PathBuf>,
}

impl State {
    /// How many files the scan numbered.
    fn n(&self) -> usize {
        self.scan.files.len()
    }

    /// File `id`'s path.
    fn path(&self, id: usize) -> &Path {
        &self.scan.files[id].path
    }

    /// File `id`'s length, as the scan found it.
    fn bytes(&self, id: usize) -> u64 {
        self.scan.files[id].bytes.unwrap_or(0)
    }

    /// Gone to the Trash.
    fn gone(&self, id: usize) -> bool {
        self.scan.lapsed(id) == Some(Lapse::Gone)
    }

    /// Changed since the scan (`last::changed_since`): not the file it
    /// judged, so no rule suggests deleting it, nor anything resting on it.
    fn changed(&self, id: u32) -> bool {
        self.scan.lapsed(id as usize) == Some(Lapse::Changed)
    }

    /// What the chosen rule suggests for file `id`.
    fn action(&self, id: u32) -> Option<Action> {
        self.by_mode.get(self.mode).and_then(|m| m.get(id as usize).copied().flatten())
    }

    /// What the chosen rule suggests for file `id`, in a few words.
    fn said(&self, id: u32) -> &'static str {
        match self.action(id) {
            Some(Action::Review) if self.changed(id) => "review: changed since the scan",
            Some(Action::Review) if MODES[self.mode] == SuggestMode::Content => match self.unheld_of(id) {
                Some(Lapse::Gone) => "review: its kept copy is gone",
                Some(Lapse::Changed) => "review: its kept copy has changed",
                None => suggestion(Some(Action::Review)),
            },
            a => suggestion(a),
        }
    }

    /// Why the content rule no longer deletes file `id`, when the file that
    /// held it has gone (`drop_gone`) or changed since the scan
    /// (`note_changed`): a REVIEW that is not one of the scan's own.
    fn unheld_of(&self, id: u32) -> Option<Lapse> {
        self.scan.unheld(id as usize)
    }

    /// Whether file `id` is for a person to look at, as the scan said: a
    /// REVIEW, except one that is only there because it or its kept copy has
    /// gone or changed since.
    fn to_look_at(&self, id: u32) -> bool {
        self.action(id) == Some(Action::Review) && !self.changed(id) && !(MODES[self.mode] == SuggestMode::Content && self.unheld_of(id).is_some())
    }

    /// What each rule suggests, asked of the scan again.
    fn suggest(&mut self) {
        self.by_mode = MODES.iter().map(|&m| self.scan.actions(m)).collect();
    }

    /// The files, of those not already known to have, that have changed since
    /// the scan: another length, or a change time after it started
    /// (`last::changed_since`). A `stat` each, on every thread.
    fn changed_now(&self) -> Vec<usize> {
        self.scan.look(self.changed_after).1
    }

    /// Take in that `ids` have changed since the scan, as `--last` does
    /// (`Outcome::lapse`). Whether any suggestion moved.
    fn note_changed(&mut self, ids: &[usize]) -> bool {
        if !self.scan.lapse(ids.iter().copied(), Lapse::Changed) {
            return false;
        }
        self.suggest();
        true
    }

    fn is_marked(&self, id: u32) -> bool {
        self.marked.get(id as usize).copied().unwrap_or(false)
    }

    /// Mark or unmark file `id`; whether that changed anything.
    fn set_mark(&mut self, id: u32, on: bool) -> bool {
        let i = id as usize;
        if self.marked.get(i).copied().unwrap_or(on) == on {
            return false;
        }
        self.marked[i] = on;
        let b = self.bytes(i);
        if on {
            self.marked_n += 1;
            self.marked_bytes += b;
        } else {
            self.marked_n -= 1;
            self.marked_bytes -= b;
        }
        true
    }

    /// The numbers of the files marked, in order.
    fn marked_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.marked.iter().enumerate().filter(|(_, m)| **m).map(|(i, _)| i as u32)
    }

    /// The marked files the content rule deletes on the strength of a kept
    /// copy that is marked too: moved together, nothing the scan found would
    /// hold what they show. Asked whichever rule is shown, since it is the
    /// content rule that knows which file holds which.
    fn marked_with_their_copy(&self) -> usize {
        self.marked_ids().filter(|&id| self.scan.kept_copy(id as usize).is_some_and(|k| self.is_marked(k as u32))).count()
    }

    /// File `id` as its first group has it, and that group.
    fn member(&self, id: u32) -> Option<(Member, usize)> {
        let gi = *self.member_of.get(id as usize)?.first()?;
        let m = self.groups.get(gi)?.files.iter().find(|f| f.id == id)?;
        Some((m.clone(), gi))
    }
}

/// An image of the group or folder shown, whether or not it is on a tile.
struct Card {
    path: PathBuf,
    id: u32,
    /// The picture asked for while it is on a tile, and the ticket that
    /// withdraws it.
    request: Option<(Key, u64)>,
}

/// A tile of the mosaic, kept from one group to the next and told in turn
/// which image it shows (`bind_tile`): only the images on or near the screen
/// are on one, so a folder of 27,000 has the tiles a screenful needs.
#[derive(Clone)]
struct Tile {
    child: gtk::Overlay,
    still: Still,
    look: gtk::Box,
    pill: gtk::Label,
    /// The image it shows, by its place in the group or folder.
    item: Rc<Cell<Option<usize>>>,
}

/// A row of the strip. The strip is a list view, so there are only as many
/// of these as a screen shows and each is told in turn which group it is:
/// as a list box of seven hundred rows it was laid out whole on every step of
/// a scroll, some 11 ms a frame on IMGS-ALL, and held every row's picture.
struct StripRow {
    picture: Still,
    count: gtk::Label,
    marked: gtk::Label,
    overlay: gtk::Overlay,
    /// The group it shows, while it is bound to one.
    group: Cell<Option<usize>>,
    /// The marks its badge shows, so that a row whose count did not change
    /// is left alone.
    shown: Cell<Option<usize>>,
    request: RefCell<Option<(Key, u64)>>,
}

/// A row of the folder tree, told in turn which folder it is, as a row of
/// the strip is told its group.
struct TreeRow {
    expander: gtk::TreeExpander,
    name: gtk::Label,
    count: gtk::Label,
    marked: gtk::Label,
    folder: Cell<Option<usize>>,
    /// The marks its badge shows.
    shown: Cell<Option<usize>>,
}

/// Which list is beside the pictures, and so what the page shows: a group,
/// or every image under a folder. What differs between the two is asked of
/// this, or of the one `match` on it that answers each question (`show_view`,
/// `step`, `scope`, `side_list`), never of a flag in the code that uses them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum View {
    Groups,
    Folders,
}

impl View {
    fn other(self) -> View {
        match self {
            View::Groups => View::Folders,
            View::Folders => View::Groups,
        }
    }

    /// Its list's place in the stack beside the pictures.
    fn child(self) -> &'static str {
        match self {
            View::Groups => "groups",
            View::Folders => "tree",
        }
    }

    pub(crate) fn heading(self) -> &'static str {
        match self {
            View::Groups => l::GROUPS,
            View::Folders => l::FOLDERS_TREE,
        }
    }

    /// What one of its list's items is.
    fn noun(self) -> &'static str {
        match self {
            View::Groups => "group",
            View::Folders => "folder",
        }
    }
}

/// The strip of groups, the groups view's list.
///
/// Activated by Enter or a double click only, since activating a group
/// takes the keyboard to its images: on a single click, the default, the
/// strip lost the keyboard to the images on every click, and Up and Down
/// then moved through the images instead of the groups.
struct Strip {
    list: gtk::ListView,
    selection: gtk::SingleSelection,
    store: gio::ListStore,
    /// Its rows that exist, by the list item each belongs to.
    rows: RefCell<HashMap<gtk::ListItem, StripRow>>,
    /// Each group's picture (`face`).
    faces: RefCell<Vec<PathBuf>>,
    /// Set while its groups are being replaced, so that the selection moving
    /// on the way does not show each group it passes.
    quiet: Cell<bool>,
}

impl Strip {
    fn new(factory: &gtk::SignalListItemFactory) -> Strip {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::SingleSelection::builder().model(&store).autoselect(true).can_unselect(false).build();
        let list = gtk::ListView::builder().model(&selection).factory(factory).single_click_activate(false).css_classes(["strip"]).build();
        Strip { list, selection, store, rows: RefCell::new(HashMap::new()), faces: RefCell::new(Vec::new()), quiet: Cell::new(false) }
    }

    /// The groups, by their pictures. Every row is told its group again,
    /// since the groups are new.
    fn fill(&self, faces: Vec<PathBuf>) {
        let items: Vec<glib::BoxedAnyObject> = (0..faces.len()).map(glib::BoxedAnyObject::new).collect();
        *self.faces.borrow_mut() = faces;
        self.quiet.set(true);
        self.store.splice(0, self.store.n_items(), &items);
        self.quiet.set(false);
    }

    /// Group `i` selected, without showing it.
    fn select(&self, i: usize) {
        self.quiet.set(true);
        self.selection.set_selected(i as u32);
        self.quiet.set(false);
    }

    fn scroll_to(&self, i: usize) {
        let _ = self.list.activate_action("list.scroll-to-item", Some(&(i as u32).to_variant()));
    }

    /// The group `by` from the one selected, selected and so shown; whether
    /// there was one.
    fn step(&self, by: i32) -> bool {
        let (cur, n) = (self.selection.selected(), self.store.n_items() as i64);
        if n == 0 || cur == gtk::INVALID_LIST_POSITION {
            return false;
        }
        let next = (cur as i64 + by as i64).clamp(0, n - 1) as usize;
        self.selection.set_selected(next as u32);
        self.scroll_to(next);
        true
    }
}

/// The folder tree, the folders view's list.
struct Folders {
    tree: Rc<RefCell<Tree>>,
    store: gio::ListStore,
    model: gtk::TreeListModel,
    sel: gtk::SingleSelection,
    list: gtk::ListView,
    rows: RefCell<HashMap<gtk::ListItem, TreeRow>>,
    /// The folder shown.
    current: Cell<Option<usize>>,
    /// Set while the tree is being replaced or opened.
    quiet: Cell<bool>,
}

impl Folders {
    fn new(factory: &gtk::SignalListItemFactory) -> Folders {
        let tree = Rc::new(RefCell::new(Tree { folders: Vec::new(), top: Vec::new() }));
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let model = gtk::TreeListModel::new(store.clone(), false, false, {
            let tree = tree.clone();
            move |o| {
                let i = *o.downcast_ref::<glib::BoxedAnyObject>()?.borrow::<usize>();
                let t = tree.borrow();
                let children = &t.folders.get(i)?.children;
                if children.is_empty() {
                    return None;
                }
                let store = gio::ListStore::new::<glib::BoxedAnyObject>();
                let items: Vec<glib::BoxedAnyObject> = children.iter().map(|&c| glib::BoxedAnyObject::new(c)).collect();
                store.splice(0, 0, &items);
                Some(store.upcast())
            }
        });
        let sel = gtk::SingleSelection::builder().model(&model).autoselect(false).can_unselect(false).build();
        let list = gtk::ListView::builder().model(&sel).factory(factory).single_click_activate(false).css_classes(["tree"]).build();
        Folders { tree, store, model, sel, list, rows: RefCell::new(HashMap::new()), current: Cell::new(None), quiet: Cell::new(false) }
    }

    /// `tree` in place of the one shown, its top folders open. The folder
    /// shown stays shown if it still holds an image.
    fn fill(&self, tree: Tree) {
        let before = self.current.get().and_then(|f| self.tree.borrow().folders.get(f).map(|f| f.path.clone()));
        let after = before.and_then(|p| tree.find(&p));
        let top: Vec<glib::BoxedAnyObject> = tree.top.iter().map(|&t| glib::BoxedAnyObject::new(t)).collect();
        *self.tree.borrow_mut() = tree;
        self.current.set(after);
        self.quiet.set(true);
        self.store.splice(0, self.store.n_items(), &top);
        for i in (0..top.len()).rev() {
            if let Some(r) = self.model.child_row(i as u32) {
                r.set_expanded(true);
            }
        }
        self.quiet.set(false);
    }

    /// The folder at row `pos`.
    fn at(&self, pos: u32) -> Option<usize> {
        let row = self.model.row(pos)?;
        let o = row.item().and_downcast::<glib::BoxedAnyObject>()?;
        Some(*o.borrow::<usize>())
    }

    /// The row that shows folder `f`, if the tree is open to it.
    fn row_of(&self, f: usize) -> Option<u32> {
        (0..self.model.n_items()).find(|&p| self.at(p) == Some(f))
    }

    fn scroll_to(&self, pos: u32) {
        let _ = self.list.activate_action("list.scroll-to-item", Some(&pos.to_variant()));
    }

    /// The tree opened to folder `f`, and `f` selected, without showing it.
    fn open_to(&self, f: usize) {
        let chain = self.tree.borrow().ancestors(f);
        self.quiet.set(true);
        for a in chain {
            if let Some(r) = self.row_of(a).and_then(|p| self.model.row(p)) {
                r.set_expanded(true);
            }
        }
        let pos = self.row_of(f);
        if let Some(p) = pos {
            self.sel.set_selected(p);
        }
        self.quiet.set(false);
        if let Some(p) = pos {
            self.scroll_to(p);
        }
    }

    /// Open the selected folder, or close it; closing a closed one goes to
    /// the folder above.
    fn expand_selected(&self, open: bool) {
        let Some(row) = self.model.row(self.sel.selected()) else { return };
        if open {
            row.set_expanded(true);
        } else if row.is_expanded() {
            row.set_expanded(false);
        } else if let Some(parent) = row.parent() {
            let p = (0..self.model.n_items()).find(|&p| self.model.row(p).as_ref() == Some(&parent));
            if let Some(p) = p {
                self.sel.set_selected(p);
                self.scroll_to(p);
            }
        }
    }

    /// The row `by` from the one selected, selected and so shown; whether
    /// there was one.
    fn step(&self, by: i32) -> bool {
        let (cur, n) = (self.sel.selected(), self.model.n_items() as i64);
        if n == 0 || cur == gtk::INVALID_LIST_POSITION {
            return false;
        }
        let next = (cur as i64 + by as i64).clamp(0, n - 1) as u32;
        self.sel.set_selected(next);
        self.scroll_to(next);
        true
    }
}

/// What the bottom bar says about one image.
struct Details {
    name: gtk::Label,
    dir: gtk::Label,
    facts: gtk::Label,
    facts_note: gtk::Label,
    why: gtk::Label,
    why_note: gtk::Label,
    suggestion: gtk::Label,
}

pub struct Results {
    pub root: gtk::Box,
    app: Rc<App>,
    thumbs: Rc<Thumbs>,
    /// The strip's pictures, with a decoder of their own, so that changing
    /// group neither drops them nor waits for them.
    strip_thumbs: Rc<Thumbs>,
    state: RefCell<State>,
    strip: Strip,
    folders: Folders,
    view: Cell<View>,
    left: gtk::Stack,
    left_label: gtk::Label,
    menu: gio::Menu,
    prev: gtk::Button,
    next: gtk::Button,
    mosaic: Mosaic,
    scroll: gtk::ScrolledWindow,
    /// What each card shows: the file as a group has it, and that group.
    shown: RefCell<Vec<(Member, usize)>>,
    title: gtk::Label,
    note: gtk::Label,
    status: gtk::Label,
    bar: gtk::Stack,
    details: Details,
    trash: gtk::Button,
    mark_suggested: gtk::Button,
    group_buttons: Vec<gtk::Widget>,
    /// The image menu, for the Menu key and a right click.
    context: gtk::PopoverMenu,
    cards: RefCell<Vec<Card>>,
    tiles: RefCell<Vec<Tile>>,
    /// The selected image, which the menu acts on: the last one clicked or
    /// reached with the arrow keys. Kept apart from focus, which a click on a
    /// button takes.
    selected: Cell<Option<usize>>,
    /// The image last marked or unmarked by hand, and which of the two:
    /// where a Shift+click's range starts, and what Shift and an arrow give
    /// the images they pass. Forgotten when the images shown change.
    anchor: Cell<Option<(usize, bool)>>,
    /// The image under the pointer.
    hovered: Cell<Option<usize>>,
    /// Whether the bar shows the image under the pointer rather than the
    /// selected one: whichever of the two moved last.
    follow_pointer: Cell<bool>,
    /// The menu's actions that need a selected image.
    image_actions: RefCell<Vec<gio::SimpleAction>>,
    /// Set while files are being moved to the Trash. Marking a card during
    /// that used to turn Trash back on, and a second trash then ran beside
    /// the first; New scan could replace the results the first was about to
    /// edit.
    trashing: Cell<bool>,
    /// A save of the marks is waiting to be written.
    marks_pending: Cell<bool>,
    settings_button: gtk::Button,
    to_settings: RefCell<Option<Box<dyn Fn()>>>,
}

impl Results {
    pub fn new(app: Rc<App>) -> Rc<Results> {
        let css = gtk::CssProvider::new();
        css.load_from_data(CSS);
        // At the user's own priority, added after theirs so it wins: a user
        // stylesheet that themes every widget would otherwise hide which
        // images are marked.
        gtk::style_context_add_provider_for_display(&WidgetExt::display(&app.window), &css, gtk::STYLE_PROVIDER_PRIORITY_USER);

        // ---- the strip of groups
        let factory = gtk::SignalListItemFactory::new();
        let strip = Strip::new(&factory);
        let list_scroll = gtk::ScrolledWindow::builder()
            .child(&strip.list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            // As wide as a row of the strip, and not as wide as its pictures
            // ask: a picture's natural width is the size it was decoded at.
            .min_content_width(STRIP_W + 16)
            // And while it is empty, which `min_content_width` does not
            // cover: the page waiting for the last scan was narrower here,
            // and moved when the groups came.
            .width_request(STRIP_W + 16)
            .propagate_natural_width(false)
            .hexpand(false)
            .build();
        // ---- the folder tree, in the strip's place in the tree view
        let tree_factory = gtk::SignalListItemFactory::new();
        let folders = Folders::new(&tree_factory);
        let tree_scroll = gtk::ScrolledWindow::builder()
            .child(&folders.list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .min_content_width(TREE_W)
            .width_request(TREE_W)
            .propagate_natural_width(false)
            .hexpand(false)
            .build();

        let left_label = gtk::Label::with_mnemonic(View::Groups.heading());
        left_label.set_mnemonic_widget(Some(&strip.list));
        left_label.set_xalign(0.0);
        left_label.set_margin_start(8);
        left_label.add_css_class("heading");
        let left_stack = gtk::Stack::builder().hhomogeneous(false).vexpand(true).build();
        left_stack.add_named(&list_scroll, Some(View::Groups.child()));
        left_stack.add_named(&tree_scroll, Some(View::Folders.child()));
        left_stack.set_visible_child_name(View::Groups.child());
        let left = gtk::Box::new(gtk::Orientation::Vertical, 6);
        left.set_margin_top(10);
        left.append(&left_label);
        left.append(&left_stack);

        // ---- the header: which group, and what applies to every group
        let prev = gtk::Button::from_icon_name("go-previous-symbolic");
        prev.set_tooltip_text(Some("Previous group (Ctrl+Page Up)"));
        let next = gtk::Button::from_icon_name("go-next-symbolic");
        next.set_tooltip_text(Some("Next group (Ctrl+Page Down)"));
        let title = gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).css_classes(["heading"]).build();
        let rule = gtk::DropDown::from_strings(RULES);
        rule.set_tooltip_text(Some(
            "Keep all content: delete only an image that a kept image shows all of, at about the same detail.\n\
             By correlation: keep each group's reference image, delete the images that agree with it closely, keep the rest, and leave weak matches to review.\n\
             Reference images only: keep each group's reference image and delete everything else.\n\
             The last two can suggest deleting an image that shows something the reference image does not, such as a collage or the uncropped photo.",
        ));
        let rule_label = gtk::Label::with_mnemonic(l::SUGGEST_RULE);
        rule_label.add_css_class("dim-label");
        rule_label.set_mnemonic_widget(Some(&rule));
        let mark_suggested = gtk::Button::with_mnemonic(l::MARK_SUGGESTED);
        mark_suggested.set_tooltip_text(Some(
            "Mark exactly the images the rule suggests deleting, in every group. Every other mark is removed, including ones made by hand.",
        ));
        mark_suggested.set_sensitive(false);

        // What applies to one image or one group: in a menu, so that the
        // page carries no row of buttons above the pictures.
        // Each item is also Alt and its letter from anywhere on the page,
        // which the menu shows beside it.
        let menu = gio::Menu::new();
        fill_menu(&menu, View::Groups);
        let more = gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&menu).tooltip_text("The selected image, the group or folder, and the view").build();

        // The pictures' size, beside what it applies to: the page below.
        let smaller = gtk::Button::from_icon_name("zoom-out-symbolic");
        smaller.set_tooltip_text(Some("Smaller images (Ctrl+Minus; Ctrl+0 fits the group to the page)"));
        let larger = gtk::Button::from_icon_name("zoom-in-symbolic");
        larger.set_tooltip_text(Some("Larger images (Ctrl+Plus; Ctrl+0 fits the group to the page)"));
        for b in [&smaller, &larger] {
            b.set_sensitive(false);
        }
        let zoom = gtk::Box::builder().css_classes(["linked"]).margin_end(8).build();
        zoom.append(&smaller);
        zoom.append(&larger);

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.append(&prev);
        head.append(&next);
        head.append(&title);
        title.set_hexpand(true);
        title.set_margin_start(4);
        head.append(&zoom);
        head.append(&rule_label);
        head.append(&rule);
        head.append(&mark_suggested);
        head.append(&more);
        let note = gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["dim-label"]).visible(false).build();

        // ---- the pictures
        let mosaic = Mosaic::new();
        let scroll = gtk::ScrolledWindow::builder().child(&mosaic).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).hexpand(true).build();
        let context = gtk::PopoverMenu::from_model(Some(&menu));
        context.set_has_arrow(false);
        context.set_halign(gtk::Align::Start);
        mosaic.add_popover(&context);
        // A group is fitted into the height the page shows. Told after the
        // allocation that changed it, not during it.
        scroll.vadjustment().connect_page_size_notify({
            let mosaic = mosaic.downgrade();
            move |a| {
                let (mosaic, h) = (mosaic.clone(), a.page_size());
                glib::idle_add_local_once(move || {
                    if let Some(m) = mosaic.upgrade() {
                        m.set_viewport(h);
                    }
                });
            }
        });

        let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
        right.set_margin_end(12);
        right.set_margin_top(10);
        right.set_margin_bottom(8);
        right.append(&head);
        right.append(&note);
        right.append(&scroll);

        // A box, not a pane: the strip keeps its width and only the pictures
        // grow with the window.
        left.set_hexpand(false);
        right.set_hexpand(true);
        let panes = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(12).vexpand(true).build();
        panes.append(&left);
        panes.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        panes.append(&right);

        // ---- the bottom bar: the image under the pointer, or the marks
        let status = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::End).build();
        let hint = gtk::Label::builder()
            .label("Point at an image, or use the arrow keys, for its details · F1 keyboard shortcuts")
            .css_classes(["dim-label", "caption"])
            .build();
        let summary = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        summary.append(&status);
        summary.append(&hint);

        let column = |a: &gtk::Label, b: &gtk::Label| {
            let c = gtk::Box::new(gtk::Orientation::Vertical, 2);
            c.set_valign(gtk::Align::Center);
            c.append(a);
            c.append(b);
            c
        };
        let label = |classes: &[&str], ellipsize: gtk::pango::EllipsizeMode| {
            gtk::Label::builder().xalign(0.0).ellipsize(ellipsize).css_classes(classes.iter().map(|c| c.to_string()).collect::<Vec<_>>()).build()
        };
        use gtk::pango::EllipsizeMode as E;
        let details = Details {
            name: label(&["heading"], E::Middle),
            dir: label(&["dim-label", "caption"], E::Start),
            facts: label(&[], E::End),
            facts_note: label(&["dim-label", "caption"], E::End),
            why: label(&[], E::End),
            why_note: label(&["dim-label", "caption"], E::Middle),
            suggestion: gtk::Label::builder().css_classes(["suggest"]).halign(gtk::Align::Start).valign(gtk::Align::Center).build(),
        };
        // The columns are fixed shares of the bar, whatever they say, so
        // that pointing from image to image never moves the text: a grid of
        // equal columns ignores what its labels would like, and each column
        // spans a set number of them (name 3, size 2, evidence 3).
        let detail_grid = gtk::Grid::builder().column_homogeneous(true).column_spacing(14).hexpand(true).build();
        let cell = |content: &gtk::Box, separated: bool| {
            let c = gtk::Box::new(gtk::Orientation::Horizontal, 14);
            if separated {
                c.append(&gtk::Separator::new(gtk::Orientation::Vertical));
            }
            content.set_hexpand(true);
            c.append(content);
            c
        };
        detail_grid.attach(&cell(&column(&details.name, &details.dir), false), 0, 0, 3, 1);
        detail_grid.attach(&cell(&column(&details.facts, &details.facts_note), true), 3, 0, 2, 1);
        detail_grid.attach(&cell(&column(&details.why, &details.why_note), true), 5, 0, 3, 1);
        // The pill's place is as wide as its longest wording, held by an
        // invisible copy of it, so that "keep" and "delete · marked" start
        // in the same place and its absence leaves the place empty.
        let sizer = gtk::Label::builder().label("suggested: delete · marked").css_classes(["suggest"]).opacity(0.0).build();
        let pill = gtk::Stack::builder().hhomogeneous(true).valign(gtk::Align::Center).build();
        pill.add_child(&sizer);
        pill.add_child(&details.suggestion);
        pill.set_visible_child(&details.suggestion);
        let detail = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        detail.append(&detail_grid);
        detail.append(&pill);
        let bar = gtk::Stack::builder().hexpand(true).build();
        bar.add_named(&summary, Some("summary"));
        bar.add_named(&detail, Some("details"));
        bar.set_visible_child_name("summary");

        let log = gtk::Button::with_mnemonic(l::RESULTS_LOG);
        let to_settings = gtk::Button::with_mnemonic(l::TO_SETTINGS);
        let trash = gtk::Button::with_mnemonic(l::TRASH);
        trash.add_css_class("destructive-action");
        trash.set_sensitive(false);
        let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bottom.set_margin_start(12);
        bottom.set_margin_end(12);
        bottom.set_margin_top(8);
        bottom.set_margin_bottom(10);
        bottom.set_size_request(-1, 44);
        bottom.append(&bar);
        for b in [&log, &to_settings, &trash] {
            b.set_valign(gtk::Align::Center);
            bottom.append(b);
        }

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&panes);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&bottom);

        let group_buttons: Vec<gtk::Widget> = vec![prev.clone().upcast(), next.clone().upcast(), more.clone().upcast()];
        let me = Rc::new(Results {
            root,
            app: app.clone(),
            thumbs: Thumbs::new(),
            strip_thumbs: Thumbs::with_workers(1, STRIP_KEEP),
            state: RefCell::new(State::default()),
            strip,
            folders,
            view: Cell::new(View::Groups),
            left: left_stack,
            left_label,
            menu,
            prev: prev.clone(),
            next: next.clone(),
            mosaic,
            scroll: scroll.clone(),
            shown: RefCell::new(Vec::new()),
            title,
            note,
            status,
            bar,
            details,
            trash,
            mark_suggested: mark_suggested.clone(),
            group_buttons,
            context,
            cards: RefCell::new(Vec::new()),
            tiles: RefCell::new(Vec::new()),
            selected: Cell::new(None),
            anchor: Cell::new(None),
            hovered: Cell::new(None),
            follow_pointer: Cell::new(false),
            image_actions: RefCell::new(Vec::new()),
            trashing: Cell::new(false),
            marks_pending: Cell::new(false),
            settings_button: to_settings.clone(),
            to_settings: RefCell::new(None),
        });

        let weak = Rc::downgrade(&me);
        let with = move |f: fn(&Rc<Results>)| {
            let weak = weak.clone();
            move |_: &gtk::Button| {
                if let Some(me) = weak.upgrade() {
                    f(&me);
                }
            }
        };
        prev.connect_clicked(with(|me| me.step(-1)));
        smaller.connect_clicked(with(|me| me.zoom(Some(false))));
        larger.connect_clicked(with(|me| me.zoom(Some(true))));
        me.mosaic.connect_least_row({
            let weak = Rc::downgrade(&me);
            move || weak.upgrade().map_or(1.0, |me| me.least_row())
        });
        me.mosaic.connect_zoomable(move |can_in, can_out| {
            larger.set_sensitive(can_in);
            smaller.set_sensitive(can_out);
        });
        next.connect_clicked(with(|me| me.step(1)));
        me.trash.connect_clicked(with(|me| me.confirm_trash()));
        mark_suggested.connect_clicked(with(|me| me.mark_suggested()));

        // The menu's items, as actions its model names.
        let actions = gio::SimpleActionGroup::new();
        for &act in MENU.iter().flat_map(|s| s.iter()) {
            let a = gio::SimpleAction::new(act.action(), None);
            let weak = Rc::downgrade(&me);
            a.connect_activate(move |_, _| {
                if let Some(me) = weak.upgrade() {
                    me.run(act, false);
                }
            });
            actions.add_action(&a);
            if act.needs_image() {
                me.image_actions.borrow_mut().push(a);
            }
        }
        me.root.insert_action_group("results", Some(&actions));
        me.select(None);
        // Back to the image the menu was about, so the keyboard carries on
        // where it was.
        me.context.connect_closed({
            let weak = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = weak.upgrade()
                    && let Some(i) = me.selected.get()
                {
                    glib::idle_add_local_once(move || me.move_to(i as i64));
                }
            }
        });

        rule.connect_selected_notify({
            let weak = Rc::downgrade(&me);
            move |d| {
                if let Some(me) = weak.upgrade() {
                    me.set_mode((d.selected() as usize).min(MODES.len() - 1));
                }
            }
        });
        to_settings.connect_clicked(with(|me| {
            if let Some(f) = me.to_settings.borrow().as_ref() {
                f();
            }
        }));
        log.connect_clicked({
            let app = app.clone();
            move |_| app.show_log()
        });

        factory.connect_setup({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                let picture = Still::new(STRIP_W, STRIP_H);
                picture.set_overflow(gtk::Overflow::Hidden);
                let count = gtk::Label::builder().css_classes(["badge"]).halign(gtk::Align::End).valign(gtk::Align::End).margin_end(4).margin_bottom(4).build();
                let marked = gtk::Label::builder().css_classes(["badge", "marked"]).halign(gtk::Align::Start).valign(gtk::Align::End).margin_start(4).margin_bottom(4).visible(false).build();
                let overlay = gtk::Overlay::builder().child(&picture).build();
                overlay.add_overlay(&count);
                overlay.add_overlay(&marked);
                item.set_child(Some(&overlay));
                let row = StripRow { picture, count, marked, overlay, group: Cell::new(None), shown: Cell::new(None), request: RefCell::new(None) };
                me.strip.rows.borrow_mut().insert(item.clone(), row);
            }
        });
        factory.connect_bind({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                me.bind_row(item);
            }
        });
        factory.connect_unbind({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                me.unbind_row(item);
            }
        });
        factory.connect_teardown({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                me.strip.rows.borrow_mut().remove(item);
            }
        });
        me.strip.selection.connect_selected_notify({
            let weak = Rc::downgrade(&me);
            move |sel| {
                let Some(me) = weak.upgrade() else { return };
                if me.strip.quiet.get() || sel.selected() == gtk::INVALID_LIST_POSITION {
                    return;
                }
                me.show_group(sel.selected() as usize);
            }
        });
        tree_factory.connect_setup({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                let name = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::Middle).build();
                let marked = gtk::Label::builder().css_classes(["badge", "marked"]).valign(gtk::Align::Center).visible(false).build();
                let count = gtk::Label::builder().css_classes(["dim-label", "caption"]).valign(gtk::Align::Center).build();
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                b.append(&name);
                b.append(&marked);
                b.append(&count);
                let expander = gtk::TreeExpander::builder().child(&b).build();
                item.set_child(Some(&expander));
                let row = TreeRow { expander, name, count, marked, folder: Cell::new(None), shown: Cell::new(None) };
                me.folders.rows.borrow_mut().insert(item.clone(), row);
            }
        });
        tree_factory.connect_bind({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                me.bind_tree_row(item);
            }
        });
        tree_factory.connect_unbind({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                if let Some(r) = me.folders.rows.borrow().get(item) {
                    r.folder.set(None);
                    r.expander.set_list_row(None);
                }
            }
        });
        tree_factory.connect_teardown({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                me.folders.rows.borrow_mut().remove(item);
            }
        });
        me.folders.sel.connect_selected_notify({
            let weak = Rc::downgrade(&me);
            move |sel| {
                let Some(me) = weak.upgrade() else { return };
                if me.folders.quiet.get() || me.view.get() != View::Folders {
                    return;
                }
                if let Some(f) = me.folders.at(sel.selected()) {
                    me.show_folder(f);
                }
            }
        });
        // Enter on a folder takes the keyboard to its images.
        me.folders.list.connect_activate({
            let weak = Rc::downgrade(&me);
            move |_, _| {
                if let Some(me) = weak.upgrade() {
                    me.focus();
                }
            }
        });
        // Enter on a group takes the keyboard to its images.
        me.strip.list.connect_activate({
            let weak = Rc::downgrade(&me);
            move |_, _| {
                if let Some(me) = weak.upgrade() {
                    me.focus();
                }
            }
        });
        // The tiles are a pool: each is told which image it now shows.
        me.mosaic.connect_bind(
            {
                let weak = Rc::downgrade(&me);
                move |slot, item| {
                    if let Some(me) = weak.upgrade() {
                        me.bind_tile(slot, item);
                    }
                }
            },
            {
                let weak = Rc::downgrade(&me);
                move || {
                    if let Some(me) = weak.upgrade() {
                        let t = me.new_tile();
                        me.mosaic.append(&t.child);
                        me.tiles.borrow_mut().push(t);
                    }
                }
            },
        );
        me.scroll.vadjustment().connect_value_changed({
            let mosaic = me.mosaic.downgrade();
            move |a| {
                if let Some(m) = mosaic.upgrade() {
                    m.set_offset(a.value());
                }
            }
        });
        // A tile given another size is given a picture of that size.
        me.mosaic.connect_resized({
            let weak = Rc::downgrade(&me);
            move || {
                if let Some(me) = weak.upgrade() {
                    me.tiles_resized();
                }
            }
        });
        // The page's keys are the window's, taken before the widget with the
        // keyboard sees them: attached to the page, they were never seen at
        // all while nothing on it had the keyboard, since GTK then hands a key
        // to the window alone. Only while the page is showing; which the
        // page takes from the widget with the keyboard is `key`'s.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let weak = Rc::downgrade(&me);
            move |_, key, _, mods| {
                let Some(me) = weak.upgrade().filter(|me| me.showing()) else { return glib::Propagation::Proceed };
                match commands::RESULTS.find(key, mods) {
                    Some(act) if me.key(act, key, mods) => glib::Propagation::Stop,
                    _ => glib::Propagation::Proceed,
                }
            }
        });
        me.app.window.add_controller(keys);
        // The bar follows the keyboard out of the images as well as in.
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave({
            let weak = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = weak.upgrade() {
                    me.refresh_bar();
                }
            }
        });
        me.mosaic.add_controller(focus);
        me
    }

    /// `key`, which means `act` (`commands::RESULTS`), done if the page
    /// takes it; whether it did.
    ///
    /// The image keys work from anywhere on the page: the arrows move the
    /// selection by the layout, left and right through the group's order,
    /// up and down to the nearest image of the row above or below. What they
    /// leave alone: Up, Down, Home, End, Space and Enter in the strip, which
    /// change and open groups there, Left and Right in the tree, which close
    /// and open its folders, and Space and Enter on a button or the rule
    /// picker, which press it. A menu item's letter is the view's menu's
    /// alone, and taken whether or not it can do anything now, as its item
    /// would be.
    fn key(self: &Rc<Self>, act: Page, key: gdk::Key, mods: gdk::ModifierType) -> bool {
        if mods.contains(gdk::ModifierType::CONTROL_MASK) {
            return self.run(act, false);
        }
        if mods.contains(gdk::ModifierType::ALT_MASK) {
            if act.label(self.view.get()).is_none() {
                return false;
            }
            if !act.needs_image() || self.selected.get().is_some() {
                self.run(act, false);
            }
            return true;
        }
        let sideways = matches!(act, Page::Left | Page::Right);
        let tree = self.view.get() == View::Folders;
        if self.cards.borrow().is_empty() && !(tree && sideways) {
            return false;
        }
        let focus = gtk::prelude::GtkWindowExt::focus(&self.app.window);
        let on_tile = focus.as_ref().is_some_and(|f| self.tiles.borrow().iter().any(|t| t.child.upcast_ref::<gtk::Widget>() == f));
        let side = self.side_list();
        let in_list = focus.as_ref().is_some_and(|f| f.is_ancestor(&side) || f == side.upcast_ref::<gtk::Widget>());
        if in_list && tree && sideways {
            self.folders.expand_selected(act == Page::Right);
            return true;
        }
        let presses = matches!(key, gdk::Key::space | gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter);
        let theirs = match act {
            Page::Up | Page::Down | Page::First | Page::Last => in_list,
            _ => presses && focus.is_some() && !on_tile,
        };
        !theirs && self.run(act, mods.contains(gdk::ModifierType::SHIFT_MASK))
    }

    /// Do `act`, from a key, the menu or a button; whether there was
    /// anything to do it to. With `shift`, an arrow gives every image it
    /// passes the mark the anchor was given (`mark_along`).
    fn run(self: &Rc<Self>, act: Page, shift: bool) -> bool {
        let selected = self.selected.get();
        let go = |i: i64| match (shift, selected) {
            (true, Some(from)) => self.mark_along(from, i),
            _ => self.move_to(i),
        };
        match (act, selected) {
            (Page::Left, Some(i)) => go(i as i64 - 1),
            (Page::Right, Some(i)) => go(i as i64 + 1),
            (Page::Up | Page::Down, Some(i)) => {
                if let Some(j) = self.mosaic.vertical_neighbour(i, act == Page::Down) {
                    go(j as i64);
                }
            }
            // Nothing selected yet: any arrow starts at the first.
            (Page::Left | Page::Right | Page::Up | Page::Down, None) => go(0),
            (Page::First, _) => go(0),
            (Page::Last, _) => go(i64::MAX),
            (Page::Mark, Some(i)) => self.toggle(i),
            (Page::Enlarge, Some(i)) => self.preview(i),
            (Page::Menu, Some(i)) => self.popup_menu(i, None),
            (Page::Open, Some(_)) => self.launch(false),
            (Page::ShowFolder, Some(_)) => self.launch(true),
            (Page::MarkOthers, Some(_)) => self.mark_all_but_selected(),
            (Page::MarkFolder, _) => self.mark_shown(true),
            (Page::Unmark, _) => self.mark_shown(false),
            (Page::UnmarkAll, _) => self.unmark_all(),
            (Page::SwitchView, _) => self.switch_view(),
            (Page::Next, _) => self.step(1),
            (Page::Previous, _) => self.step(-1),
            (Page::Larger, _) => self.zoom(Some(true)),
            (Page::Smaller, _) => self.zoom(Some(false)),
            (Page::Fit, _) => self.zoom(None),
            _ => return false,
        }
        true
    }

    /// The pictures one step larger (`Some(true)`), smaller, or (`None`)
    /// fitted to the page again (`Mosaic::zoom`), with the selected image
    /// kept in view once the page has been laid out anew.
    fn zoom(self: &Rc<Self>, step: Option<bool>) {
        let moved = match step {
            Some(in_) => self.mosaic.zoom(in_),
            None => self.mosaic.zoom_fit(),
        };
        if !moved {
            return;
        }
        // At idle priority, which is after the frame's layout.
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(me) = weak.upgrade()
                && let Some(i) = me.selected.get()
            {
                me.scroll_into_view(i);
            }
        });
    }

    /// Whether this page is the one on screen, and so the window's keys are
    /// its own.
    fn showing(&self) -> bool {
        self.app.stack.visible_child_name().as_deref() == Some("results")
    }

    pub fn set_to_settings(&self, f: impl Fn() + 'static) {
        *self.to_settings.borrow_mut() = Some(Box::new(f));
    }

    pub fn has_results(&self) -> bool {
        self.state.borrow().has_results
    }

    /// A finished scan's groups, replacing whatever was shown.
    pub fn show(self: &Rc<Self>, prepared: Prepared, problems: bool) {
        let Prepared { groups, scan, by_mode, kept, roots, changed_after } = prepared;
        {
            let mut s = self.state.borrow_mut();
            // The rule chosen stays chosen from one scan to the next.
            let mode = s.mode;
            let n = scan.files.len();
            *s = State {
                groups,
                analysed: scan.head.files_analysed,
                scan,
                by_mode,
                marked: vec![false; n],
                problems,
                has_results: true,
                mode,
                kept,
                roots,
                changed_after,
                ..State::default()
            };
        }
        self.marks_pending.set(false);
        self.thumbs.clear_queue();
        self.strip_thumbs.clear_queue();
        // Another scan's folders: the tree starts again from the top.
        self.folders.current.set(None);
        self.rebuild(0);
        // A file written while the scan ran, after it was read, is already
        // not the file it judged.
        self.check_changed();
    }

    /// Look again for files changed since the scan, and take in any found
    /// (`State::note_changed`). Asked when the results are shown, and again
    /// before anything acts on the suggestions: a file can change while they
    /// are on screen. Whether any suggestion moved.
    fn check_changed(self: &Rc<Self>) -> bool {
        let found = self.state.borrow().changed_now();
        if !self.state.borrow_mut().note_changed(&found) {
            return false;
        }
        let (current, selected) = (self.state.borrow().current, self.selected.get());
        self.rebuild(current);
        self.select(selected);
        self.update_status();
        self.refresh_bar();
        true
    }

    /// The scan kept from last time, as it was left: its groups and marks,
    /// less the files that have gone since (to the Trash before the window
    /// closed, or any other way), whose suggestions are worked out with them
    /// first, as they were when the scan was shown.
    pub fn restore(self: &Rc<Self>, prepared: Prepared, problems: bool, marks: &[u32], gone: &HashSet<PathBuf>) {
        self.show(prepared, problems);
        self.mark_ids(marks, true);
        if !gone.is_empty() {
            self.drop_gone(gone);
        }
        self.update_status();
        self.refresh_bar();
    }

    /// An empty page while the last scan's results are read, so that the
    /// window opens where it will be rather than on the settings.
    pub fn loading(&self) {
        self.title.set_text("");
        self.note.set_text("Loading the last scan's results…");
        self.note.set_visible(true);
        self.status.set_text("");
        for b in &self.group_buttons {
            b.set_sensitive(false);
        }
        self.trash.set_sensitive(false);
        self.mark_suggested.set_sensitive(false);
    }

    /// The marks have changed: write them for the next window, once they
    /// stop changing for a moment, so that a run of clicks is one write.
    fn marks_changed(self: &Rc<Self>) {
        if self.state.borrow().kept.is_none() || self.marks_pending.replace(true) {
            return;
        }
        let me = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(400), move || {
            if let Some(me) = me.upgrade() {
                me.flush_marks();
            }
        });
    }

    /// Write the marks now, if a save is waiting: the window is closing.
    pub fn flush_marks(&self) {
        if !self.marks_pending.replace(false) {
            return;
        }
        let s = self.state.borrow();
        let Some(kept) = s.kept else { return };
        let paths = s.marked_ids().filter(|&id| !s.gone(id as usize)).map(|id| s.path(id as usize).to_path_buf()).collect();
        last::save_marks(kept, paths);
    }

    /// Keyboard to the page: the selected image, else the first, or the
    /// list when there are none.
    pub fn focus(&self) {
        if self.cards.borrow().is_empty() {
            self.side_list().grab_focus();
            return;
        }
        let i = self.selected.get().unwrap_or(0);
        match self.tile_of(i) {
            Some(t) => {
                t.child.grab_focus();
            }
            None => self.move_to(i as i64),
        }
    }

    /// The list on the left: the strip of groups, or the folder tree.
    fn side_list(&self) -> gtk::ListView {
        match self.view.get() {
            View::Groups => self.strip.list.clone(),
            View::Folders => self.folders.list.clone(),
        }
    }

    /// Both lists, from the state, with group `select` selected.
    fn rebuild(self: &Rc<Self>, select: usize) {
        let (faces, tree) = {
            let mut s = self.state.borrow_mut();
            let mut member_of: Vec<Vec<usize>> = vec![Vec::new(); s.n()];
            for (gi, g) in s.groups.iter().enumerate() {
                for f in &g.files {
                    member_of[f.id as usize].push(gi);
                }
            }
            s.member_of = member_of;
            let faces: Vec<PathBuf> = s.groups.iter().map(|g| face(&g.files).map(|f| f.path.clone()).unwrap_or_default()).collect();
            let files: Vec<(u32, &Path)> = (0..s.n()).filter(|&i| !s.member_of[i].is_empty()).map(|i| (i as u32, s.path(i))).collect();
            (faces, folders::build(&s.roots, &files))
        };
        self.folders.fill(tree);
        let n = faces.len();
        self.strip.fill(faces);
        for b in &self.group_buttons {
            b.set_sensitive(n > 0);
        }
        if n == 0 {
            self.clear_cards();
            self.shown.borrow_mut().clear();
            let s = self.state.borrow();
            self.title.set_text("");
            let mut text = format!("No duplicates found among {} image{}.", s.analysed, if s.analysed == 1 { "" } else { "s" });
            if s.problems {
                text.push_str(" The scan had problems with some files; see the scan log.");
            }
            self.note.set_text(&text);
            self.note.set_visible(true);
            drop(s);
            self.update_status();
            return;
        }
        let i = select.min(n - 1);
        self.strip.select(i);
        self.state.borrow_mut().current = i;
        self.show_view(true);
        self.update_status();
    }

    /// What the view has selected, on the page: the group, or the folder.
    /// With `reveal`, the list is brought to it too, the tree opened to the
    /// folder (its first top folder when none was shown yet).
    fn show_view(self: &Rc<Self>, reveal: bool) {
        match self.view.get() {
            View::Groups => {
                let current = self.state.borrow().current;
                self.show_group(current);
                if reveal {
                    self.strip.scroll_to(current);
                }
            }
            View::Folders => {
                let top = || self.folders.tree.borrow().top.first().copied();
                let Some(f) = (if reveal { self.folders.current.get().or_else(top) } else { self.folders.current.get() }) else { return };
                if reveal {
                    self.folders.open_to(f);
                }
                self.show_folder(f);
            }
        }
    }

    /// The group or folder `by` from the one shown, and the keyboard to its
    /// images.
    fn step(self: &Rc<Self>, by: i32) {
        let moved = match self.view.get() {
            View::Groups => self.strip.step(by),
            View::Folders => self.folders.step(by),
        };
        if moved {
            self.focus();
        }
    }

    /// The files of the group or folder shown: in the tree view, every image
    /// under the folder, on a tile or not.
    fn scope(&self) -> Vec<u32> {
        match (self.view.get(), self.folders.current.get()) {
            (View::Groups, _) => self.cards.borrow().iter().map(|c| c.id).collect(),
            (View::Folders, Some(f)) => {
                let s = self.state.borrow();
                self.folders.tree.borrow().folders.get(f).map_or_else(Vec::new, |f| f.all.iter().copied().filter(|&id| !s.gone(id as usize)).collect())
            }
            (View::Folders, None) => Vec::new(),
        }
    }

    /// Show the other view: the tree in place of the strip, or the strip
    /// again.
    fn switch_view(self: &Rc<Self>) {
        let view = self.view.get().other();
        self.view.set(view);
        fill_menu(&self.menu, view);
        self.left.set_visible_child_name(view.child());
        self.left_label.set_text_with_mnemonic(view.heading());
        self.left_label.set_mnemonic_widget(Some(&self.side_list()));
        let what = view.noun();
        self.prev.set_tooltip_text(Some(&format!("Previous {what} (Ctrl+Page Up)")));
        self.next.set_tooltip_text(Some(&format!("Next {what} (Ctrl+Page Down)")));
        if !self.state.borrow().groups.is_empty() {
            self.show_view(true);
        }
        self.side_list().grab_focus();
    }

    /// Row `item` of the tree now shows the folder its item names.
    fn bind_tree_row(&self, item: &gtk::ListItem) {
        let Some(row) = item.item().and_downcast::<gtk::TreeListRow>() else { return };
        let Some(f) = row.item().and_downcast::<glib::BoxedAnyObject>().map(|o| *o.borrow::<usize>()) else { return };
        let rows = self.folders.rows.borrow();
        let Some(r) = rows.get(item) else { return };
        r.expander.set_list_row(Some(&row));
        r.folder.set(Some(f));
        r.shown.set(None);
        let t = self.folders.tree.borrow();
        let Some(folder) = t.folders.get(f) else { return };
        let label = folder.label();
        r.name.set_text(&label);
        r.count.set_text(&folder.all.len().to_string());
        drop(t);
        self.show_tree_row(r);
        r.expander.update_property(&[gtk::accessible::Property::Label(&label)]);
    }

    /// A row of the tree says how many images under its folder are marked.
    fn show_tree_row(&self, r: &TreeRow) {
        let Some(f) = r.folder.get() else { return };
        let marked = {
            let (s, t) = (self.state.borrow(), self.folders.tree.borrow());
            t.folders.get(f).map_or(0, |f| f.all.iter().filter(|&&id| s.is_marked(id)).count())
        };
        if r.shown.replace(Some(marked)) == Some(marked) {
            return;
        }
        if marked > 0 {
            r.marked.set_text(&marked.to_string());
        }
        if r.marked.is_visible() != (marked > 0) {
            r.marked.set_visible(marked > 0);
        }
    }

    /// The images of folder `f`, its subfolders' included, on the page.
    fn show_folder(self: &Rc<Self>, f: usize) {
        let (path, own, all) = {
            let t = self.folders.tree.borrow();
            let Some(folder) = t.folders.get(f) else { return };
            (folder.path.clone(), folder.own, folder.all.clone())
        };
        self.folders.current.set(Some(f));
        let (files, problems) = {
            let s = self.state.borrow();
            let files: Vec<(Member, usize)> = all.iter().filter_map(|&id| s.member(id)).collect();
            (files, s.problems)
        };
        let n = all.len();
        let mut title = format!("{} · {n} image{}", path.display(), if n == 1 { "" } else { "s" });
        if own != n {
            title.push_str(&format!(" ({own} in this folder)"));
        }
        let mut note = String::new();
        if problems {
            note.push_str("The scan had problems with some files; see the scan log.");
        }
        self.show_files(files, &title, &note, Vec::new());
    }

    /// Row `item` of the strip now shows the group its item names.
    fn bind_row(&self, item: &gtk::ListItem) {
        let Some(gi) = item.item().and_downcast::<glib::BoxedAnyObject>().map(|o| *o.borrow::<usize>()) else { return };
        let strip = self.strip.rows.borrow();
        let Some(row) = strip.get(item) else { return };
        row.group.set(Some(gi));
        row.shown.set(None);
        self.show_row(row, gi);
        row.picture.set_texture(None);
        let Some(face) = self.strip.faces.borrow().get(gi).cloned() else { return };
        let scale = self.app.window.scale_factor().max(1) as u32;
        let size = Size::cover(STRIP_W as u32 * scale, STRIP_H as u32 * scale);
        let ticket = self.strip_thumbs.request(&face, size, Shown::Still(row.picture.clone()), false);
        *row.request.borrow_mut() = Some(((face, size), ticket));
    }

    /// Row `item` of the strip no longer shows a group: its picture is not
    /// wanted, and not held.
    fn unbind_row(&self, item: &gtk::ListItem) {
        let strip = self.strip.rows.borrow();
        let Some(row) = strip.get(item) else { return };
        row.group.set(None);
        if let Some((key, ticket)) = row.request.borrow_mut().take() {
            self.strip_thumbs.withdraw(&key, ticket);
        }
        row.picture.set_texture(None);
    }

    /// Group `gi`'s row of the strip, if one shows it, says what it holds.
    fn update_row(&self, gi: usize) {
        let strip = self.strip.rows.borrow();
        if let Some(row) = strip.values().find(|r| r.group.get() == Some(gi)) {
            self.show_row(row, gi);
        }
    }

    fn show_row(&self, r: &StripRow, gi: usize) {
        let s = self.state.borrow();
        let Some(g) = s.groups.get(gi) else { return };
        let marked = g.files.iter().filter(|f| s.is_marked(f.id)).count();
        let before = r.shown.replace(Some(marked));
        if before == Some(marked) {
            return;
        }
        if before.is_none() {
            r.count.set_text(&g.files.len().to_string());
        }
        if marked > 0 {
            r.marked.set_text(&marked.to_string());
        }
        if r.marked.is_visible() != (marked > 0) {
            r.marked.set_visible(marked > 0);
        }
        let mut text = format!("Group {} · {} images", gi + 1, g.files.len());
        if marked > 0 {
            text.push_str(&format!(" · {marked} marked"));
        }
        // No tooltip: the badges say the same, and setting seven hundred
        // tooltips was most of the time the strip took to build.
        r.overlay.update_property(&[gtk::accessible::Property::Label(&text)]);
    }

    fn clear_cards(&self) {
        self.select(None);
        self.anchor.set(None);
        // Every tile is told it shows nothing while the cards it showed are
        // still there to withdraw their pictures.
        self.mosaic.show(&[]);
        self.cards.borrow_mut().clear();
        self.hovered.set(None);
        self.refresh_bar();
    }

    fn show_group(self: &Rc<Self>, gi: usize) {
        let (files, reference_gone, n, problems) = {
            let mut s = self.state.borrow_mut();
            let Some(g) = s.groups.get(gi) else { return };
            let files = g.files.clone();
            let gone = g.reference_gone;
            s.current = gi;
            (files, gone, s.groups.len(), s.problems)
        };
        let title = format!("Group {} of {} · {} images", gi + 1, n, files.len());
        let mut note = String::new();
        if reference_gone {
            note.push_str("The image the others in this group were matched against has been moved to the Trash. The images left were not compared with each other. ");
        }
        if problems {
            note.push_str("The scan had problems with some files; see the scan log.");
        }
        // The next group, ahead of being asked for.
        let ahead: Vec<Member> = self.state.borrow().groups.get(gi + 1).map(|g| g.files.clone()).unwrap_or_default();
        self.show_files(files.into_iter().map(|m| (m, gi)).collect(), &title, &note, ahead);
    }

    /// `files`, each with the group it is taken from, on the page; `ahead`
    /// is decoded ahead of being asked for.
    fn show_files(self: &Rc<Self>, shown: Vec<(Member, usize)>, title: &str, note: &str, ahead: Vec<Member>) {
        self.thumbs.clear_queue();
        self.clear_cards();
        self.title.set_text(title);
        self.note.set_text(note.trim());
        self.note.set_visible(!note.trim().is_empty());
        let files: Vec<Member> = shown.iter().map(|(m, _)| m.clone()).collect();
        *self.shown.borrow_mut() = shown;

        let dims = |files: &[Member]| files.iter().map(|m| m.dims.unwrap_or((0, 0))).collect::<Vec<_>>();
        *self.cards.borrow_mut() = files.iter().map(|m| Card { path: m.path.clone(), id: m.id, request: None }).collect();
        // From the top, and the images there on tiles: the rest are given
        // theirs as the page is scrolled to them.
        self.scroll.vadjustment().set_value(0.0);
        self.mosaic.show(&dims(&files));
        // At the sizes it will be shown at.
        if !ahead.is_empty() {
            let sizes = self.picture_sizes(&dims(&ahead));
            for (i, m) in ahead.iter().enumerate().take(AHEAD) {
                let size = sizes.as_ref().map_or(Size::within(card_px(ahead.len()) * self.scale()), |s| s[i]);
                self.thumbs.request(&m.path, size, Shown::Callback(Box::new(|_| {})), false);
            }
        }
        // A tile left holding the keyboard may now be hidden, or show
        // another image: the keyboard goes back to the images only when it
        // was among them and nothing else has taken it (a click on the strip
        // changes group before the strip takes the keyboard).
        let focus = gtk::prelude::GtkWindowExt::focus(&self.app.window);
        if focus.is_some_and(|f| self.tiles.borrow().iter().any(|t| t.child.upcast_ref::<gtk::Widget>() == &f)) {
            self.side_list().grab_focus();
        }
        self.refresh_bar();
    }

    fn scale(&self) -> u32 {
        self.app.window.scale_factor().max(1) as u32
    }

    /// The size each picture of a group of `dims` is drawn at, in device
    /// pixels: its tile's, less the tile's border. `None` before the page
    /// has been laid out.
    fn picture_sizes(&self, dims: &[(u32, u32)]) -> Option<Vec<Size>> {
        let tiles = self.mosaic.tile_sizes(dims)?;
        // The border is the tiles' style's: measured off a tile laid out,
        // whose width is its content's.
        let border = self.border();
        let scale = self.scale();
        Some(tiles.iter().map(|&(w, h)| Size::cover((w - border.0).max(1) as u32 * scale, (h - border.1).max(1) as u32 * scale)).collect())
    }

    /// A tile's style's border, both ways: measured off a tile laid out,
    /// whose width is its content's.
    fn border(&self) -> (i32, i32) {
        for (slot, t) in self.tiles.borrow().iter().enumerate() {
            if let Some((w, h)) = self.mosaic.allocated(slot)
                && t.still.width() > 0
            {
                return (w - t.still.width(), h - t.still.height());
            }
        }
        (6, 6)
    }

    /// The shortest row a tile can take: its border, the mark's tick with
    /// its margin at the top, and a line of the badge's text with its margin
    /// at the bottom, which lie at opposite corners and must not meet.
    fn least_row(&self) -> f32 {
        let tiles = self.tiles.borrow();
        let Some(t) = tiles.first() else { return 1.0 };
        let mut tick = 0;
        let mut c = t.child.first_child();
        while let Some(w) = c {
            if w.has_css_class("tick") {
                tick = w.measure(gtk::Orientation::Vertical, -1).0 + w.margin_top();
            }
            c = w.next_sibling();
        }
        // Measured from its text, since it is hidden on most tiles.
        let badge = t.pill.create_pango_layout(Some("Reference")).pixel_size().1 + t.pill.margin_bottom();
        (self.border().1 + tick + badge) as f32
    }

    /// The tile image `i` is on, if it is on one.
    fn tile_of(&self, i: usize) -> Option<Tile> {
        self.mosaic.slot_of(i).and_then(|s| self.tiles.borrow().get(s).cloned())
    }

    /// Tile `slot` now shows image `item`, or nothing: the picture it was
    /// asked for is withdrawn, and the new one asked for at the tile's size.
    fn bind_tile(&self, slot: usize, item: Option<usize>) {
        let Some(t) = self.tiles.borrow().get(slot).cloned() else { return };
        if let Some(old) = t.item.replace(item) {
            if let Some((key, ticket)) = self.cards.borrow_mut().get_mut(old).and_then(|c| c.request.take()) {
                self.thumbs.withdraw(&key, ticket);
            }
            if self.hovered.get() == Some(old) {
                self.hovered.set(None);
            }
            t.still.set_texture(None);
        }
        let Some(i) = item else { return };
        let Some((m, _)) = self.shown.borrow().get(i).cloned() else { return };
        self.dress(&t, &m, self.selected.get() == Some(i));
        let n = self.cards.borrow().len();
        let size = match self.mosaic.rect(i) {
            Some((_, _, w, h)) => {
                let b = self.border();
                Size::cover((w - b.0).max(1) as u32 * self.scale(), (h - b.1).max(1) as u32 * self.scale())
            }
            None => Size::within(card_px(n) * self.scale()),
        };
        let ticket = self.thumbs.request(&m.path, size, Shown::Still(t.still.clone()), false);
        if let Some(c) = self.cards.borrow_mut().get_mut(i) {
            c.request = Some(((m.path.clone(), size), ticket));
        }
    }

    /// Bring image `i` onto the part of the page shown.
    fn scroll_into_view(&self, i: usize) {
        let Some((_, y, _, h)) = self.mosaic.rect(i) else { return };
        let a = self.scroll.vadjustment();
        let (y, h, top, page) = (y as f64, h as f64, a.value(), a.page_size());
        if y < top {
            a.set_value((y - 6.0).max(0.0));
        } else if y + h > top + page {
            a.set_value(y + h - page + 6.0);
        }
    }

    /// The tiles have been laid out again: each picture made for another size
    /// is asked for at its tile's. The picture shown stays until it comes.
    fn tiles_resized(&self) {
        let scale = self.scale();
        let tiles = self.tiles.borrow();
        let mut cards = self.cards.borrow_mut();
        for (slot, item) in self.mosaic.bound() {
            let (Some(t), Some(c)) = (tiles.get(slot), cards.get_mut(item)) else { continue };
            let (w, h) = (t.still.width(), t.still.height());
            if w <= 0 || h <= 0 {
                continue;
            }
            let size = Size::cover(w as u32 * scale, h as u32 * scale);
            if c.request.as_ref().is_some_and(|(k, _)| k.1 == size) {
                continue;
            }
            if let Some((key, ticket)) = c.request.take() {
                self.thumbs.withdraw(&key, ticket);
            }
            let ticket = self.thumbs.request(&c.path, size, Shown::Still(t.still.clone()), false);
            c.request = Some(((c.path.clone(), size), ticket));
        }
    }

    /// Tile `t`, dressed for `m`.
    fn dress(&self, t: &Tile, m: &Member, picked: bool) {
        let name = file_name(&m.path);
        t.still.set_label(&name);
        if t.still.has_tooltip() {
            t.still.set_tooltip_text(None);
        }
        let (review, marked) = {
            let s = self.state.borrow();
            (s.to_look_at(m.id), s.is_marked(m.id))
        };
        if t.look.is_visible() != review {
            t.look.set_visible(review);
        }
        set_class(&t.child, "review", review);
        set_class(&t.child, "marked", marked);
        set_class(&t.child, "picked", picked);
        let mut pills = Vec::new();
        if m.is_representative() {
            pills.push("Reference");
        }
        if m.damaged {
            pills.push("Damaged");
        }
        t.pill.set_text(&pills.join(" · "));
        t.pill.set_visible(!pills.is_empty());
        t.child.update_property(&[gtk::accessible::Property::Label(&name)]);
    }

    /// One more tile for the pool, with what it does to whichever image it
    /// shows.
    fn new_tile(self: &Rc<Self>) -> Tile {
        let item: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        let still = Still::new(0, 0);
        let child = gtk::Overlay::builder().child(&still).focusable(true).overflow(gtk::Overflow::Hidden).css_classes(["tile"]).build();
        let tick = gtk::Button::builder()
            .icon_name("object-select-symbolic")
            .css_classes(["tick"])
            .focusable(false)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .margin_start(6)
            .margin_top(6)
            // One wording for both states: a tooltip costs a quarter of a
            // millisecond to change, and marking a group's worth at once
            // changed ninety of them.
            .tooltip_text("Mark or unmark for the Trash (Space)")
            .build();
        // A marked picture is dimmed by a veil drawn over it rather than by
        // its own opacity, which the software renderer pays for by drawing
        // each picture apart first: some 70 ms of frame for a group of ninety
        // marked at once.
        let veil = gtk::Box::builder().css_classes(["veil"]).can_target(false).build();
        child.add_overlay(&veil);
        child.add_overlay(&tick);
        let look = gtk::Box::builder().css_classes(["look"]).halign(gtk::Align::End).valign(gtk::Align::Start).tooltip_text(REVIEW_TIP).visible(false).build();
        child.add_overlay(&look);
        // Shortened rather than cut off on a tile narrower than it, which a
        // tall picture's is once the images are made small (`Mosaic::zoom`).
        let pill = gtk::Label::builder()
            .css_classes(["pill"])
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .margin_start(6)
            .margin_end(6)
            .margin_bottom(6)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .visible(false)
            .build();
        child.add_overlay(&pill);
        tick.connect_clicked({
            let (weak, item) = (Rc::downgrade(self), item.clone());
            move |_| {
                if let (Some(me), Some(i)) = (weak.upgrade(), item.get()) {
                    me.select(Some(i));
                    me.toggle(i);
                }
            }
        });
        // Reached by the keyboard, or clicked anywhere on it: either way it
        // is the image the menu now means, and the bar shows it.
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter({
            let (weak, item) = (Rc::downgrade(self), item.clone());
            move |_| {
                if let (Some(me), Some(i)) = (weak.upgrade(), item.get()) {
                    me.select(Some(i));
                    me.refresh_bar();
                }
            }
        });
        child.add_controller(focus);
        let click = gtk::GestureClick::builder().button(0).propagation_phase(gtk::PropagationPhase::Capture).build();
        click.connect_pressed({
            let (weak, item) = (Rc::downgrade(self), item.clone());
            let child = child.downgrade();
            move |g, n, x, y| {
                let (Some(me), Some(child), Some(i)) = (weak.upgrade(), child.upgrade(), item.get()) else { return };
                // Shift and a click, on the picture or its tick, marks the
                // range from the anchor. Claimed here, so the tick under the
                // pointer does not toggle the image a second time.
                let shift = g.current_event_state().contains(gdk::ModifierType::SHIFT_MASK);
                if shift && n == 1 && g.current_button() == gdk::BUTTON_PRIMARY {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    me.mark_range(i);
                    me.follow_pointer.set(true);
                    child.grab_focus();
                    return;
                }
                me.select(Some(i));
                me.follow_pointer.set(true);
                child.grab_focus();
                match g.current_button() {
                    gdk::BUTTON_SECONDARY => me.popup_menu(i, Some((x, y))),
                    gdk::BUTTON_PRIMARY if n == 2 => me.preview(i),
                    _ => {}
                }
            }
        });
        child.add_controller(click);
        let motion = gtk::EventControllerMotion::new();
        motion.connect_enter({
            let (weak, item) = (Rc::downgrade(self), item.clone());
            move |_, _, _| {
                if let (Some(me), Some(i)) = (weak.upgrade(), item.get()) {
                    me.hovered.set(Some(i));
                    me.follow_pointer.set(true);
                    me.refresh_bar();
                }
            }
        });
        motion.connect_leave({
            let (weak, item) = (Rc::downgrade(self), item.clone());
            move |_| {
                if let Some(me) = weak.upgrade() {
                    if item.get().is_some() && me.hovered.get() == item.get() {
                        me.hovered.set(None);
                    }
                    me.refresh_bar();
                }
            }
        });
        child.add_controller(motion);
        Tile { child, still, look, pill, item }
    }

    /// The image menu at card `i`: at `at` in the card, or at its middle.
    fn popup_menu(&self, i: usize, at: Option<(f64, f64)>) {
        let Some(child) = self.tile_of(i).map(|t| t.child) else { return };
        self.select(Some(i));
        let (x, y) = at.unwrap_or((child.width() as f64 / 2.0, child.height() as f64 / 2.0));
        #[allow(deprecated)]
        let Some((mx, my)) = child.translate_coordinates(&self.mosaic, x, y) else { return };
        self.context.set_pointing_to(Some(&gdk::Rectangle::new(mx as i32, my as i32, 1, 1)));
        self.context.popup();
    }

    /// Select card `i`, or none: outline it, and let the menu's image items
    /// act on it.
    fn select(&self, i: Option<usize>) {
        if let Some(t) = self.selected.get().and_then(|j| self.tile_of(j)) {
            t.child.remove_css_class("picked");
        }
        let i = i.filter(|&i| i < self.cards.borrow().len());
        if let Some(t) = i.and_then(|i| self.tile_of(i)) {
            t.child.add_css_class("picked");
        }
        self.selected.set(i);
        for a in self.image_actions.borrow().iter() {
            a.set_enabled(i.is_some());
        }
    }

    /// Select card `i`, clamped to the group, give it the keyboard, and show
    /// it in the bar even while the pointer rests on another image.
    fn move_to(&self, i: i64) {
        let n = self.cards.borrow().len() as i64;
        if n == 0 {
            return;
        }
        let i = i.clamp(0, n - 1) as usize;
        self.select(Some(i));
        self.follow_pointer.set(false);
        // On a tile now, if it was scrolled away from.
        self.scroll_into_view(i);
        self.mosaic.rebind_now();
        if let Some(t) = self.tile_of(i) {
            t.child.grab_focus();
        }
        self.refresh_bar();
    }

    /// The bar: the image under the pointer or the selected one, whichever
    /// moved last, else the marks.
    fn refresh_bar(&self) {
        let pointer = if self.follow_pointer.get() { self.hovered.get() } else { None };
        let shown = if self.trashing.get() { None } else { pointer.or(self.selected.get()) };
        let member = shown.and_then(|i| self.shown.borrow().get(i).cloned());
        let Some((m, gi)) = member else {
            self.bar.set_visible_child_name("summary");
            return;
        };
        let s = self.state.borrow();
        let d = &self.details;
        d.name.set_text(&file_name(&m.path));
        d.dir.set_text(&m.path.parent().map(|p| p.display().to_string()).unwrap_or_default());
        d.facts.set_text(&facts(&m));
        let reference = s.groups.get(gi).and_then(|g| g.files.iter().find(|f| f.is_representative()));
        let mut note = match (m.is_representative(), reference.and_then(|r| r.dims)) {
            (false, Some((w, h))) => format!("the reference is {w} × {h}"),
            _ => String::new(),
        };
        // In the tree view, which group the match is in.
        if self.view.get() == View::Folders {
            let groups = s.member_of.get(m.id as usize).map_or(0, Vec::len);
            let more = if groups > 1 { format!(" (and {} more)", groups - 1) } else { String::new() };
            note = [format!("group {}{more}", gi + 1), note].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ");
        }
        d.facts_note.set_text(&note);
        d.why.set_text(&evidence(&m));
        let action = s.action(m.id);
        let kept = match s.scan.kept_copy(m.id as usize) {
            Some(k) if s.mode == 0 => held_by(&m.path, s.path(k)),
            _ => String::new(),
        };
        let kept = match (kept.is_empty(), s.changed(m.id), s.mode == 0, s.unheld_of(m.id)) {
            (true, true, _, _) => "It has changed since the scan".to_string(),
            (true, _, true, Some(Lapse::Gone)) => "The file that held everything in it has gone".to_string(),
            (true, _, true, Some(Lapse::Changed)) => "The file that held everything in it has changed since the scan".to_string(),
            _ => kept,
        };
        d.why_note.set_text(&kept);
        // Whole, for a folder the bar has cut short.
        d.why_note.set_tooltip_text(Some(&kept).filter(|k| !k.is_empty()).map(|k| k.as_str()));
        let said = s.said(m.id);
        d.suggestion.set_text(said);
        // Hidden by opacity, not visibility, so that the stack keeps showing it.
        d.suggestion.set_opacity(if action.is_some() { 1.0 } else { 0.0 });
        for c in ["keep", "delete", "review"] {
            d.suggestion.remove_css_class(c);
        }
        d.suggestion.add_css_class(match action {
            Some(Action::Keep) => "keep",
            Some(Action::Delete) => "delete",
            _ => "review",
        });
        let marked = s.is_marked(m.id);
        drop(s);
        if marked {
            d.suggestion.set_text(&format!("{said} · marked"));
        }
        self.bar.set_visible_child_name("details");
    }

    fn toggle(self: &Rc<Self>, i: usize) {
        let Some(id) = self.cards.borrow().get(i).map(|c| c.id) else { return };
        let now = !self.state.borrow().is_marked(id);
        self.set_marked(i, now);
    }

    /// Mark or unmark card `i`'s file, everywhere it is shown, and make it
    /// the anchor.
    fn set_marked(self: &Rc<Self>, i: usize, on: bool) {
        let Some(id) = self.cards.borrow().get(i).map(|c| c.id) else { return };
        self.anchor.set(Some((i, on)));
        self.mark_ids(&[id], on);
        self.update_status();
        self.refresh_bar();
    }

    /// Shift and a click on card `i`: it is marked or unmarked as a click on
    /// its tick would, and every image from the anchor to it, in the order
    /// the arrows follow, is given the same. With no anchor, from the
    /// selected image; with neither, `i` alone. `i` is the anchor after.
    fn mark_range(self: &Rc<Self>, i: usize) {
        let Some(id) = self.cards.borrow().get(i).map(|c| c.id) else { return };
        let on = !self.state.borrow().is_marked(id);
        let from = self.anchor.get().map(|(a, _)| a).or(self.selected.get()).unwrap_or(i);
        self.mark_span(from, i, on);
        self.select(Some(i));
        self.refresh_bar();
    }

    /// Shift and an arrow from card `from` to `to`: move there, and give
    /// every image from `from` to it the anchor's mark. With no anchor,
    /// `from` is toggled first and becomes it, so the first Shift and arrow
    /// starts a run the way Space would.
    fn mark_along(self: &Rc<Self>, from: usize, to: i64) {
        let n = self.cards.borrow().len() as i64;
        if n == 0 {
            return;
        }
        let to = to.clamp(0, n - 1) as usize;
        let on = match self.anchor.get() {
            Some((_, on)) => on,
            None => {
                let Some(id) = self.cards.borrow().get(from).map(|c| c.id) else { return };
                !self.state.borrow().is_marked(id)
            }
        };
        self.mark_span(from, to, on);
        self.move_to(to as i64);
    }

    /// Cards `a` to `b`, either way round and both included, marked or
    /// unmarked, with `b` the anchor.
    fn mark_span(self: &Rc<Self>, a: usize, b: usize, on: bool) {
        let ids: Vec<u32> = {
            let cards = self.cards.borrow();
            let (lo, hi) = (a.min(b), a.max(b).min(cards.len().saturating_sub(1)));
            cards.get(lo..=hi).map_or_else(Vec::new, |c| c.iter().map(|c| c.id).collect())
        };
        self.anchor.set(Some((b, on)));
        self.mark_ids(&ids, on);
        self.update_status();
    }

    /// Mark or unmark every file of `ids`, then show it: each image shown
    /// and each group's row once, however many of the files it holds.
    ///
    /// One file at a time, every file recounted its group's marks and rewrote
    /// its row, and every file searched the images shown: Mark suggested
    /// deletions on IMGS-ALL, 17,850 files, held the window for 0.9 s, and
    /// Unmark all groups as long.
    fn mark_ids(self: &Rc<Self>, ids: &[u32], on: bool) {
        let groups: HashSet<usize> = {
            let mut s = self.state.borrow_mut();
            let mut groups = HashSet::new();
            for &id in ids {
                if s.set_mark(id, on) {
                    groups.extend(s.member_of.get(id as usize).into_iter().flatten().copied());
                }
            }
            groups
        };
        {
            let s = self.state.borrow();
            let (cards, tiles) = (self.cards.borrow(), self.tiles.borrow());
            for (slot, item) in self.mosaic.bound() {
                if let (Some(c), Some(t)) = (cards.get(item), tiles.get(slot)) {
                    set_class(&t.child, "marked", s.is_marked(c.id));
                }
            }
        }
        if !groups.is_empty() {
            self.marks_changed();
            for r in self.folders.rows.borrow().values() {
                self.show_tree_row(r);
            }
        }
        for gi in groups {
            self.update_row(gi);
        }
    }

    fn mark_all_but_selected(self: &Rc<Self>) {
        let Some(keep) = self.selected.get() else { return };
        let Some(this) = self.cards.borrow().get(keep).map(|c| c.id) else { return };
        let others: Vec<u32> = self.scope().into_iter().filter(|&id| id != this).collect();
        self.mark_ids(&[this], false);
        self.mark_ids(&others, true);
        self.update_status();
        // Back to the image that was kept, so the keyboard carries on there.
        self.move_to(keep as i64);
        self.refresh_bar();
    }

    /// Make the marks the chosen rule's deletions, exactly: every file it
    /// suggests deleting is marked and every other mark goes, whoever made
    /// it, so a second rule replaces the first rather than adding to it.
    fn mark_suggested(self: &Rc<Self>) {
        self.check_changed();
        let (off, on): (Vec<u32>, Vec<u32>) = {
            let s = self.state.borrow();
            let delete = |id: u32| s.action(id) == Some(Action::Delete);
            let all = (0..s.n() as u32).filter(|&id| !s.gone(id as usize));
            all.filter(|&id| s.is_marked(id) != delete(id)).partition(|&id| s.is_marked(id))
        };
        self.mark_ids(&off, false);
        self.mark_ids(&on, true);
        self.update_status();
        self.refresh_bar();
    }

    /// Suggest by `MODES[mode]` from now on, and show it.
    fn set_mode(self: &Rc<Self>, mode: usize) {
        {
            let mut s = self.state.borrow_mut();
            if s.mode == mode {
                return;
            }
            s.mode = mode;
        }
        let selected = self.selected.get();
        self.show_view(false);
        self.select(selected);
        self.refresh_bar();
        self.update_status();
    }

    /// Mark or unmark the group shown, or every image under the folder
    /// shown.
    fn mark_shown(self: &Rc<Self>, on: bool) {
        let ids = self.scope();
        self.mark_ids(&ids, on);
        self.update_status();
        self.refresh_bar();
    }

    /// Unmark every image, in every group.
    fn unmark_all(self: &Rc<Self>) {
        let ids: Vec<u32> = self.state.borrow().marked_ids().collect();
        self.mark_ids(&ids, false);
        self.update_status();
        self.refresh_bar();
    }

    fn update_status(&self) {
        // The trash in progress owns the status line and the button, and says
        // how it went when it is done.
        if self.trashing.get() {
            return;
        }
        let s = self.state.borrow();
        let (n, bytes) = (s.marked_n, s.marked_bytes);
        let groups = s.groups.len();
        let live = || (0..s.n() as u32).filter(|&id| !s.gone(id as usize));
        let suggested = live().filter(|&id| s.action(id) == Some(Action::Delete)).count();
        let text = if n == 0 && suggested > 0 {
            format!("{groups} group{}. {suggested} suggested for deletion.", if groups == 1 { "" } else { "s" })
        } else if n == 0 {
            format!("{groups} group{}. Nothing marked.", if groups == 1 { "" } else { "s" })
        } else {
            format!("{n} image{} marked, {}.", if n == 1 { "" } else { "s" }, size(bytes))
        };
        self.status.set_text(&text);
        self.trash.set_sensitive(n > 0);
        self.trash.set_label(&if n == 0 { l::TRASH.to_string() } else { l::trash_n(n) });
        // Off only when the marks already are the suggestion.
        let same = live().all(|id| s.is_marked(id) == (s.action(id) == Some(Action::Delete)));
        self.mark_suggested.set_sensitive(!same);
    }

    /// Open the selected image, or its folder.
    fn launch(self: &Rc<Self>, folder: bool) {
        let Some(i) = self.selected.get() else { return };
        let Some(path) = self.cards.borrow().get(i).map(|c| c.path.clone()) else { return };
        let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(&path)));
        let window = self.app.window.clone();
        let app = self.app.clone();
        let done = move |r: Result<(), glib::Error>| {
            if let Err(e) = r {
                app.log_line(&format!("could not open {}: {e}", path.display()));
            }
        };
        if folder {
            launcher.open_containing_folder(Some(&window), gio::Cancellable::NONE, done);
        } else {
            launcher.launch(Some(&window), gio::Cancellable::NONE, done);
        }
    }

    // ------------------------------------------------------------ large view

    fn preview(self: &Rc<Self>, start: usize) {
        let picture = gtk::Picture::builder().content_fit(gtk::ContentFit::Contain).can_shrink(true).vexpand(true).hexpand(true).build();
        // Beside it, when comparing, its group's reference image; over each,
        // the part it shows of the other (`Outline`).
        let reference = gtk::Picture::builder().content_fit(gtk::ContentFit::Contain).can_shrink(true).vexpand(true).hexpand(true).build();
        let (here, there) = (Outline::over(&picture), Outline::over(&reference));
        let caption = || gtk::Label::builder().xalign(0.5).ellipsize(gtk::pango::EllipsizeMode::Middle).visible(false).build();
        let (this_caption, reference_caption) = (caption(), caption());
        let side = |o: &Outline, c: &gtk::Label| {
            let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
            b.append(&o.overlay);
            b.append(c);
            b
        };
        let reference_side = side(&there, &reference_caption);
        reference_side.set_visible(false);
        let pictures = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(8).homogeneous(true).vexpand(true).build();
        pictures.append(&reference_side);
        pictures.append(&side(&here, &this_caption));
        let info = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::Middle).build();
        let prev = gtk::Button::with_mnemonic(l::PREVIEW_PREV);
        prev.set_tooltip_text(Some("Left arrow"));
        let next = gtk::Button::with_mnemonic(l::PREVIEW_NEXT);
        next.set_tooltip_text(Some("Right arrow"));
        let compare = gtk::ToggleButton::with_mnemonic(l::PREVIEW_COMPARE);
        compare.set_tooltip_text(Some("C"));
        let mark = gtk::CheckButton::with_mnemonic(l::PREVIEW_MARK);
        mark.set_tooltip_text(Some("Space"));
        let close = gtk::Button::with_mnemonic(l::PREVIEW_CLOSE);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.set_margin_start(12);
        bar.set_margin_end(12);
        bar.set_margin_top(8);
        bar.set_margin_bottom(12);
        for w in [info.upcast_ref::<gtk::Widget>(), prev.upcast_ref(), next.upcast_ref(), compare.upcast_ref(), mark.upcast_ref(), close.upcast_ref()] {
            bar.append(w);
        }
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&pictures);
        body.append(&bar);
        let win = gtk::Window::builder()
            .transient_for(&self.app.window)
            .modal(true)
            .default_width(1100)
            .default_height(800)
            .child(&body)
            .build();

        let at = Rc::new(Cell::new(start));
        let quiet = Rc::new(Cell::new(false));
        let (latest, latest_reference) = (Latest::default(), Latest::default());
        let show = {
            let (me, picture, reference, info, mark, win, at, quiet) = (Rc::downgrade(self), picture.clone(), reference.clone(), info.clone(), mark.clone(), win.clone(), at.clone(), quiet.clone());
            let (here, there, compare, reference_side, this_caption, reference_caption) =
                (here.clone(), there.clone(), compare.clone(), reference_side.clone(), this_caption.clone(), reference_caption.clone());
            Rc::new(move || {
                let Some(me) = me.upgrade() else { return };
                let cards = me.cards.borrow();
                let n = cards.len();
                let Some(card) = cards.get(at.get()) else { return };
                let path = card.path.clone();
                let s = me.state.borrow();
                let shown = me.shown.borrow().get(at.get()).cloned();
                let member = shown.as_ref().map(|(m, _)| m.clone());
                let marked = member.as_ref().is_some_and(|m| s.is_marked(m.id));
                // What it is compared with: its group's reference image, while
                // that is still there, and where the scan placed one in the
                // other (a scan kept before 0.62 did not keep that).
                let pair = shown.as_ref().and_then(|(m, gi)| {
                    let g = s.groups.get(*gi).filter(|g| !g.reference_gone)?;
                    let r = g.files.first().filter(|r| r.is_representative() && r.id != m.id)?;
                    Some((r.clone(), m.at?))
                });
                drop(s);
                drop(shown);
                drop(cards);
                win.set_title(Some(&format!("{} ({} of {n})", file_name(&path), at.get() + 1)));
                let text = match &member {
                    Some(m) => {
                        let said = me.state.borrow().said(m.id);
                        format!("{}  ·  {}  ·  {}  ·  {}", path.display(), facts(m), evidence(m), said)
                    }
                    None => path.display().to_string(),
                };
                info.set_text(&text);
                info.set_tooltip_text(Some(&text));
                quiet.set(true);
                mark.set_active(marked);
                quiet.set(false);
                let scale = win.scale_factor().max(1) as u32;
                // Every picture this view asks for is for the same widget, and
                // the answers come back in whatever order they are decoded; only
                // the one asked for last is shown. See `Latest`.
                let ask = |p: &gtk::Picture, path: &Path, latest: &Latest| {
                    p.set_paintable(None::<&gdk::Paintable>);
                    let current = latest.next();
                    let pic = p.downgrade();
                    let shown = Shown::Callback(Box::new(move |r| {
                        if let (true, Some(p)) = (current(), pic.upgrade()) {
                            show_on(&p, r);
                        }
                    }));
                    me.thumbs.request(path, Size::within(LARGE * scale), shown, true);
                };
                ask(&picture, &path, &latest);
                let comparing = compare.is_active();
                match pair.filter(|_| comparing) {
                    Some((r, place)) => {
                        ask(&reference, &r.path, &latest_reference);
                        let dims = member.as_ref().and_then(|m| m.dims);
                        let (on_this, on_reference) = overlays(&place, r.dims, dims);
                        here.set(Some(on_this));
                        there.set(on_reference);
                        reference_caption.set_text(&format!("Reference image: {}  ·  {}", file_name(&r.path), facts(&r)));
                        // How the reference lies in it, in words, beside the arrows.
                        let how = match (r.dims, dims) {
                            (Some(from), Some(to)) => turned(&place, from, to),
                            _ => String::new(),
                        };
                        let how = if how.is_empty() { how } else { format!("  ·  {how} from the reference") };
                        this_caption.set_text(&format!("This image: {}{how}", member.as_ref().map(facts).unwrap_or_default()));
                    }
                    None => {
                        here.set(None);
                        there.set(None);
                        // Nothing to set beside it: say why, where the
                        // reference would be.
                        let why = match &member {
                            _ if !comparing => "",
                            Some(m) if m.is_representative() => "This is the reference image the others were matched against.",
                            Some(m) if m.with.is_some() && m.at.is_none() => "Scan again to compare: this scan did not keep where the images lie in each other.",
                            _ => "Its group's reference image has been moved to the Trash.",
                        };
                        this_caption.set_text(why);
                    }
                }
                let both = comparing && here.shows();
                reference_side.set_visible(both);
                reference_caption.set_visible(both);
                this_caption.set_visible(comparing && !this_caption.text().is_empty());
            })
        };
        compare.connect_toggled({
            let show = show.clone();
            move |_| show()
        });
        let go = {
            let (me, at, show) = (Rc::downgrade(self), at.clone(), show.clone());
            Rc::new(move |by: i32| {
                let Some(me) = me.upgrade() else { return };
                let n = me.cards.borrow().len() as i32;
                if n == 0 {
                    return;
                }
                at.set((at.get() as i32 + by).clamp(0, n - 1) as usize);
                show();
            })
        };
        prev.connect_clicked({
            let go = go.clone();
            move |_| go(-1)
        });
        next.connect_clicked({
            let go = go.clone();
            move |_| go(1)
        });
        mark.connect_toggled({
            let (me, at, quiet) = (Rc::downgrade(self), at.clone(), quiet.clone());
            move |c| {
                if quiet.get() {
                    return;
                }
                if let Some(me) = me.upgrade() {
                    me.set_marked(at.get(), c.is_active());
                }
            }
        });
        close.connect_clicked({
            let win = win.clone();
            move |_| win.close()
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let (go, mark, compare, win) = (go.clone(), mark.clone(), compare.clone(), win.clone());
            move |_, key, _, mods| {
                match commands::LARGE.find(key, mods) {
                    Some(Large::Close) => win.close(),
                    Some(Large::Previous) => go(-1),
                    Some(Large::Next) => go(1),
                    Some(Large::Mark) => mark.set_active(!mark.is_active()),
                    Some(Large::Compare) => compare.set_active(!compare.is_active()),
                    None => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        });
        win.add_controller(keys);
        crate::keep_mnemonics_visible(&win);
        // Back where the keyboard was, on the image last shown.
        win.connect_close_request({
            let (me, at) = (Rc::downgrade(self), at.clone());
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.move_to(at.get() as i64);
                }
                glib::Propagation::Proceed
            }
        });
        show();
        win.present();
        next.grab_focus();
    }

    // ------------------------------------------------------------ the Trash

    fn confirm_trash(self: &Rc<Self>) {
        if self.trashing.get() {
            return;
        }
        // What the suggestions rest on, as it is now.
        self.check_changed();
        let (paths, bytes, whole, changed_after, with_copy, kept_changed) = {
            let s = self.state.borrow();
            // With the length the scan found, which the move checks first.
            let mut paths: Vec<(PathBuf, u64)> = s.marked_ids().map(|id| (s.path(id as usize).to_path_buf(), s.bytes(id as usize))).collect();
            let changed_after = s.changed_after;
            paths.sort();
            let bytes = s.marked_bytes;
            // The groups, by the number the list shows them under, in which
            // every image is marked.
            let whole: Vec<usize> = s
                .groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.files.iter().all(|f| s.is_marked(f.id)))
                .map(|(gi, _)| gi + 1)
                .collect();
            let kept_changed = s.marked_ids().filter(|&id| s.unheld_of(id) == Some(Lapse::Changed)).count();
            (paths, bytes, whole, changed_after, s.marked_with_their_copy(), kept_changed)
        };
        if paths.is_empty() {
            return;
        }
        let n = paths.len();
        let mut detail = format!(
            "{} {}. {} can be restored from the Trash.",
            if n == 1 { "It takes" } else { "Together they take" },
            size(bytes),
            if n == 1 { "It" } else { "They" }
        );
        if !whole.is_empty() {
            let one = whole.len() == 1;
            detail.push_str(&format!(
                "\n\nIn {} {} every image is marked, so no copy of {} would be left.",
                if one { "group" } else { "groups" },
                numbers(&whole),
                if one { "that picture" } else { "those pictures" }
            ));
        }
        // Groups are a reference image and its matches, and they overlap, so
        // a picture can lose its last whole copy with no group all marked: an
        // original and its re-encode marked, a crop of it left. What the
        // content rule knows is which file holds which.
        if with_copy > 0 {
            let one = with_copy == 1;
            detail.push_str(&format!(
                "\n\n{with_copy} of {} {} suggested for deletion because another image shows all of {}, and that image is marked too: \
                 moved together, nothing the scan found would be left holding what {}.",
                if n == 1 { "it" } else { "them" },
                if one { "is" } else { "are" },
                if one { "it" } else { "them" },
                if one { "it shows" } else { "they show" }
            ));
        }
        // A kept copy that has changed since the scan may no longer hold what
        // the deletion rested on: such a file is no longer suggested for
        // deletion, but a mark made before is the person's own.
        if kept_changed > 0 {
            let one = kept_changed == 1;
            detail.push_str(&format!(
                "\n\n{kept_changed} of {} {} suggested for deletion because another image showed all of {}, and that image has \
                 changed since the scan: it may no longer.",
                if n == 1 { "it" } else { "them" },
                if one { "was" } else { "were" },
                if one { "it" } else { "them" },
            ));
        }
        let dialog = gtk::AlertDialog::builder()
            .message(format!("Move {n} image{} to the Trash?", if n == 1 { "" } else { "s" }))
            .detail(detail)
            .buttons([l::CONFIRM_CANCEL, l::CONFIRM_TRASH])
            .cancel_button(0)
            .default_button(0)
            .modal(true)
            .build();
        let me = self.clone();
        glib::spawn_future_local(async move {
            if matches!(dialog.choose_future(Some(&me.app.window)).await, Ok(1)) {
                me.trash(paths, changed_after).await;
            }
        });
    }

    async fn trash(self: &Rc<Self>, paths: Vec<(PathBuf, u64)>, changed_after: Option<u64>) {
        if self.trashing.replace(true) {
            return;
        }
        self.trash.set_sensitive(false);
        self.mark_suggested.set_sensitive(false);
        self.refresh_bar();
        self.settings_button.set_sensitive(false);
        let n = paths.len();
        let mut gone: HashSet<PathBuf> = HashSet::new();
        let mut failed: Vec<String> = Vec::new();
        for (i, (p, scanned)) in paths.iter().enumerate() {
            self.status.set_text(&format!("Moving to the Trash: {} of {n}…", i + 1));
            // A file at that path that is not the length the scan found is
            // not the file the scan judged: replaced since, or, for a scan
            // kept from an earlier session, another file under the same
            // name. Nor is one changed since the scan started, whatever its
            // length: its change time is the kernel's, which `cp -p` or
            // `rsync -a` cannot put back as they put back a modification
            // time (a rename or a `chmod` since counts too, and leaves the
            // file where it is, which is the side to err on). It stays where
            // it is.
            if let Ok(md) = std::fs::metadata(p) {
                if img_fp::last::changed_since(&md, *scanned, changed_after) {
                    failed.push(format!("{}: changed since the scan, so it was left where it is", p.display()));
                    continue;
                }
            }
            match gio::File::for_path(p).trash_future(glib::Priority::DEFAULT).await {
                Ok(()) => {
                    gone.insert(p.clone());
                }
                // Already gone is as good as moved.
                Err(e) if e.matches(gio::IOErrorEnum::NotFound) => {
                    gone.insert(p.clone());
                }
                Err(e) => failed.push(format!("{}: {}", p.display(), e.message())),
            }
        }
        self.trashing.set(false);
        self.settings_button.set_sensitive(true);
        for p in &gone {
            self.app.log_line(&format!("moved to the Trash: {}", p.display()));
        }
        self.drop_gone(&gone);
        if !failed.is_empty() {
            let more = failed.len().saturating_sub(10);
            let mut detail = failed.iter().take(10).cloned().collect::<Vec<_>>().join("\n");
            if more > 0 {
                detail.push_str(&format!("\n… and {more} more"));
            }
            let d = gtk::AlertDialog::builder()
                .message(format!("{} image{} could not be moved to the Trash", failed.len(), if failed.len() == 1 { "" } else { "s" }))
                .detail(detail)
                .modal(true)
                .build();
            d.show(Some(&self.app.window));
        }
    }

    /// Take files that are no longer there off the page, as the Trash does:
    /// unmarked, out of their groups, and a group left with one file gone.
    /// What each rule suggested for the others stays what it was with them,
    /// except a deletion that rested on a file now gone: carried out, it
    /// would take the last copy of what it shows. The content rule's is
    /// REVIEW, and the group rules judge only by reference images still
    /// there.
    fn drop_gone(self: &Rc<Self>, gone: &HashSet<PathBuf>) {
        for p in gone {
            self.thumbs.forget(p);
            self.strip_thumbs.forget(p);
        }
        let mut unmarked = false;
        let select = {
            let mut s = self.state.borrow_mut();
            let ids: Vec<u32> = (0..s.n() as u32).filter(|&id| gone.contains(s.path(id as usize))).collect();
            for &id in &ids {
                if s.set_mark(id, false) {
                    unmarked = true;
                }
            }
            s.scan.lapse(ids.iter().map(|&id| id as usize), Lapse::Gone);
            s.suggest();
            for g in s.groups.iter_mut() {
                if g.files.iter().any(|f| f.is_representative() && gone.contains(&f.path)) {
                    g.reference_gone = true;
                }
                g.files.retain(|f| !gone.contains(&f.path));
            }
            let current = s.current;
            // Where the shown group lands once the emptied ones before it go.
            let before = s.groups[..current.min(s.groups.len())].iter().filter(|g| g.files.len() < 2).count();
            s.groups.retain(|g| g.files.len() >= 2);
            current.saturating_sub(before)
        };
        if unmarked {
            self.marks_changed();
        }
        self.thumbs.clear_queue();
        self.rebuild(select);
        self.focus();
    }
}

/// A style class on or off, touched only when it changes.
fn set_class(w: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if w.has_css_class(class) == on {
        return;
    }
    if on {
        w.add_css_class(class);
    } else {
        w.remove_css_class(class);
    }
}

/// The file that shows what a group is a picture of, for the strip: the
/// plain photograph rather than a collage, an embed or a page holding it. The
/// reference image can be any of those, since it is only the file that
/// matched the most others. What marks the plain one is its size: copies,
/// re-encodes and edits of a photograph keep its width and height, while
/// every canvas it was pasted into has a size of its own. So the size most
/// files share, the larger on a tie, and of the files that size one neither
/// inverted nor mirrored, the one the scan suggests keeping first.
fn face(files: &[Member]) -> Option<&Member> {
    let mut sizes: HashMap<(u32, u32), usize> = HashMap::new();
    for f in files {
        if let Some(d) = f.dims {
            *sizes.entry(d).or_default() += 1;
        }
    }
    let Some((&size, _)) = sizes.iter().max_by_key(|&(&(w, h), &n)| (n, w as u64 * h as u64)) else { return files.first() };
    let same = || files.iter().filter(move |f| f.dims == Some(size));
    // An inverted or mirrored copy is the same size and the wrong picture.
    let upright = |f: &&Member| f.with.is_none_or(|e| !e.inverted && !e.mirrored);
    same()
        .filter(upright)
        .find(|f| f.suggested == Some(Action::Keep))
        .or_else(|| same().find(upright))
        .or_else(|| same().next())
}

/// Group numbers as a sentence names them: "4", "4 and 9", "4, 9 and 12",
/// and past twenty, the first twenty and how many more.
fn numbers(ns: &[usize]) -> String {
    const SHOWN: usize = 20;
    let words: Vec<String> = ns.iter().take(SHOWN).map(|n| n.to_string()).collect();
    if ns.len() > SHOWN {
        return format!("{} and {} more", words.join(", "), ns.len() - SHOWN);
    }
    match words.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// What the bar says of a deletion's kept copy: its name, and its folder
/// when that is not the deleted file's own.
///
/// It said the name alone, and copies are what share names: a backup's
/// `IMG_0001.jpg` deleted for the library's read "Everything in it is also
/// in IMG_0001.jpg", which looks like the file vouching for itself.
fn held_by(file: &Path, kept: &Path) -> String {
    match kept.parent().filter(|&d| Some(d) != file.parent()) {
        Some(dir) => format!("Everything in it is also in {}, in {}", file_name(kept), dir.display()),
        None => format!("Everything in it is also in {}", file_name(kept)),
    }
}

/// A picture, and over it the part of it another picture shows (`Overlay`).
#[derive(Clone)]
struct Outline {
    overlay: gtk::Overlay,
    area: gtk::DrawingArea,
    shown: Rc<Cell<Option<Overlay>>>,
}

/// What is drawn over one picture of a pair: the other's frame, and the
/// reference's axes as they lie in this picture.
#[derive(Clone, Copy, Debug)]
struct Overlay {
    /// Where the other picture's frame lies in this one.
    frame: Place,
    /// Where one pixel step right (X) and down (Y) in the reference goes in
    /// this picture's pixels, `[xx, yx, xy, yy]`: X is `(xx, xy)`, Y is
    /// `(yx, yy)`. The reference's own are `IDENTITY`. `None` for a picture
    /// whose size the scan does not know.
    axes: Option<[f64; 4]>,
}

impl Overlay {
    const IDENTITY: [f64; 4] = [1.0, 0.0, 0.0, 1.0];
}

/// How far each arrow reaches: this share of the way from its origin to the
/// side of the frame it points at (of the frame's part on the picture), so
/// it grows and shrinks with the frame; and no shorter than `AXIS_LEAST`
/// screen pixels, so a small frame's arrows still show.
const AXIS_REACH: f64 = 0.5;
const AXIS_LEAST: f64 = 14.0;
const AXIS_X: (f64, f64, f64) = (0.95, 0.22, 0.2);
const AXIS_Y: (f64, f64, f64) = (0.2, 0.85, 0.3);

impl Outline {
    fn over(picture: &gtk::Picture) -> Outline {
        let area = gtk::DrawingArea::builder().can_target(false).hexpand(true).vexpand(true).build();
        let overlay = gtk::Overlay::builder().child(picture).vexpand(true).hexpand(true).build();
        overlay.add_overlay(&area);
        let shown: Rc<Cell<Option<Overlay>>> = Rc::default();
        area.set_draw_func({
            let (picture, shown) = (picture.downgrade(), shown.clone());
            move |_, cr, w, h| {
                let (Some(o), Some(picture)) = (shown.get(), picture.upgrade()) else { return };
                let Some(p) = picture.paintable() else { return };
                paint_overlay(cr, (w as f64, h as f64), (p.intrinsic_width() as f64, p.intrinsic_height() as f64), &o);
            }
        });
        picture.connect_paintable_notify({
            let area = area.downgrade();
            move |_| {
                if let Some(a) = area.upgrade() {
                    a.queue_draw();
                }
            }
        });
        Outline { overlay, area, shown }
    }

    fn set(&self, shown: Option<Overlay>) {
        self.shown.set(shown);
        self.area.queue_draw();
    }

    fn shows(&self) -> bool {
        self.shown.get().is_some()
    }
}

/// What is drawn over a member and over its reference, given where the
/// reference lies in the member and each one's size as shown: on each the
/// other's frame, and the reference's axes on both, its own on it and where
/// they go in the member.
fn overlays(place: &Place, reference: Option<(u32, u32)>, member: Option<(u32, u32)>) -> (Overlay, Option<Overlay>) {
    let axes = match (reference, member) {
        (Some(from), Some(to)) => Some(axes_in(place, from, to)),
        _ => None,
    };
    let on_reference = place.reversed().map(|frame| Overlay { frame, axes: reference.map(|_| Overlay::IDENTITY) });
    (Overlay { frame: *place, axes }, on_reference)
}

/// Where `Contain` puts a picture of `pw` x `ph` in a widget of `w` x `h`:
/// its left, top, width and height.
fn contained((w, h): (f64, f64), (pw, ph): (f64, f64)) -> (f64, f64, f64, f64) {
    let k = (w / pw).min(h / ph);
    let (dw, dh) = (pw * k, ph * k);
    ((w - dw) / 2.0, (h - dh) / 2.0, dw, dh)
}

/// The part of the other's frame on this picture, in fractions of it.
fn shared_part(frame: &Place) -> Vec<(f32, f32)> {
    clip_to_frame(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(x, y)| frame.apply(x, y)))
}

/// The arrows' origin, in fractions of this picture: the middle (centroid)
/// of the part of the other's frame on this picture. That is the frame's
/// middle when all of it is on the picture, and the picture's when the
/// frame holds all of it (a slide's frame, seen on the photo it shows).
fn axes_origin(frame: &Place) -> Option<(f32, f32)> {
    let p: Vec<(f64, f64)> = shared_part(frame).into_iter().map(|(x, y)| (x as f64, y as f64)).collect();
    let (mut area, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for (i, &a) in p.iter().enumerate() {
        let b = p[(i + 1) % p.len()];
        let c = a.0 * b.1 - b.0 * a.1;
        area += c;
        cx += (a.0 + b.0) * c;
        cy += (a.1 + b.1) * c;
    }
    (area.abs() > 1e-12).then(|| ((cx / (3.0 * area)) as f32, (cy / (3.0 * area)) as f32))
}

/// The two arrows, from `origin` inside the convex `frame`, all in screen
/// pixels: each one's tip, X (the reference's right) then Y (its up, towards
/// the top side of its frame). `None` for a map that flattens the picture.
fn axis_tips(origin: (f64, f64), axes: &[f64; 4], frame: &[(f64, f64)]) -> Option<[(f64, f64); 2]> {
    let [xx, yx, xy, yy] = *axes;
    let det = (xx * yy - yx * xy).abs();
    if !(det > 0.0) || !det.is_finite() {
        return None;
    }
    let tip = |(dx, dy): (f64, f64)| {
        let len = dx.hypot(dy);
        let u = (dx / len, dy / len);
        let reach = (AXIS_REACH * to_edge(frame, origin, u)).max(AXIS_LEAST);
        (origin.0 + reach * u.0, origin.1 + reach * u.1)
    };
    // The axes say where a step down goes; up is the other way.
    Some([tip((xx, xy)), tip((-yx, -yy))])
}

/// How far a ray from `origin` along the unit `u`, inside the convex
/// polygon `poly`, goes before it meets the polygon's edge.
fn to_edge(poly: &[(f64, f64)], origin: (f64, f64), u: (f64, f64)) -> f64 {
    let cross = |a: (f64, f64), b: (f64, f64)| a.0 * b.1 - a.1 * b.0;
    let mut nearest = f64::INFINITY;
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let (e, w) = ((b.0 - a.0, b.1 - a.1), (a.0 - origin.0, a.1 - origin.1));
        let d = cross(u, e);
        if d.abs() < 1e-12 {
            continue;
        }
        // origin + t u = a + s e
        let (t, s) = (cross(w, e) / d, cross(w, u) / d);
        if t > 1e-9 && (-1e-9..=1.0 + 1e-9).contains(&s) {
            nearest = nearest.min(t);
        }
    }
    if nearest.is_finite() { nearest } else { 0.0 }
}

/// Draw `o` over a picture of `pw` x `ph` shown `Contain` in `w` x `h`: the
/// other picture's frame outlined and the rest of this one dimmed, and the
/// reference's right (X) and up (Y) as arrows from the middle of the frame.
fn paint_overlay(cr: &gtk::cairo::Context, (w, h): (f64, f64), (pw, ph): (f64, f64), o: &Overlay) {
    if pw <= 0.0 || ph <= 0.0 {
        return;
    }
    let (ox, oy, dw, dh) = contained((w, h), (pw, ph));
    let on_screen = |(u, v): (f32, f32)| (ox + u as f64 * dw, oy + v as f64 * dh);
    let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(x, y)| on_screen(o.frame.apply(x, y)));
    let path = |cr: &gtk::cairo::Context| {
        cr.move_to(corners[0].0, corners[0].1);
        for c in &corners[1..] {
            cr.line_to(c.0, c.1);
        }
        cr.close_path();
    };
    let _ = cr.save();
    cr.rectangle(ox, oy, dw, dh);
    cr.clip();
    cr.set_fill_rule(gtk::cairo::FillRule::EvenOdd);
    cr.rectangle(ox, oy, dw, dh);
    path(cr);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    let _ = cr.fill();
    // Dark under the colour, so a line shows on any picture.
    let stroke = |cr: &gtk::cairo::Context, (r, g, b): (f64, f64, f64), width: f64| {
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.8);
        cr.set_line_width(width + 2.5);
        let _ = cr.stroke_preserve();
        cr.set_source_rgb(r, g, b);
        cr.set_line_width(width);
        let _ = cr.stroke();
    };
    path(cr);
    stroke(cr, (1.0, 0.85, 0.2), 2.0);
    let _ = cr.restore();
    let (Some(axes), Some(origin)) = (o.axes, axes_origin(&o.frame)) else { return };
    let origin = on_screen(origin);
    let shared: Vec<(f64, f64)> = shared_part(&o.frame).into_iter().map(on_screen).collect();
    let Some(tips) = axis_tips(origin, &axes, &shared) else { return };
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    let arrows = [(tips[0], AXIS_X, "X"), (tips[1], AXIS_Y, "Y")].map(|(tip, colour, name)| {
        let (dx, dy) = (tip.0 - origin.0, tip.1 - origin.1);
        let len = dx.hypot(dy);
        // Thinner on a short arrow, so a small frame's do not crowd it.
        (tip, colour, name, len, (dx / len, dy / len), (len / 30.0).clamp(1.5, 2.5))
    });
    let shaft = |cr: &gtk::cairo::Context, tip: (f64, f64), len: f64, (ux, uy): (f64, f64)| {
        // The head: two strokes back from the tip, a quarter of the arrow
        // long (within reason), either side of it.
        let head = (len * 0.28).clamp(6.0, 14.0);
        let (c, s) = (0.42f64.cos(), 0.42f64.sin());
        cr.move_to(origin.0, origin.1);
        cr.line_to(tip.0, tip.1);
        cr.move_to(tip.0 - head * (ux * c - uy * s), tip.1 - head * (uy * c + ux * s));
        cr.line_to(tip.0, tip.1);
        cr.line_to(tip.0 - head * (ux * c + uy * s), tip.1 - head * (uy * c - ux * s));
    };
    // Both dark edges under both arrows, so neither hides where the other
    // starts.
    for &(tip, _, _, len, u, width) in &arrows {
        shaft(cr, tip, len, u);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.8);
        cr.set_line_width(width + 2.5);
        let _ = cr.stroke();
    }
    for &(tip, (r, g, b), _, len, u, width) in &arrows {
        shaft(cr, tip, len, u);
        cr.set_source_rgb(r, g, b);
        cr.set_line_width(width);
        let _ = cr.stroke();
    }
    // Each axis's name, upright, past its tip: the colours alone do not tell
    // them apart for everyone.
    cr.select_font_face("Sans", gtk::cairo::FontSlant::Normal, gtk::cairo::FontWeight::Bold);
    cr.set_font_size(14.0);
    for &(tip, colour, name, _, (ux, uy), _) in &arrows {
        if let Ok(e) = cr.text_extents(name) {
            let (lx, ly) = (tip.0 + ux * 12.0, tip.1 + uy * 12.0);
            cr.move_to(lx - e.width() / 2.0 - e.x_bearing(), ly - e.height() / 2.0 - e.y_bearing());
            cr.text_path(name);
            stroke(cr, colour, 1.0);
        }
    }
}

/// Where one pixel step right and down in a picture of size `from` goes in
/// one of size `to` (both as shown), by `place`: `[xx, yx, xy, yy]`, as
/// `Overlay::axes`.
fn axes_in(place: &Place, (fw, fh): (u32, u32), (tw, th): (u32, u32)) -> [f64; 4] {
    let m = place.0.map(f64::from);
    let (fw, fh, tw, th) = (fw as f64, fh as f64, tw as f64, th as f64);
    [m[0] * tw / fw, m[1] * tw / fh, m[3] * th / fw, m[4] * th / fh]
}

/// The part of a quadrilateral, in fractions of a picture, that lies on the
/// picture: clipped to the unit square edge by edge (Sutherland-Hodgman).
fn clip_to_frame(poly: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = poly.to_vec();
    // Each edge of the square: which side is inside, and where a segment
    // crosses it.
    let edges: [(fn((f32, f32)) -> f32, usize, f32); 4] = [(|p| p.0, 0, 0.0), (|p| -p.0, 0, 1.0), (|p| p.1, 1, 0.0), (|p| -p.1, 1, 1.0)];
    for (side, axis, at) in edges {
        let inside = |p: (f32, f32)| side(p) >= side(if axis == 0 { (at, p.1) } else { (p.0, at) });
        let input = std::mem::take(&mut out);
        for (i, &p) in input.iter().enumerate() {
            let q = input[(i + 1) % input.len()];
            let cross = |a: (f32, f32), b: (f32, f32)| {
                let (ca, cb) = if axis == 0 { (a.0, b.0) } else { (a.1, b.1) };
                let t = (at - ca) / (cb - ca);
                (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
            };
            match (inside(p), inside(q)) {
                (true, true) => out.push(q),
                (true, false) => out.push(cross(p, q)),
                (false, true) => {
                    out.push(cross(p, q));
                    out.push(q);
                }
                (false, false) => {}
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// How a picture of size `from` lies turned or flipped in one of size `to`
/// (both as shown), by `place`: "" when it is the same way up.
fn turned(place: &Place, from: (u32, u32), to: (u32, u32)) -> String {
    // In pixels: how one pixel of `from` moves across `to`.
    let j = axes_in(place, from, to);
    let mirrored = j[0] * j[3] - j[1] * j[2] < 0.0;
    // A mirror first, left to right, then a turn: y points down, so a
    // positive angle is clockwise.
    let (c, s) = if mirrored { (-j[0], -j[2]) } else { (j[0], j[2]) };
    let deg = s.atan2(c).to_degrees();
    // Whole degrees, and a quarter turn as one: the fit puts an exact copy's
    // corners within 0.04% of the shared area's diagonal on IMGS-ALL's
    // ground truth (median), a twentieth of a degree, so a turn of one
    // degree is a turn.
    let quarter = (deg / 90.0).round() * 90.0;
    let deg = if (deg - quarter).abs() < 0.5 { quarter } else { deg.round() };
    let turn = match deg as i64 {
        0 => String::new(),
        180 | -180 => "turned 180°".to_string(),
        d if d > 0 => format!("turned {d}° clockwise"),
        d => format!("turned {}° anticlockwise", -d),
    };
    match (mirrored, turn.as_str()) {
        (false, t) => t.to_string(),
        (true, "") => "mirrored".to_string(),
        (true, "turned 180°") => "upside down".to_string(),
        (true, t) => format!("mirrored, {t}"),
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

fn size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_073_741_824.0 {
        format!("{:.1} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{bytes} bytes")
    }
}

/// What a member's match with the reference image rests on: how much of one
/// lies inside the other, and how closely their pixels agree there.
fn evidence(m: &Member) -> String {
    let Some(e) = m.with else {
        return "reference image".into();
    };
    match e.relation {
        Relation::Identical => return "identical copy of the reference".into(),
        Relation::SamePixels => return "same picture as the reference, saved differently".into(),
        _ => {}
    }
    // A corroborated match cleared only the lower bar a pair inside an
    // existing group faces, so its correlation can sit below the one the scan
    // asked for; the text report says so, and so does the card.
    let mut s = if e.relation == Relation::Corroborated { "corroborated · ".to_string() } else { String::new() };
    s.push_str(&format!("overlap {:.2} · correlation {:.2}", e.frame_overlap, e.pixel_correlation));
    if e.mirrored {
        s.push_str(" · mirrored");
    }
    if e.inverted {
        s.push_str(" · inverted");
    }
    s
}

/// `menu`, as `view` has it: the header's button and the right click share
/// it, so both change with it. Each item's letter is also Alt and that letter
/// anywhere on the page (`Results::key`), which it shows beside it.
fn fill_menu(menu: &gio::Menu, view: View) {
    menu.remove_all();
    for part in MENU {
        let section = gio::Menu::new();
        for (act, label) in part.iter().filter_map(|&a| a.label(view).map(|l| (a, l))) {
            let item = gio::MenuItem::new(Some(label), Some(&format!("results.{}", act.action())));
            item.set_attribute_value("accel", Some(&format!("<Alt>{}", l::letter(label)).to_variant()));
            section.append_item(&item);
        }
        menu.append_section(None, &section);
    }
}

/// The rules the picker offers, in its order.
const RULES: &[&str] = &["Keep all content", "By correlation", "Reference images only"];
const MODES: [SuggestMode; 3] = [SuggestMode::Content, SuggestMode::Correlation, SuggestMode::Representative];

/// A scan made ready for the page, off the main thread: its groups as the
/// page shows them, and what each rule suggests for each file.
pub struct Prepared {
    groups: Vec<GroupState>,
    scan: Outcome,
    by_mode: Vec<Vec<Option<Action>>>,
    kept: Option<u64>,
    roots: Vec<PathBuf>,
    changed_after: Option<u64>,
}

impl Prepared {
    /// The folders the scan was given.
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
    }

    pub fn scan(&self) -> &Outcome {
        &self.scan
    }

    pub fn groups(&self) -> usize {
        self.groups.len()
    }

    pub fn analysed(&self) -> usize {
        self.scan.head.files_analysed
    }

    /// A file changed after `ns` (nanoseconds since 1970) is not the file the scan
    /// judged (`last::Notes::changed_after`).
    pub fn set_changed_after(&mut self, ns: u64) {
        self.changed_after = Some(ns);
    }

    /// It is the scan kept on disk as `id` (`last::Notes::id`).
    pub fn set_kept(&mut self, id: u64) {
        self.kept = Some(id);
    }
}

pub fn prepare(scan: Outcome) -> Prepared {
    let suggested = scan.actions(SuggestMode::Content);
    let groups = scan
        .groups
        .iter()
        .map(|g| {
            let files = g
                .members
                .iter()
                .map(|m| {
                    let f = &scan.files[m.file];
                    Member { id: m.file as u32, path: f.path.clone(), dims: f.dims, bytes: f.bytes, damaged: f.damaged, with: m.with, at: m.at, suggested: suggested[m.file] }
                })
                .collect();
            GroupState { files, reference_gone: false }
        })
        .collect();
    let by_mode = MODES.iter().map(|&m| scan.actions(m)).collect();
    Prepared { groups, scan, by_mode, kept: None, roots: Vec::new(), changed_after: None }
}

/// What a REVIEW is, for the yellow corner: either of the two reasons the
/// content rule leaves a file to a person (`suggest.rs`), a match too weak to
/// delete on, or one of two versions of one picture where nothing the scan
/// measured says which is the original. The second is most of them, and it
/// was labelled "weak match" beside a correlation of 1.00.
const REVIEW_TIP: &str = "Review: a close copy that could not be confirmed, or one of two versions of the same picture where nothing says which is the original";

/// What is suggested for a file, in a few words.
fn suggestion(action: Option<Action>) -> &'static str {
    match action {
        Some(Action::Keep) => "suggested: keep",
        Some(Action::Delete) => "suggested: delete",
        Some(Action::Review) => "suggested: review",
        None => "",
    }
}

/// Dimensions and size, as far as the report knows them.
fn facts(m: &Member) -> String {
    let mut parts = Vec::new();
    if let Some((w, h)) = m.dims {
        parts.push(format!("{w} × {h}"));
    }
    if let Some(b) = m.bytes {
        parts.push(size(b));
    }
    // The one copy not to keep, and nothing else on the card says so.
    if m.damaged {
        parts.push("damaged file".into());
    }
    parts.join(" · ")
}


#[cfg(test)]
mod tests {
    use super::*;

    /// What `paint_overlay` drew, read back from its pixels.
    struct Drawn {
        w: usize,
        h: usize,
        px: Vec<[u8; 3]>,
    }

    impl Drawn {
        /// `o` over a picture of `pic` pixels, shown in a `w` x `h` widget,
        /// on mid grey.
        fn paint(w: i32, h: i32, pic: (u32, u32), o: &Overlay) -> Drawn {
            let mut surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::Rgb24, w, h).unwrap();
            {
                let cr = gtk::cairo::Context::new(&surface).unwrap();
                cr.set_source_rgb(0.5, 0.5, 0.5);
                cr.paint().unwrap();
                paint_overlay(&cr, (w as f64, h as f64), (pic.0 as f64, pic.1 as f64), o);
            }
            surface.flush();
            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            let px = (0..h as usize).flat_map(|y| (0..w as usize).map(move |x| (x, y))).map(|(x, y)| {
                let i = y * stride + x * 4;
                [data[i + 2], data[i + 1], data[i]]
            });
            Drawn { w: w as usize, h: h as usize, px: px.collect() }
        }

        /// Whether a pixel is of `colour`: an arrow's by its one strong
        /// channel (a thin line blends with its dark edge), anything else
        /// near the colour itself.
        fn is(&self, x: usize, y: usize, colour: (f64, f64, f64)) -> bool {
            let p = self.px[y * self.w + x].map(i32::from);
            let strong = |k: usize| (0..3).all(|j| j == k || p[k] > p[j] + 60);
            if colour == AXIS_X {
                return strong(0);
            }
            if colour == AXIS_Y {
                return strong(1);
            }
            let want = [colour.0, colour.1, colour.2].map(|c| (c * 255.0) as i32);
            (0..3).all(|k| (p[k] - want[k]).abs() < 40)
        }

        fn of(&self, colour: (f64, f64, f64)) -> Vec<(f64, f64)> {
            (0..self.h).flat_map(|y| (0..self.w).map(move |x| (x, y))).filter(|&(x, y)| self.is(x, y, colour)).map(|(x, y)| (x as f64 + 0.5, y as f64 + 0.5)).collect()
        }

        /// The direction an arrow of `colour` points from `origin`, in
        /// degrees clockwise from the screen's right, and how long it is:
        /// the centroid of its pixels clear of the other arrow at the
        /// origin and short of its name past the tip (the head is
        /// symmetric about the shaft), then how far along that its colour
        /// runs unbroken.
        fn arrow(&self, origin: (f64, f64), colour: (f64, f64, f64)) -> (f64, f64) {
            let along = |a: f64, r: f64| ((origin.0 + r * a.cos()).floor() as usize, (origin.1 + r * a.sin()).floor() as usize);
            let rough: Vec<(f64, f64)> = self.of(colour).into_iter().filter(|p| (4.0..12.0).contains(&(p.0 - origin.0).hypot(p.1 - origin.1))).collect();
            assert!(rough.len() > 5, "an arrow is drawn: {} pixels", rough.len());
            let n = rough.len() as f64;
            let a = (rough.iter().map(|p| p.1).sum::<f64>() / n - origin.1).atan2(rough.iter().map(|p| p.0).sum::<f64>() / n - origin.0);
            let mut len = 0.0;
            let mut r = 4.0;
            while r < len + 2.5 || len == 0.0 {
                let (x, y) = along(a, r);
                if x >= self.w || y >= self.h {
                    break;
                }
                // A pixel either side: the rough direction drifts off a
                // thin line along a long arrow.
                let (nx, ny) = (-a.sin(), a.cos());
                let near = [-1.0, 0.0, 1.0].iter().any(|&k: &f64| {
                    let (x, y) = ((x as f64 + 0.5 + k * nx).floor(), (y as f64 + 0.5 + k * ny).floor());
                    x >= 0.0 && y >= 0.0 && (x as usize) < self.w && (y as usize) < self.h && self.is(x as usize, y as usize, colour)
                });
                if near {
                    len = r;
                }
                r += 0.5;
            }
            let shaft: Vec<(f64, f64)> = self.of(colour).into_iter().filter(|p| (4.0..len - 1.0).contains(&(p.0 - origin.0).hypot(p.1 - origin.1))).collect();
            let n = shaft.len() as f64;
            let (cx, cy) = (shaft.iter().map(|p| p.0).sum::<f64>() / n, shaft.iter().map(|p| p.1).sum::<f64>() / n);
            ((cy - origin.1).atan2(cx - origin.0).to_degrees(), len)
        }

        /// How near to `at` the arrow of `colour` comes: where it starts.
        fn nearest(&self, at: (f64, f64), colour: (f64, f64, f64)) -> f64 {
            self.of(colour).iter().map(|p| (p.0 - at.0).hypot(p.1 - at.1)).fold(f64::MAX, f64::min)
        }
    }

    fn angle_between(a: f64, b: f64) -> f64 {
        let d = (a - b).rem_euclid(360.0);
        d.min(360.0 - d)
    }

    /// The reference's X and Y arrows, over the member and over the
    /// reference, for every way one picture can lie in another: drawn
    /// through `overlays` and `paint_overlay` as the window draws them, and
    /// read back from the pixels. The expected directions are what each
    /// edit does to the picture, not the formula: a quarter turn
    /// anticlockwise takes the reference's top edge to the copy's left, so
    /// its right (X) points up there and its up (Y) left, to the frame's
    /// top side. Each arrow reaches half way to the side of the frame (as
    /// far as it is on the picture) it points at: at twice its length is the
    /// outline or the picture's edge.
    #[test]
    fn the_axes_are_drawn_where_and_as_the_transform_puts_them() {
        const RIGHT: f64 = 0.0;
        const DOWN: f64 = 90.0;
        const LEFT: f64 = 180.0;
        const UP: f64 = -90.0;
        let r = (400, 300);
        // Name, where the reference lies in the member (fractions), the
        // member's size, and X and Y as they should point in the member.
        let cases: &[(&str, [f32; 6], (u32, u32), f64, f64)] = &[
            ("same", [1.0, 0.0, 0.0, 0.0, 1.0, 0.0], (400, 300), RIGHT, UP),
            ("half the size", [1.0, 0.0, 0.0, 0.0, 1.0, 0.0], (200, 150), RIGHT, UP),
            ("mirrored", [-1.0, 0.0, 1.0, 0.0, 1.0, 0.0], (400, 300), LEFT, UP),
            ("upside down", [1.0, 0.0, 0.0, 0.0, -1.0, 1.0], (400, 300), RIGHT, DOWN),
            ("turned 180", [-1.0, 0.0, 1.0, 0.0, -1.0, 1.0], (400, 300), LEFT, DOWN),
            ("quarter anticlockwise", [0.0, 1.0, 0.0, -1.0, 0.0, 1.0], (300, 400), UP, LEFT),
            ("quarter clockwise", [0.0, -1.0, 1.0, 1.0, 0.0, 0.0], (300, 400), DOWN, RIGHT),
            ("transposed", [0.0, 1.0, 0.0, 1.0, 0.0, 0.0], (300, 400), DOWN, LEFT),
            ("anti-transposed", [0.0, -1.0, 1.0, -1.0, 0.0, 1.0], (300, 400), UP, RIGHT),
            ("centre crop", [2.0, 0.0, -0.5, 0.0, 2.0, -0.5], (200, 150), RIGHT, UP),
            ("corner crop", [3.0, 0.0, 0.0, 0.0, 3.0, 0.0], (133, 100), RIGHT, UP),
            ("squeezed", [1.0, 0.0, 0.0, 0.0, 1.0, 0.0], (200, 300), RIGHT, UP),
            ("mirrored crop", [-2.0, 0.0, 1.5, 0.0, 2.0, -0.5], (200, 150), LEFT, UP),
            ("embedded, turned", [0.0, 0.25, 0.4, -0.25, 0.0, 0.6], (1200, 1600), UP, LEFT),
            ("embedded off centre", [0.3, 0.0, 0.1, 0.0, 0.3, 0.6], (400, 300), RIGHT, UP),
            // A photo in a slide, off its middle: the slide's middle lies on
            // the photo, but the arrows there start at the photo's.
            ("in a slide", [0.4, 0.0, 0.4, 0.0, 0.35, 0.4], (1728, 972), RIGHT, UP),
            ("embedded tiny", [0.04, 0.0, 0.48, 0.0, 0.04, 0.48], (4000, 3000), RIGHT, UP),
        ];
        let mut cases: Vec<(String, [f32; 6], (u32, u32), f64, f64)> = cases.iter().map(|c| (c.0.to_string(), c.1, c.2, c.3, c.4)).collect();
        // Turned by small amounts either way, in pixels: a turn of t
        // clockwise moves X t below the right and Y t past the down.
        for t in [-30.0f64, -10.0, -2.0, 4.0, 15.0, 45.0] {
            let (c, s) = (t.to_radians().cos() as f32, t.to_radians().sin() as f32);
            // About the middle, in the reference's pixels, put back in
            // fractions of a picture of the same size.
            let (w, h) = (400.0f32, 300.0f32);
            let m = [c, -s * h / w, 0.0, s * w / h, c, 0.0];
            let (cx, cy) = (m[0] * 0.5 + m[1] * 0.5, m[3] * 0.5 + m[4] * 0.5);
            let m = [m[0], m[1], 0.5 - cx, m[3], m[4], 0.5 - cy];
            cases.push((format!("turned {t}"), m, (400, 300), t, t - 90.0));
        }
        for (name, m, member, x, y) in &cases {
            let place = Place(*m);
            let (on_member, on_reference) = overlays(&place, Some(r), Some(*member));
            for (side, o, pic, x, y) in [("member", on_member, *member, *x, *y), ("reference", on_reference.unwrap(), r, RIGHT, UP)] {
                let (w, h) = (360, 300);
                let d = Drawn::paint(w, h, pic, &o);
                let (ox, oy, dw, dh) = contained((w as f64, h as f64), (pic.0 as f64, pic.1 as f64));
                let origin = axes_origin(&o.frame).unwrap();
                let origin = (ox + origin.0 as f64 * dw, oy + origin.1 as f64 * dh);
                // Where it starts: the frame's middle when it is all on the
                // picture, the picture's when it holds all of the picture.
                let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].map(|(u, v)| o.frame.apply(u, v));
                let on_picture = |(u, v): (f32, f32)| (-1e-3..=1.0 + 1e-3).contains(&u) && (-1e-3..=1.0 + 1e-3).contains(&v);
                let held = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].iter().all(|&(u, v)| {
                    // A corner of the picture inside the frame: on the same
                    // side of each of its edges.
                    let side = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (v - a.1) - (b.1 - a.1) * (u - a.0);
                    let s: Vec<f32> = (0..4).map(|i| side(corners[i], corners[(i + 1) % 4])).collect();
                    s.iter().all(|&x| x >= -1e-4) || s.iter().all(|&x| x <= 1e-4)
                });
                let want = if corners.iter().all(|&c| on_picture(c)) {
                    Some(o.frame.apply(0.5, 0.5))
                } else if held {
                    Some((0.5, 0.5))
                } else {
                    None
                };
                if let Some((fx, fy)) = want {
                    let mid = (ox + fx as f64 * dw, oy + fy as f64 * dh);
                    assert!((origin.0 - mid.0).hypot(origin.1 - mid.1) < 0.5, "{name}, {side}: starts at {origin:?}, not {mid:?}");
                }
                for colour in [AXIS_X, AXIS_Y] {
                    assert!(d.nearest(origin, colour) < 4.0, "{name}, {side}: an arrow starts at {origin:?}");
                }
                let on_screen = |(u, v): (f32, f32)| (ox + u as f64 * dw, oy + v as f64 * dh);
                let yellow = (1.0, 0.85, 0.2);
                let outline = d.of(yellow);
                // Where the frame is all on the picture, the middle of its
                // right and top sides, seen from the reference's side.
                let whole = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].iter().all(|&(u, v)| {
                    let (u, v) = o.frame.apply(u, v);
                    (-1e-3..=1.0 + 1e-3).contains(&u) && (-1e-3..=1.0 + 1e-3).contains(&v)
                });
                for (colour, want, label, side_mid) in [(AXIS_X, x, "X", (1.0, 0.5)), (AXIS_Y, y, "Y", (0.5, 0.0))] {
                    let (got, len) = d.arrow(origin, colour);
                    assert!(angle_between(got, want) < 3.0, "{name}, {side}: {label} points {got:.1}, not {want}");
                    let a = got.to_radians();
                    if len < AXIS_LEAST + 3.0 {
                        // A small frame's arrow is drawn at the least length.
                        assert!((len - AXIS_LEAST).abs() < 3.0, "{name}, {side}: {label} is {len:.1} long, not the least");
                        continue;
                    }
                    let far = (origin.0 + 2.0 * len * a.cos(), origin.1 + 2.0 * len * a.sin());
                    let on_edge = (far.0 - ox).abs().min((far.0 - ox - dw).abs()).min((far.1 - oy).abs()).min((far.1 - oy - dh).abs()) < 5.0;
                    let on_outline = outline.iter().any(|p| (p.0 - far.0).hypot(p.1 - far.1) < 5.0);
                    // The name, 12 px past the tip, covers the outline at
                    // twice a short arrow's length.
                    assert!(len < 24.0 || on_edge || on_outline, "{name}, {side}: {label} ({len:.1} long) is not half way to the frame's side at {far:?}");
                    if side == "member" && whole {
                        // The reference's own sides: X half way to its right
                        // side's middle, Y to its top's.
                        let mid = on_screen(o.frame.apply(0.5, 0.5));
                        let end = on_screen(o.frame.apply(side_mid.0, side_mid.1));
                        let want = (end.0 - mid.0).hypot(end.1 - mid.1) / 2.0;
                        assert!((len - want).abs() < 3.0, "{name}, {side}: {label} is {len:.1} long, not {want:.1}");
                    }
                }
            }
        }
    }

    /// The outline is the other picture's frame: its corners where the
    /// transform puts them.
    #[test]
    fn the_outline_is_the_other_frame() {
        // A quarter-size crop at the reference's top left, seen over the
        // reference: its frame is the reference's top-left quarter.
        let o = Overlay { frame: Place([0.5, 0.0, 0.0, 0.0, 0.5, 0.0]), axes: None };
        let d = Drawn::paint(400, 300, (400, 300), &o);
        let yellow = (1.0, 0.85, 0.2);
        let lit = d.of(yellow);
        let near = |x: f64, y: f64| lit.iter().any(|p| (p.0 - x).hypot(p.1 - y) < 2.5);
        for (x, y) in [(0.0f64, 0.0f64), (200.0, 0.0), (200.0, 150.0), (0.0, 150.0), (100.0, 150.0), (200.0, 75.0)] {
            assert!(near(x.max(1.0), y.max(1.0)), "the outline passes ({x}, {y})");
        }
        assert!(!near(300.0, 150.0) && !near(200.0, 250.0), "and nowhere else");
        // Outside it, dimmed; inside, the picture as it is.
        let grey = |x: usize, y: usize| d.px[y * d.w + x][0] as i32;
        assert!(grey(300, 200) < 80 && (grey(100, 75) - 128).abs() < 3);
    }

    /// A 400 x 300 reference, and copies of it in each orientation: the
    /// words say how the copy lies, whatever its size.
    #[test]
    fn a_turn_or_flip_is_said_in_words() {
        let at = |m: [f32; 6]| Place(m);
        let r = (400, 300);
        assert_eq!(turned(&at([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]), r, (200, 150)), "");
        assert_eq!(turned(&at([-1.0, 0.0, 1.0, 0.0, 1.0, 0.0]), r, r), "mirrored");
        assert_eq!(turned(&at([1.0, 0.0, 0.0, 0.0, -1.0, 1.0]), r, r), "upside down");
        assert_eq!(turned(&at([-1.0, 0.0, 1.0, 0.0, -1.0, 1.0]), r, r), "turned 180°");
        // A quarter turn clockwise: the reference's top edge is the copy's
        // right edge, top to bottom; the copy is 300 x 400.
        assert_eq!(turned(&at([0.0, -1.0, 1.0, 1.0, 0.0, 0.0]), r, (300, 400)), "turned 90° clockwise");
        assert_eq!(turned(&at([0.0, 1.0, 0.0, -1.0, 0.0, 1.0]), r, (300, 400)), "turned 90° anticlockwise");
        // A quarter turn and a mirror: the reference's top edge on the
        // copy's left.
        assert_eq!(turned(&at([0.0, 1.0, 0.0, 1.0, 0.0, 0.0]), r, (300, 400)), "mirrored, turned 90° anticlockwise");
        // A small turn is said in degrees; the fit's noise near none is not.
        let (c, s) = (10f32.to_radians().cos(), 10f32.to_radians().sin());
        assert_eq!(turned(&at([c, -s * 0.75, 0.0, s / 0.75, c, 0.0]), r, r), "turned 10° clockwise");
        let (c, s) = (2f32.to_radians().cos(), 2f32.to_radians().sin());
        assert_eq!(turned(&at([c, -s * 0.75, 0.0, s / 0.75, c, 0.0]), r, r), "turned 2° clockwise");
        let (c, s) = (0.2f32.to_radians().cos(), 0.2f32.to_radians().sin());
        assert_eq!(turned(&at([c, -s * 0.75, 0.0, s / 0.75, c, 0.0]), r, r), "");
    }

    #[test]
    fn the_shared_part_is_clipped_to_the_picture() {
        let area = |p: &[(f32, f32)]| (0..p.len()).map(|i| p[i].0 * p[(i + 1) % p.len()].1 - p[(i + 1) % p.len()].0 * p[i].1).sum::<f32>().abs() / 2.0;
        let square = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        assert_eq!(area(&clip_to_frame(&square)), 1.0);
        // A reference four times a crop's size, around it: all of the crop.
        assert_eq!(area(&clip_to_frame(&[(-1.0, -1.0), (3.0, -1.0), (3.0, 3.0), (-1.0, 3.0)])), 1.0);
        // Half off the right edge.
        assert_eq!(area(&clip_to_frame(&[(0.5, 0.0), (1.5, 0.0), (1.5, 1.0), (0.5, 1.0)])), 0.5);
        assert!(clip_to_frame(&[(2.0, 2.0), (3.0, 2.0), (3.0, 3.0), (2.0, 3.0)]).is_empty());
    }

    /// 0 the reference and kept original, 1 and 2 copies; the content rule
    /// says `content`, and deletes each DELETE for 0.
    fn state(content: [Option<Action>; 3]) -> State {
        use img_fp::outcome::{File, Group, Head, Member};
        let ev = Evidence { relation: Relation::Direct, aligned_points: Some(40), frame_overlap: 1.0, pixel_correlation: 0.99, mirrored: false, inverted: false };
        let files = (0..3).map(|i| File { path: PathBuf::from(format!("/p/{i}.jpg")), dims: Some((8, 8)), bytes: Some(1), damaged: false, library: false }).collect();
        let members = (0..3).map(|i| Member { file: i, with: (i > 0).then_some(ev), at: None }).collect();
        let kept = content.iter().map(|a| (*a == Some(Action::Delete)).then_some(0)).collect();
        let p = prepare(Outcome::new(Head::default(), 0.7, files, vec![Group { members }], content.to_vec(), kept));
        State { groups: p.groups, scan: p.scan, by_mode: p.by_mode, marked: vec![false; 3], ..Default::default() }
    }

    /// The Trash warns of a marked file whose kept copy is marked too,
    /// whichever rule is shown and whether or not a group is all marked.
    #[test]
    fn a_marked_deletion_whose_kept_copy_is_marked_is_counted() {
        // 1 a re-encode 0 holds, 2 a crop of it.
        let mut s = state([Some(Action::Keep), Some(Action::Delete), Some(Action::Keep)]);
        s.mode = 1;
        s.set_mark(1, true);
        assert_eq!(s.marked_with_their_copy(), 0, "the original stays");
        s.set_mark(0, true);
        assert_eq!(s.marked_with_their_copy(), 1, "the original goes with it; the crop does not hold it");
    }

    /// A kept copy changed since the scan holds nothing the window can vouch
    /// for: what the content rule deleted on its strength is left for a look,
    /// the group rules do not judge by it, and it is not deleted itself.
    #[test]
    fn a_kept_copy_changed_since_the_scan_is_not_relied_on() {
        let d = Some(Action::Delete);
        let mut s = state([Some(Action::Keep), d, d]);
        assert!(!s.note_changed(&[]), "nothing changed, nothing moves");
        assert!(s.note_changed(&[0]));
        assert_eq!(s.by_mode[0], vec![Some(Action::Keep), Some(Action::Review), Some(Action::Review)]);
        assert_eq!((s.scan.kept_copy(1), s.unheld_of(1)), (None, Some(Lapse::Changed)));
        s.mode = 0;
        assert_eq!(s.said(1), "review: its kept copy has changed");
        assert!(!s.to_look_at(1), "not one the scan left to look at");
        for mode in 1..3 {
            assert_eq!(s.by_mode[mode], vec![None; 3], "{:?}: judged by a reference that changed", MODES[mode]);
        }

        // A copy itself changed is not deleted by any rule, and says why.
        let mut t = state([Some(Action::Keep), d, d]);
        t.note_changed(&[1]);
        for mode in 0..3 {
            assert_eq!(t.by_mode[mode][1], Some(Action::Review), "{:?}", MODES[mode]);
            assert_eq!(t.by_mode[mode][2], d, "{:?}: an unchanged copy is as it was", MODES[mode]);
        }
        assert_eq!(t.said(1), "review: changed since the scan");
    }

    /// A kept copy in another folder is named with its folder: copies share
    /// names, and the name alone read as the file vouching for itself.
    #[test]
    fn a_kept_copy_elsewhere_is_named_with_its_folder() {
        let (a, b, c) = (Path::new("/backup/IMG_0001.jpg"), Path::new("/photos/IMG_0001.jpg"), Path::new("/backup/IMG_0001 (1).jpg"));
        assert_eq!(held_by(a, b), "Everything in it is also in IMG_0001.jpg, in /photos");
        assert_eq!(held_by(c, a), "Everything in it is also in IMG_0001.jpg");
    }

    #[test]
    fn group_numbers_read_as_a_sentence() {
        assert_eq!(numbers(&[4]), "4");
        assert_eq!(numbers(&[4, 9]), "4 and 9");
        assert_eq!(numbers(&[4, 9, 12]), "4, 9 and 12");
        let many: Vec<usize> = (1..=23).collect();
        assert!(numbers(&many).ends_with("19, 20 and 3 more"));
    }
}
