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
//! to see at a glance: which one is the reference, which are weak matches, and
//! which are marked. Pointing at an image, or moving to it with the keyboard,
//! puts its details in the bar at the bottom of the page, so the picture being
//! judged is never covered.
//!
//! **The scan's suggestion is shown and offered, never applied.** The bar says
//! what img-fp suggests for the image (keep, delete, or "weak match"), and a
//! weak match has a yellow corner. *Mark suggested deletions* makes the marks
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
//! click) offers the rest, Ctrl+Page Down / Ctrl+Page Up change group, and
//! every button has an Alt mnemonic.

use crate::folders::{self, Tree};
use crate::labels as l;
use crate::last;
use crate::mosaic::Mosaic;
use crate::scan::{Found, Group, Member};
use crate::still::Still;
use crate::thumbs::{show_on, Key, Latest, Shown, Size, Thumbs};
use crate::App;
use img_fp::{Action, SuggestMode};
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
overlay.tile .weak { min-width: 20px; min-height: 20px; background-image: linear-gradient(to bottom left, @warning_color 50%, transparent 50%); }
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
    groups: Vec<GroupState>,
    /// Each file's path, by number.
    paths: Vec<PathBuf>,
    marked: Vec<bool>,
    /// How many are marked, and their bytes.
    marked_n: usize,
    marked_bytes: u64,
    /// For each file, the groups it is in.
    member_of: Vec<Vec<usize>>,
    sizes: Vec<u64>,
    /// Gone to the Trash.
    gone: Vec<bool>,
    /// What each rule suggests for each file, in `MODES`' order, worked out
    /// when the results arrive: once a reference image is in the Trash, its
    /// group no longer says what was kept for its files.
    by_mode: Vec<Vec<Option<Action>>>,
    /// The rule chosen: an index into `MODES`.
    mode: usize,
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
    /// What the chosen rule suggests for file `id`.
    fn action(&self, id: u32) -> Option<Action> {
        self.by_mode.get(self.mode).and_then(|m| m.get(id as usize).copied().flatten())
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
        let b = self.sizes[i];
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
    weak: gtk::Box,
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
    list: gtk::ListView,
    selection: gtk::SingleSelection,
    store: gio::ListStore,
    /// The strip's rows that exist, by the list item each belongs to.
    strip: RefCell<HashMap<gtk::ListItem, StripRow>>,
    /// Each group's picture in the strip (`face`).
    faces: RefCell<Vec<PathBuf>>,
    /// Set while the strip's groups are being replaced, so that the
    /// selection moving on the way does not show each group it passes.
    quiet: Cell<bool>,
    /// The folder tree, the strip's alternative (`tree_view`).
    tree: Rc<RefCell<Tree>>,
    tree_store: gio::ListStore,
    tree_model: gtk::TreeListModel,
    tree_sel: gtk::SingleSelection,
    tree_list: gtk::ListView,
    tree_rows: RefCell<HashMap<gtk::ListItem, TreeRow>>,
    /// The tree is shown in place of the strip.
    tree_view: Cell<bool>,
    /// The folder shown, in the tree view.
    folder: Cell<Option<usize>>,
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
        // Activated by Enter or a double click only, since activating a group
        // takes the keyboard to its images: on a single click, the default,
        // the strip lost the keyboard to the images on every click, and Up
        // and Down then moved through the images instead of the groups.
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk::SingleSelection::builder().model(&store).autoselect(true).can_unselect(false).build();
        let factory = gtk::SignalListItemFactory::new();
        let list = gtk::ListView::builder().model(&selection).factory(&factory).single_click_activate(false).css_classes(["strip"]).build();
        let list_scroll = gtk::ScrolledWindow::builder()
            .child(&list)
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
        let tree = Rc::new(RefCell::new(Tree { folders: Vec::new(), top: Vec::new() }));
        let tree_store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let tree_model = gtk::TreeListModel::new(tree_store.clone(), false, false, {
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
        let tree_sel = gtk::SingleSelection::builder().model(&tree_model).autoselect(false).can_unselect(false).build();
        let tree_factory = gtk::SignalListItemFactory::new();
        let tree_list = gtk::ListView::builder().model(&tree_sel).factory(&tree_factory).single_click_activate(false).css_classes(["tree"]).build();
        let tree_scroll = gtk::ScrolledWindow::builder()
            .child(&tree_list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .min_content_width(TREE_W)
            .width_request(TREE_W)
            .propagate_natural_width(false)
            .hexpand(false)
            .build();

        let left_label = gtk::Label::with_mnemonic(l::GROUPS);
        left_label.set_mnemonic_widget(Some(&list));
        left_label.set_xalign(0.0);
        left_label.set_margin_start(8);
        left_label.add_css_class("heading");
        let left_stack = gtk::Stack::builder().hhomogeneous(false).vexpand(true).build();
        left_stack.add_named(&list_scroll, Some("groups"));
        left_stack.add_named(&tree_scroll, Some("tree"));
        left_stack.set_visible_child_name("groups");
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
             By correlation: keep each group's reference image, delete the images that agree with it closely, keep the rest, and call weak matches weak.\n\
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
        fill_menu(&menu, false);
        let more = gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&menu).tooltip_text("The selected image, the group or folder, and the view").build();

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.append(&prev);
        head.append(&next);
        head.append(&title);
        title.set_hexpand(true);
        title.set_margin_start(4);
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
            list,
            selection,
            store,
            strip: RefCell::new(HashMap::new()),
            faces: RefCell::new(Vec::new()),
            quiet: Cell::new(false),
            tree,
            tree_store,
            tree_model,
            tree_sel,
            tree_list,
            tree_rows: RefCell::new(HashMap::new()),
            tree_view: Cell::new(false),
            folder: Cell::new(None),
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
        next.connect_clicked(with(|me| me.step(1)));
        me.trash.connect_clicked(with(|me| me.confirm_trash()));
        mark_suggested.connect_clicked(with(|me| me.mark_suggested()));

        let actions = gio::SimpleActionGroup::new();
        let action = |name: &str, f: fn(&Rc<Results>)| {
            let a = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(&me);
            a.connect_activate(move |_, _| {
                if let Some(me) = weak.upgrade() {
                    f(&me);
                }
            });
            actions.add_action(&a);
            a
        };
        let image_actions = vec![
            action("enlarge", |me| {
                if let Some(i) = me.selected.get() {
                    me.preview(i);
                }
            }),
            action("open", |me| me.launch(false)),
            action("folder", |me| me.launch(true)),
            action("mark-others", |me| me.mark_all_but_selected()),
        ];
        action("unmark-group", |me| me.unmark_group());
        action("unmark-all", |me| me.unmark_all());
        action("mark-folder", |me| me.mark_folder());
        action("toggle-view", |me| me.toggle_view());
        *me.image_actions.borrow_mut() = image_actions;
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
                me.strip.borrow_mut().insert(item.clone(), row);
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
                me.strip.borrow_mut().remove(item);
            }
        });
        me.selection.connect_selected_notify({
            let weak = Rc::downgrade(&me);
            move |sel| {
                let Some(me) = weak.upgrade() else { return };
                if me.quiet.get() || sel.selected() == gtk::INVALID_LIST_POSITION {
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
                me.tree_rows.borrow_mut().insert(item.clone(), row);
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
                if let Some(r) = me.tree_rows.borrow().get(item) {
                    r.folder.set(None);
                    r.expander.set_list_row(None);
                }
            }
        });
        tree_factory.connect_teardown({
            let weak = Rc::downgrade(&me);
            move |_, item| {
                let (Some(me), Some(item)) = (weak.upgrade(), item.downcast_ref::<gtk::ListItem>()) else { return };
                me.tree_rows.borrow_mut().remove(item);
            }
        });
        me.tree_sel.connect_selected_notify({
            let weak = Rc::downgrade(&me);
            move |sel| {
                let Some(me) = weak.upgrade() else { return };
                if me.quiet.get() || !me.tree_view.get() {
                    return;
                }
                if let Some(f) = me.folder_at(sel.selected()) {
                    me.show_folder(f);
                }
            }
        });
        // Enter on a folder takes the keyboard to its images.
        me.tree_list.connect_activate({
            let weak = Rc::downgrade(&me);
            move |_, _| {
                if let Some(me) = weak.upgrade() {
                    me.focus();
                }
            }
        });
        // Enter on a group takes the keyboard to its images.
        me.list.connect_activate({
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
        // to the window alone. Only while the page is showing.
        //
        // The image keys work from anywhere on the page: the arrows move the
        // selection by the layout, left and right through the group's order,
        // up and down to the nearest image of the row above or below. What
        // they leave alone: Up, Down, Home, End, Space and Enter in the strip,
        // which change and open groups there, and Space and Enter on a button
        // or the rule picker, which press it.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let weak = Rc::downgrade(&me);
            move |_, key, _, mods| {
                let Some(me) = weak.upgrade() else { return glib::Propagation::Proceed };
                if !me.showing() || mods.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
                    return glib::Propagation::Proceed;
                }
                use gdk::Key as K;
                if me.cards.borrow().is_empty() && !(me.tree_view.get() && matches!(key, K::Left | K::KP_Left | K::Right | K::KP_Right)) {
                    return glib::Propagation::Proceed;
                }
                let focus = gtk::prelude::GtkWindowExt::focus(&me.app.window);
                let on_tile = focus.as_ref().is_some_and(|f| me.tiles.borrow().iter().any(|t| t.child.upcast_ref::<gtk::Widget>() == f));
                let side = me.side_list();
                let in_strip = focus.as_ref().is_some_and(|f| f.is_ancestor(&side) || f == side.upcast_ref::<gtk::Widget>());
                // Left and Right in the tree close and open its folders.
                if in_strip && me.tree_view.get() && matches!(key, K::Left | K::KP_Left | K::Right | K::KP_Right) {
                    me.expand_selected(matches!(key, K::Right | K::KP_Right));
                    return glib::Propagation::Stop;
                }
                let theirs = match key {
                    K::Up | K::KP_Up | K::Down | K::KP_Down | K::Home | K::KP_Home | K::End | K::KP_End => in_strip,
                    K::space | K::Return | K::KP_Enter | K::ISO_Enter => focus.is_some() && !on_tile,
                    _ => false,
                };
                if theirs {
                    return glib::Propagation::Proceed;
                }
                let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
                // With Shift, every image passed on the way takes the mark
                // the anchor was given (`mark_along`).
                let go = |i: i64| match (shift, me.selected.get()) {
                    (true, Some(from)) => me.mark_along(from, i),
                    _ => me.move_to(i),
                };
                match (key, me.selected.get()) {
                    (K::Left | K::KP_Left, Some(i)) => go(i as i64 - 1),
                    (K::Right | K::KP_Right, Some(i)) => go(i as i64 + 1),
                    (K::Up | K::KP_Up, Some(i)) => {
                        if let Some(j) = me.mosaic.vertical_neighbour(i, false) {
                            go(j as i64);
                        }
                    }
                    (K::Down | K::KP_Down, Some(i)) => {
                        if let Some(j) = me.mosaic.vertical_neighbour(i, true) {
                            go(j as i64);
                        }
                    }
                    // Nothing selected yet: any arrow starts at the first.
                    (K::Left | K::KP_Left | K::Right | K::KP_Right | K::Up | K::KP_Up | K::Down | K::KP_Down, None) => go(0),
                    (K::Home | K::KP_Home, _) => go(0),
                    (K::End | K::KP_End, _) => go(i64::MAX),
                    (K::space | K::Delete | K::KP_Delete, Some(i)) => me.toggle(i),
                    (K::Return | K::KP_Enter | K::ISO_Enter, Some(i)) => me.preview(i),
                    (K::Menu, Some(i)) => me.popup_menu(i, None),
                    (K::F10, Some(i)) if shift => me.popup_menu(i, None),
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        });
        me.app.window.add_controller(keys);
        // The menu's items are Alt and their letter from anywhere on the
        // page, as a button's mnemonic would be.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let actions = actions.clone();
            let weak = Rc::downgrade(&me);
            move |_, key, _, mods| {
                let Some(me) = weak.upgrade().filter(|me| me.showing()) else {
                    return glib::Propagation::Proceed;
                };
                if !mods.contains(gdk::ModifierType::ALT_MASK) || mods.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK) {
                    return glib::Propagation::Proceed;
                }
                let Some(c) = key.to_lower().to_unicode() else { return glib::Propagation::Proceed };
                let Some((_, name)) = menu_of(me.tree_view.get()).iter().flat_map(|p| p.iter()).find(|(label, _)| l::letter(label) == c) else {
                    return glib::Propagation::Proceed;
                };
                if actions.is_action_enabled(name) {
                    actions.activate_action(name, None);
                }
                glib::Propagation::Stop
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
        // Ctrl+Page Down / Up change group from anywhere on the page.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let weak = Rc::downgrade(&me);
            move |_, key, _, mods| {
                let Some(me) = weak.upgrade() else { return glib::Propagation::Proceed };
                if !me.showing() || !mods.contains(gdk::ModifierType::CONTROL_MASK) {
                    return glib::Propagation::Proceed;
                }
                match key {
                    gdk::Key::Page_Down | gdk::Key::KP_Page_Down => me.step(1),
                    gdk::Key::Page_Up | gdk::Key::KP_Page_Up => me.step(-1),
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        });
        me.app.window.add_controller(keys);
        me
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
        let Prepared { groups, paths, sizes, by_mode, analysed, kept, roots } = prepared;
        {
            let mut s = self.state.borrow_mut();
            // The rule chosen stays chosen from one scan to the next.
            let mode = s.mode;
            let n = paths.len();
            *s = State {
                groups,
                paths,
                sizes,
                by_mode,
                marked: vec![false; n],
                gone: vec![false; n],
                problems,
                analysed,
                has_results: true,
                mode,
                kept,
                roots,
                ..State::default()
            };
        }
        self.marks_pending.set(false);
        self.thumbs.clear_queue();
        self.strip_thumbs.clear_queue();
        // Another scan's folders: the tree starts again from the top.
        self.folder.set(None);
        self.rebuild(0);
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
        let paths = s.marked_ids().filter(|&id| !s.gone[id as usize]).map(|id| s.paths[id as usize].clone()).collect();
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
        if self.tree_view.get() { self.tree_list.clone() } else { self.list.clone() }
    }

    /// The strip of groups, from the state, with `select` selected.
    fn rebuild(self: &Rc<Self>, select: usize) {
        {
            let mut s = self.state.borrow_mut();
            let mut member_of: Vec<Vec<usize>> = vec![Vec::new(); s.paths.len()];
            for (gi, g) in s.groups.iter().enumerate() {
                for f in &g.files {
                    member_of[f.id as usize].push(gi);
                }
            }
            s.member_of = member_of;
        }
        self.rebuild_tree();
        let faces: Vec<PathBuf> = {
            let s = self.state.borrow();
            s.groups.iter().map(|g| face(&g.files).map(|f| f.path.clone()).unwrap_or_default()).collect()
        };
        let n = faces.len();
        *self.faces.borrow_mut() = faces;
        // Every row is told its group again, since the groups are new.
        self.quiet.set(true);
        let items: Vec<glib::BoxedAnyObject> = (0..n).map(glib::BoxedAnyObject::new).collect();
        self.store.splice(0, self.store.n_items(), &items);
        self.quiet.set(false);
        let empty = n == 0;
        for b in &self.group_buttons {
            b.set_sensitive(!empty);
        }
        if empty {
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
        self.quiet.set(true);
        self.selection.set_selected(i as u32);
        self.quiet.set(false);
        if self.tree_view.get() {
            self.state.borrow_mut().current = i;
            let top = self.tree.borrow().top.first().copied();
            if let Some(f) = self.folder.get().or(top) {
                self.select_folder(f);
            }
        } else {
            self.show_group(i);
            self.scroll_strip_to(i);
        }
        self.update_status();
    }

    /// The folder tree, from the groups as they are now, its top folders
    /// open. The folder shown stays shown if it still holds an image.
    fn rebuild_tree(&self) {
        let before = self.folder.get().and_then(|f| self.tree.borrow().folders.get(f).map(|f| f.path.clone()));
        let tree = {
            let s = self.state.borrow();
            let files: Vec<(u32, &Path)> =
                (0..s.paths.len()).filter(|&i| !s.member_of[i].is_empty()).map(|i| (i as u32, s.paths[i].as_path())).collect();
            folders::build(&s.roots, &files)
        };
        let after = before.and_then(|p| tree.find(&p));
        let top: Vec<glib::BoxedAnyObject> = tree.top.iter().map(|&t| glib::BoxedAnyObject::new(t)).collect();
        *self.tree.borrow_mut() = tree;
        self.folder.set(after);
        self.quiet.set(true);
        self.tree_store.splice(0, self.tree_store.n_items(), &top);
        for i in (0..top.len()).rev() {
            if let Some(r) = self.tree_model.child_row(i as u32) {
                r.set_expanded(true);
            }
        }
        self.quiet.set(false);
    }

    /// The folder at row `pos` of the tree.
    fn folder_at(&self, pos: u32) -> Option<usize> {
        let row = self.tree_model.row(pos)?;
        let o = row.item().and_downcast::<glib::BoxedAnyObject>()?;
        Some(*o.borrow::<usize>())
    }

    /// The row of the tree that shows folder `f`, if it is open to it.
    fn row_of(&self, f: usize) -> Option<u32> {
        (0..self.tree_model.n_items()).find(|&p| self.folder_at(p) == Some(f))
    }

    /// Open the tree to folder `f`, select it and show it.
    fn select_folder(self: &Rc<Self>, f: usize) {
        let chain = self.tree.borrow().ancestors(f);
        self.quiet.set(true);
        for a in chain {
            if let Some(r) = self.row_of(a).and_then(|p| self.tree_model.row(p)) {
                r.set_expanded(true);
            }
        }
        let pos = self.row_of(f);
        if let Some(p) = pos {
            self.tree_sel.set_selected(p);
        }
        self.quiet.set(false);
        self.show_folder(f);
        if let Some(p) = pos {
            let _ = self.tree_list.activate_action("list.scroll-to-item", Some(&p.to_variant()));
        }
    }

    /// Open the selected folder, or close it; closing a closed one goes to
    /// the folder above.
    fn expand_selected(self: &Rc<Self>, open: bool) {
        let Some(row) = self.tree_model.row(self.tree_sel.selected()) else { return };
        if open {
            row.set_expanded(true);
        } else if row.is_expanded() {
            row.set_expanded(false);
        } else if let Some(parent) = row.parent() {
            let p = (0..self.tree_model.n_items()).find(|&p| self.tree_model.row(p).as_ref() == Some(&parent));
            if let Some(p) = p {
                self.tree_sel.set_selected(p);
                let _ = self.tree_list.activate_action("list.scroll-to-item", Some(&p.to_variant()));
            }
        }
    }

    /// Row `item` of the tree now shows the folder its item names.
    fn bind_tree_row(&self, item: &gtk::ListItem) {
        let Some(row) = item.item().and_downcast::<gtk::TreeListRow>() else { return };
        let Some(f) = row.item().and_downcast::<glib::BoxedAnyObject>().map(|o| *o.borrow::<usize>()) else { return };
        let rows = self.tree_rows.borrow();
        let Some(r) = rows.get(item) else { return };
        r.expander.set_list_row(Some(&row));
        r.folder.set(Some(f));
        r.shown.set(None);
        let t = self.tree.borrow();
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
            let (s, t) = (self.state.borrow(), self.tree.borrow());
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
            let t = self.tree.borrow();
            let Some(folder) = t.folders.get(f) else { return };
            (folder.path.clone(), folder.own, folder.all.clone())
        };
        self.folder.set(Some(f));
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

    /// Show the tree in place of the strip, or the strip again.
    fn toggle_view(self: &Rc<Self>) {
        let tree = !self.tree_view.get();
        self.tree_view.set(tree);
        fill_menu(&self.menu, tree);
        self.left.set_visible_child_name(if tree { "tree" } else { "groups" });
        self.left_label.set_text_with_mnemonic(if tree { l::FOLDERS_TREE } else { l::GROUPS });
        self.left_label.set_mnemonic_widget(Some(&self.side_list()));
        let what = if tree { "folder" } else { "group" };
        self.prev.set_tooltip_text(Some(&format!("Previous {what} (Ctrl+Page Up)")));
        self.next.set_tooltip_text(Some(&format!("Next {what} (Ctrl+Page Down)")));
        if !self.state.borrow().groups.is_empty() {
            if tree {
                let top = self.tree.borrow().top.first().copied();
                if let Some(f) = self.folder.get().or(top) {
                    self.select_folder(f);
                }
            } else {
                let current = self.state.borrow().current;
                self.show_group(current);
                self.scroll_strip_to(current);
            }
        }
        self.side_list().grab_focus();
    }

    /// Show what is shown again, as the rule now suggests it.
    fn reshow(self: &Rc<Self>) {
        match (self.tree_view.get(), self.folder.get()) {
            (true, Some(f)) => self.show_folder(f),
            (true, None) => {}
            (false, _) => {
                let current = self.state.borrow().current;
                if self.state.borrow().groups.get(current).is_some() {
                    self.show_group(current);
                }
            }
        }
    }

    /// The files the group or folder on screen holds: in the tree view, every
    /// image under the folder.
    fn view_ids(&self) -> Vec<u32> {
        match (self.tree_view.get(), self.folder.get()) {
            (true, Some(f)) => {
                let s = self.state.borrow();
                self.tree.borrow().folders.get(f).map_or_else(Vec::new, |f| f.all.iter().copied().filter(|&id| !s.gone[id as usize]).collect())
            }
            (true, None) => Vec::new(),
            (false, _) => self.cards.borrow().iter().map(|c| c.id).collect(),
        }
    }

    /// Mark every image under the folder shown.
    fn mark_folder(self: &Rc<Self>) {
        let ids = self.view_ids();
        self.mark_ids(&ids, true);
        self.update_status();
        self.refresh_bar();
    }

    fn scroll_strip_to(&self, i: usize) {
        let _ = self.list.activate_action("list.scroll-to-item", Some(&(i as u32).to_variant()));
    }

    /// Row `item` of the strip now shows the group its item names.
    fn bind_row(&self, item: &gtk::ListItem) {
        let Some(gi) = item.item().and_downcast::<glib::BoxedAnyObject>().map(|o| *o.borrow::<usize>()) else { return };
        let strip = self.strip.borrow();
        let Some(row) = strip.get(item) else { return };
        row.group.set(Some(gi));
        row.shown.set(None);
        self.show_row(row, gi);
        row.picture.set_texture(None);
        let Some(face) = self.faces.borrow().get(gi).cloned() else { return };
        let scale = self.app.window.scale_factor().max(1) as u32;
        let size = Size::cover(STRIP_W as u32 * scale, STRIP_H as u32 * scale);
        let ticket = self.strip_thumbs.request(&face, size, Shown::Still(row.picture.clone()), false);
        *row.request.borrow_mut() = Some(((face, size), ticket));
    }

    /// Row `item` of the strip no longer shows a group: its picture is not
    /// wanted, and not held.
    fn unbind_row(&self, item: &gtk::ListItem) {
        let strip = self.strip.borrow();
        let Some(row) = strip.get(item) else { return };
        row.group.set(None);
        if let Some((key, ticket)) = row.request.borrow_mut().take() {
            self.strip_thumbs.withdraw(&key, ticket);
        }
        row.picture.set_texture(None);
    }

    /// Group `gi`'s row of the strip, if one shows it, says what it holds.
    fn update_row(&self, gi: usize) {
        let strip = self.strip.borrow();
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

        let dims = |files: &[Member]| files.iter().map(|m| (m.width.unwrap_or(0), m.height.unwrap_or(0))).collect::<Vec<_>>();
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
        let (action, marked) = {
            let s = self.state.borrow();
            (s.action(m.id), s.is_marked(m.id))
        };
        let review = action == Some(Action::Review);
        if t.weak.is_visible() != review {
            t.weak.set_visible(review);
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
        let weak = gtk::Box::builder().css_classes(["weak"]).halign(gtk::Align::End).valign(gtk::Align::Start).tooltip_text("Weak match").visible(false).build();
        child.add_overlay(&weak);
        let pill = gtk::Label::builder().css_classes(["pill"]).halign(gtk::Align::End).valign(gtk::Align::End).margin_end(6).margin_bottom(6).visible(false).build();
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
        Tile { child, still, weak, pill, item }
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
        let mut note = match (m.is_representative(), reference.and_then(|r| Some((r.width?, r.height?)))) {
            (false, Some((w, h))) => format!("the reference is {w} × {h}"),
            _ => String::new(),
        };
        // In the tree view, which group the match is in.
        if self.tree_view.get() {
            let groups = s.member_of.get(m.id as usize).map_or(0, Vec::len);
            let more = if groups > 1 { format!(" (and {} more)", groups - 1) } else { String::new() };
            note = [format!("group {}{more}", gi + 1), note].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ");
        }
        d.facts_note.set_text(&note);
        d.why.set_text(&evidence(&m));
        let action = s.action(m.id);
        let kept = match (action, &m.kept_copy) {
            (Some(Action::Delete), Some(k)) if s.mode == 0 => format!("Everything in it is also in {}", file_name(k)),
            _ => String::new(),
        };
        d.why_note.set_text(&kept);
        d.suggestion.set_text(suggestion(action));
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
            d.suggestion.set_text(&format!("{} · marked", suggestion(action)));
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
            for r in self.tree_rows.borrow().values() {
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
        let others: Vec<u32> = self.view_ids().into_iter().filter(|&id| id != this).collect();
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
        let (off, on): (Vec<u32>, Vec<u32>) = {
            let s = self.state.borrow();
            let delete = |id: u32| s.action(id) == Some(Action::Delete);
            let all = (0..s.paths.len() as u32).filter(|&id| !s.gone[id as usize]);
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
        self.reshow();
        self.select(selected);
        self.refresh_bar();
        self.update_status();
    }

    /// Unmark the group shown, or every image under the folder shown.
    fn unmark_group(self: &Rc<Self>) {
        let ids = self.view_ids();
        self.mark_ids(&ids, false);
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

    fn step(self: &Rc<Self>, by: i32) {
        if self.tree_view.get() {
            let n = self.tree_model.n_items() as i64;
            let cur = self.tree_sel.selected();
            if n == 0 || cur == gtk::INVALID_LIST_POSITION {
                return;
            }
            let next = (cur as i64 + by as i64).clamp(0, n - 1) as u32;
            self.tree_sel.set_selected(next);
            let _ = self.tree_list.activate_action("list.scroll-to-item", Some(&next.to_variant()));
            self.focus();
            return;
        }
        let (cur, n) = {
            let s = self.state.borrow();
            (s.current as i32, s.groups.len() as i32)
        };
        if n == 0 {
            return;
        }
        let next = (cur + by).clamp(0, n - 1) as usize;
        self.selection.set_selected(next as u32);
        // The strip shows it; the keyboard goes to the images.
        self.scroll_strip_to(next);
        self.focus();
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
        let live = || (0..s.paths.len() as u32).filter(|&id| !s.gone[id as usize]);
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
        let info = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::Middle).build();
        let prev = gtk::Button::with_mnemonic(l::PREVIEW_PREV);
        prev.set_tooltip_text(Some("Left arrow"));
        let next = gtk::Button::with_mnemonic(l::PREVIEW_NEXT);
        next.set_tooltip_text(Some("Right arrow"));
        let mark = gtk::CheckButton::with_mnemonic(l::PREVIEW_MARK);
        mark.set_tooltip_text(Some("Space"));
        let close = gtk::Button::with_mnemonic(l::PREVIEW_CLOSE);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.set_margin_start(12);
        bar.set_margin_end(12);
        bar.set_margin_top(8);
        bar.set_margin_bottom(12);
        for w in [info.upcast_ref::<gtk::Widget>(), prev.upcast_ref(), next.upcast_ref(), mark.upcast_ref(), close.upcast_ref()] {
            bar.append(w);
        }
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&picture);
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
        let latest = Latest::default();
        let show = {
            let (me, picture, info, mark, win, at, quiet) = (Rc::downgrade(self), picture.clone(), info.clone(), mark.clone(), win.clone(), at.clone(), quiet.clone());
            Rc::new(move || {
                let Some(me) = me.upgrade() else { return };
                let cards = me.cards.borrow();
                let n = cards.len();
                let Some(card) = cards.get(at.get()) else { return };
                let path = card.path.clone();
                let s = me.state.borrow();
                let member = me.shown.borrow().get(at.get()).map(|(m, _)| m.clone());
                let marked = member.as_ref().is_some_and(|m| s.is_marked(m.id));
                drop(s);
                drop(cards);
                win.set_title(Some(&format!("{} ({} of {n})", file_name(&path), at.get() + 1)));
                let text = match &member {
                    Some(m) => {
                        let action = me.state.borrow().action(m.id);
                        format!("{}  ·  {}  ·  {}  ·  {}", path.display(), facts(m), evidence(m), suggestion(action))
                    }
                    None => path.display().to_string(),
                };
                info.set_text(&text);
                info.set_tooltip_text(Some(&text));
                quiet.set(true);
                mark.set_active(marked);
                quiet.set(false);
                picture.set_paintable(None::<&gdk::Paintable>);
                let scale = win.scale_factor().max(1) as u32;
                // Every picture this view asks for is for the same widget, and
                // the answers come back in whatever order they are decoded; only
                // the one asked for last is shown. See `Latest`.
                let current = latest.next();
                let pic = picture.downgrade();
                let shown = Shown::Callback(Box::new(move |r| {
                    if let (true, Some(p)) = (current(), pic.upgrade()) {
                        show_on(&p, r);
                    }
                }));
                me.thumbs.request(&path, Size::within(LARGE * scale), shown, true);
            })
        };
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
            let (go, mark, win) = (go.clone(), mark.clone(), win.clone());
            move |_, key, _, mods| {
                if mods.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
                    return glib::Propagation::Proceed;
                }
                match key {
                    gdk::Key::Escape => win.close(),
                    gdk::Key::Left | gdk::Key::KP_Left | gdk::Key::Page_Up => go(-1),
                    gdk::Key::Right | gdk::Key::KP_Right | gdk::Key::Page_Down => go(1),
                    gdk::Key::space | gdk::Key::Delete | gdk::Key::KP_Delete => mark.set_active(!mark.is_active()),
                    _ => return glib::Propagation::Proceed,
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
        let (paths, bytes, whole) = {
            let s = self.state.borrow();
            let mut paths: Vec<PathBuf> = s.marked_ids().map(|id| s.paths[id as usize].clone()).collect();
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
            (paths, bytes, whole)
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
                me.trash(paths).await;
            }
        });
    }

    async fn trash(self: &Rc<Self>, paths: Vec<PathBuf>) {
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
        for (i, p) in paths.iter().enumerate() {
            self.status.set_text(&format!("Moving to the Trash: {} of {n}…", i + 1));
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
    /// What each rule suggested for the others stays what it was with them.
    fn drop_gone(self: &Rc<Self>, gone: &HashSet<PathBuf>) {
        for p in gone {
            self.thumbs.forget(p);
            self.strip_thumbs.forget(p);
        }
        let mut unmarked = false;
        let select = {
            let mut s = self.state.borrow_mut();
            let ids: Vec<u32> = (0..s.paths.len() as u32).filter(|&id| gone.contains(&s.paths[id as usize])).collect();
            for id in ids {
                if s.set_mark(id, false) {
                    unmarked = true;
                }
                s.gone[id as usize] = true;
                for m in s.by_mode.iter_mut() {
                    m[id as usize] = None;
                }
            }
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
        if let (Some(w), Some(h)) = (f.width, f.height) {
            *sizes.entry((w, h)).or_default() += 1;
        }
    }
    let Some((&size, _)) = sizes.iter().max_by_key(|&(&(w, h), &n)| (n, w as u64 * h as u64)) else { return files.first() };
    let same = || files.iter().filter(move |f| (f.width, f.height) == (Some(size.0), Some(size.1)));
    // An inverted or mirrored copy is the same size and the wrong picture.
    let upright = |f: &&Member| f.inverted != Some(true) && f.mirrored != Some(true);
    same()
        .filter(upright)
        .find(|f| f.suggested() == Some(Action::Keep))
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
    if m.is_representative() {
        return "reference image".into();
    }
    if m.relation.as_deref() == Some("identical") {
        return "identical copy of the reference".into();
    }
    if m.relation.as_deref() == Some("same_pixels") {
        return "same picture as the reference, saved differently".into();
    }
    let (Some(ov), Some(corr)) = (m.frame_overlap, m.pixel_correlation) else {
        return String::new();
    };
    // A corroborated match cleared only the lower bar a pair inside an
    // existing group faces, so its correlation can sit below the one the scan
    // asked for; the text report says so, and so does the card.
    let mut s = if m.relation.as_deref() == Some("corroborated") { "corroborated · ".to_string() } else { String::new() };
    s.push_str(&format!("overlap {ov:.2} · correlation {corr:.2}"));
    if m.mirrored == Some(true) {
        s.push_str(" · mirrored");
    }
    if m.inverted == Some(true) {
        s.push_str(" · inverted");
    }
    s
}

/// The menu, in sections: a label and its action. Each label's letter is also
/// Alt and that letter anywhere on the page.
const MENU: &[&[(&str, &str)]] = &[
    &[(l::ENLARGE, "enlarge"), (l::OPEN, "open"), (l::SHOW_FOLDER, "folder")],
    &[(l::MARK_OTHERS, "mark-others"), (l::UNMARK_GROUP, "unmark-group"), (l::UNMARK_ALL, "unmark-all")],
    &[(l::TO_TREE, "toggle-view")],
];
/// The menu in the tree view, where the group's items are the folder's.
const MENU_TREE: &[&[(&str, &str)]] = &[
    &[(l::ENLARGE, "enlarge"), (l::OPEN, "open"), (l::SHOW_FOLDER, "folder")],
    &[(l::MARK_OTHERS, "mark-others"), (l::MARK_FOLDER, "mark-folder"), (l::UNMARK_FOLDER, "unmark-group"), (l::UNMARK_ALL_FOLDERS, "unmark-all")],
    &[(l::TO_GROUPS, "toggle-view")],
];

fn menu_of(tree: bool) -> &'static [&'static [(&'static str, &'static str)]] {
    if tree { MENU_TREE } else { MENU }
}

/// `menu`, as the view has it: the header's button and the right click share
/// it, so both change with it.
fn fill_menu(menu: &gio::Menu, tree: bool) {
    menu.remove_all();
    for part in menu_of(tree) {
        let section = gio::Menu::new();
        for (label, action) in part.iter() {
            let item = gio::MenuItem::new(Some(label), Some(&format!("results.{action}")));
            item.set_attribute_value("accel", Some(&format!("<Alt>{}", l::letter(label)).to_variant()));
            section.append_item(&item);
        }
        menu.append_section(None, &section);
    }
}

/// The rules the picker offers, in its order.
const RULES: &[&str] = &["Keep all content", "By correlation", "Reference images only"];
const MODES: [SuggestMode; 3] = [SuggestMode::Content, SuggestMode::Correlation, SuggestMode::Representative];

/// What `mode` suggests for each of `n` files of `groups`, by number. The
/// scan's own rule came with the report; the other two read the groups, as
/// the command line's do (`img_fp::by_group`). A file is numbered in the
/// order the groups first name it, which is the order `by_group` asks for.
fn suggestions(groups: &[GroupState], n: usize, mode: SuggestMode, min_correlation: f32) -> Vec<Option<Action>> {
    let files = || groups.iter().flat_map(|g| g.files.iter());
    if mode == SuggestMode::Content {
        let mut v = vec![None; n];
        for m in files() {
            if let Some(a) = m.suggested() {
                v[m.id as usize] = Some(a);
            }
        }
        return v;
    }
    let seats = files().map(|m| img_fp::Seat { file: m.id as usize, representative: m.is_representative(), correlation: m.correlation() });
    img_fp::by_group(mode, n, seats, min_correlation)
}

/// A scan's report made ready for the page, off the main thread: every
/// file numbered, and what each rule suggests for it. On IMGS-ALL's report
/// that is a fifth of a second the window used to spend frozen.
pub struct Prepared {
    groups: Vec<GroupState>,
    paths: Vec<PathBuf>,
    sizes: Vec<u64>,
    by_mode: Vec<Vec<Option<Action>>>,
    analysed: usize,
    kept: Option<u64>,
    roots: Vec<PathBuf>,
}

impl Prepared {
    /// The folders the scan was given.
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
    }

    /// Each file's path, by number.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn groups(&self) -> usize {
        self.groups.len()
    }

    pub fn analysed(&self) -> usize {
        self.analysed
    }

    /// It is the scan kept on disk as `id` (`last::Notes::id`).
    pub fn set_kept(&mut self, id: u64) {
        self.kept = Some(id);
    }
}

pub fn prepare(found: Found) -> Prepared {
    let Found { groups: report, analysed, min_correlation, kept } = found;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut sizes: Vec<u64> = Vec::new();
    let mut ids: HashMap<PathBuf, u32> = HashMap::new();
    let mut groups = Vec::with_capacity(report.len());
    for Group { mut files } in report {
        for f in files.iter_mut() {
            f.id = *ids.entry(f.path.clone()).or_insert_with(|| {
                paths.push(f.path.clone());
                sizes.push(f.size_bytes.unwrap_or(0));
                (paths.len() - 1) as u32
            });
        }
        groups.push(GroupState { files, reference_gone: false });
    }
    let n = paths.len();
    let by_mode = MODES.iter().map(|&m| suggestions(&groups, n, m, min_correlation)).collect();
    Prepared { groups, paths, sizes, by_mode, analysed, kept, roots: Vec::new() }
}

/// What is suggested for a file, in a few words.
fn suggestion(action: Option<Action>) -> &'static str {
    match action {
        Some(Action::Keep) => "suggested: keep",
        Some(Action::Delete) => "suggested: delete",
        Some(Action::Review) => "weak match",
        None => "",
    }
}

/// Dimensions and size, as far as the report knows them.
fn facts(m: &Member) -> String {
    let mut parts = Vec::new();
    if let (Some(w), Some(h)) = (m.width, m.height) {
        parts.push(format!("{w} × {h}"));
    }
    if let Some(b) = m.size_bytes {
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
    #[test]
    fn group_numbers_read_as_a_sentence() {
        assert_eq!(super::numbers(&[4]), "4");
        assert_eq!(super::numbers(&[4, 9]), "4 and 9");
        assert_eq!(super::numbers(&[4, 9, 12]), "4, 9 and 12");
        let many: Vec<usize> = (1..=23).collect();
        assert!(super::numbers(&many).ends_with("19, 20 and 3 more"));
    }
}
