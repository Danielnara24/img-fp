//! The second page: the groups a scan found, for choosing what to move to the
//! Trash.
//!
//! Nothing is marked to begin with. What a group says is that every file in
//! it was matched to the file at its head (its reference image); which of them
//! to keep is left to the person looking.
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
//! Keyboard: arrows move between images, Space or Delete marks the one under
//! the keyboard, Enter opens it large, Ctrl+Page Down / Ctrl+Page Up change
//! group, and every button has an Alt mnemonic.

use crate::labels as l;
use crate::scan::{Found, Group, Member};
use crate::thumbs::{Shown, Thumbs};
use crate::App;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Long side of a card's picture, in pixels.
const CARD: i32 = 200;
/// Long side of the large view's picture.
const LARGE: u32 = 1600;
/// Cards of the next group decoded ahead, for paging.
const AHEAD: usize = 16;

const CSS: &str = "
flowboxchild.card { border-radius: 8px; padding: 6px; }
flowboxchild.card.picked { outline: 3px solid @theme_selected_bg_color; outline-offset: -3px; }
flowboxchild.card.marked { background-color: alpha(@error_color, 0.16); }
flowboxchild.card.marked checkbutton label { color: @error_color; font-weight: bold; }
";

struct GroupState {
    files: Vec<Member>,
    /// Its reference image has been moved to the Trash.
    reference_gone: bool,
}

#[derive(Default)]
struct State {
    groups: Vec<GroupState>,
    marked: HashSet<PathBuf>,
    /// For each file, the groups it is in.
    member_of: HashMap<PathBuf, Vec<usize>>,
    sizes: HashMap<PathBuf, u64>,
    current: usize,
    problems: bool,
    analysed: usize,
    has_results: bool,
}

struct Card {
    path: PathBuf,
    child: gtk::FlowBoxChild,
    check: gtk::CheckButton,
}

pub struct Results {
    pub root: gtk::Box,
    app: Rc<App>,
    thumbs: Rc<Thumbs>,
    state: RefCell<State>,
    list: gtk::ListBox,
    row_labels: RefCell<Vec<gtk::Label>>,
    flow: gtk::FlowBox,
    title: gtk::Label,
    note: gtk::Label,
    status: gtk::Label,
    trash: gtk::Button,
    group_buttons: Vec<gtk::Widget>,
    cards: RefCell<Vec<Card>>,
    /// The image the buttons act on: the last one clicked or reached with
    /// the keyboard. Kept apart from focus, which a click on a button takes.
    picked: Cell<usize>,
    /// Set while code, not a person, is changing a check box.
    quiet: Cell<bool>,
    new_scan: RefCell<Option<Box<dyn Fn()>>>,
}

impl Results {
    pub fn new(app: Rc<App>) -> Rc<Results> {
        let css = gtk::CssProvider::new();
        css.load_from_data(CSS);
        // At the user's own priority, added after theirs so it wins: a user
        // stylesheet that themes every flow box child would otherwise hide
        // which images are marked.
        gtk::style_context_add_provider_for_display(&WidgetExt::display(&app.window), &css, gtk::STYLE_PROVIDER_PRIORITY_USER);

        let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::Browse).build();
        let list_scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .min_content_width(220)
            .build();
        let groups_label = gtk::Label::with_mnemonic(l::GROUPS);
        groups_label.set_mnemonic_widget(Some(&list));
        groups_label.set_xalign(0.0);
        groups_label.add_css_class("heading");
        let left = gtk::Box::new(gtk::Orientation::Vertical, 6);
        left.set_margin_start(12);
        left.set_margin_top(12);
        left.set_margin_bottom(12);
        left.append(&groups_label);
        left.append(&list_scroll);

        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .activate_on_single_click(false)
            .homogeneous(true)
            .valign(gtk::Align::Start)
            .row_spacing(8)
            .column_spacing(8)
            .max_children_per_line(12)
            .min_children_per_line(1)
            .build();
        let flow_scroll = gtk::ScrolledWindow::builder().child(&flow).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).hexpand(true).build();
        let images_label = gtk::Label::with_mnemonic(l::IMAGES);
        images_label.set_mnemonic_widget(Some(&flow));
        images_label.add_css_class("heading");
        let title = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::End).build();
        let note = gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["dim-label"]).visible(false).build();
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        head.append(&images_label);
        head.append(&title);

        let prev = gtk::Button::with_mnemonic(l::PREV_GROUP);
        prev.set_tooltip_text(Some("Ctrl+Page Up"));
        let next = gtk::Button::with_mnemonic(l::NEXT_GROUP);
        next.set_tooltip_text(Some("Ctrl+Page Down"));
        let mark_others = gtk::Button::with_mnemonic(l::MARK_OTHERS);
        mark_others.set_tooltip_text(Some("Mark every other image in this group, and keep the outlined one."));
        let unmark = gtk::Button::with_mnemonic(l::UNMARK_GROUP);
        let open = gtk::Button::with_mnemonic(l::OPEN);
        open.set_tooltip_text(Some("Open the outlined image in its usual application."));
        let folder = gtk::Button::with_mnemonic(l::SHOW_FOLDER);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        for b in [&prev, &next, &mark_others, &unmark, &open, &folder] {
            tools.append(b);
        }

        let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
        right.set_margin_end(12);
        right.set_margin_top(12);
        right.set_margin_bottom(12);
        right.append(&head);
        right.append(&note);
        right.append(&tools);
        right.append(&flow_scroll);

        let paned = gtk::Paned::builder().orientation(gtk::Orientation::Horizontal).start_child(&left).end_child(&right).position(260).vexpand(true).build();
        paned.set_shrink_start_child(false);

        let status = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::End).build();
        let hint = gtk::Label::builder()
            .label("F1 keyboard shortcuts")
            .css_classes(["dim-label", "caption"])
            .build();
        let log = gtk::Button::with_mnemonic(l::RESULTS_LOG);
        let new_scan = gtk::Button::with_mnemonic(l::NEW_SCAN);
        let trash = gtk::Button::with_mnemonic(l::TRASH);
        trash.add_css_class("destructive-action");
        trash.set_sensitive(false);
        let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bottom.set_margin_start(12);
        bottom.set_margin_end(12);
        bottom.set_margin_top(8);
        bottom.set_margin_bottom(12);
        bottom.append(&status);
        bottom.append(&hint);
        bottom.append(&log);
        bottom.append(&new_scan);
        bottom.append(&trash);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&paned);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&bottom);

        let group_buttons: Vec<gtk::Widget> =
            [&prev, &next, &mark_others, &unmark, &open, &folder].iter().map(|b| b.upcast_ref::<gtk::Widget>().clone()).collect();
        let me = Rc::new(Results {
            root,
            app: app.clone(),
            thumbs: Thumbs::new(),
            state: RefCell::new(State::default()),
            list,
            row_labels: RefCell::new(Vec::new()),
            flow,
            title,
            note,
            status,
            trash,
            group_buttons,
            cards: RefCell::new(Vec::new()),
            picked: Cell::new(0),
            quiet: Cell::new(false),
            new_scan: RefCell::new(None),
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
        mark_others.connect_clicked(with(|me| me.mark_all_but_focused()));
        unmark.connect_clicked(with(|me| me.unmark_group()));
        open.connect_clicked(with(|me| me.launch(false)));
        folder.connect_clicked(with(|me| me.launch(true)));
        me.trash.connect_clicked(with(|me| me.confirm_trash()));
        new_scan.connect_clicked(with(|me| {
            if let Some(f) = me.new_scan.borrow().as_ref() {
                f();
            }
        }));
        log.connect_clicked({
            let app = app.clone();
            move |_| app.show_log()
        });

        me.list.connect_row_selected({
            let weak = Rc::downgrade(&me);
            move |_, row| {
                let (Some(me), Some(row)) = (weak.upgrade(), row) else { return };
                me.show_group(row.index() as usize);
            }
        });
        // Enter, and a double click, open an image large.
        me.flow.connect_child_activated({
            let weak = Rc::downgrade(&me);
            move |_, child| {
                if let Some(me) = weak.upgrade() {
                    me.preview(child.index() as usize);
                }
            }
        });
        // Space and Delete mark the image under the keyboard, taken before the
        // flow box sees them, since Space would otherwise activate the card.
        // The arrows are here too: the flow box moves its own cursor only
        // once a click or a Tab has put it there, and not when the keyboard
        // was handed to a card from code, which is how this page gives it.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let weak = Rc::downgrade(&me);
            move |_, key, _, mods| {
                let Some(me) = weak.upgrade() else { return glib::Propagation::Proceed };
                if mods.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
                    return glib::Propagation::Proceed;
                }
                let Some(i) = me.focused_card() else { return glib::Propagation::Proceed };
                match key {
                    gdk::Key::space | gdk::Key::Delete | gdk::Key::KP_Delete => me.toggle(i),
                    gdk::Key::Left | gdk::Key::KP_Left => me.move_to(i as i64 - 1),
                    gdk::Key::Right | gdk::Key::KP_Right => me.move_to(i as i64 + 1),
                    gdk::Key::Up | gdk::Key::KP_Up => me.move_to(i as i64 - me.columns()),
                    gdk::Key::Down | gdk::Key::KP_Down => me.move_to(i as i64 + me.columns()),
                    gdk::Key::Home | gdk::Key::KP_Home => me.move_to(0),
                    gdk::Key::End | gdk::Key::KP_End => me.move_to(i64::MAX),
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        });
        me.flow.add_controller(keys);
        // Ctrl+Page Down / Up change group from anywhere on the page.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let weak = Rc::downgrade(&me);
            move |_, key, _, mods| {
                let Some(me) = weak.upgrade() else { return glib::Propagation::Proceed };
                if !mods.contains(gdk::ModifierType::CONTROL_MASK) {
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
        me.root.add_controller(keys);
        me
    }

    pub fn set_new_scan(&self, f: impl Fn() + 'static) {
        *self.new_scan.borrow_mut() = Some(Box::new(f));
    }

    pub fn has_results(&self) -> bool {
        self.state.borrow().has_results
    }

    /// A finished scan's groups, replacing whatever was shown.
    pub fn show(self: &Rc<Self>, found: Found, problems: bool) {
        let Found { groups, analysed } = found;
        {
            let mut s = self.state.borrow_mut();
            *s = State { problems, analysed, has_results: true, ..State::default() };
            for g in groups {
                let Group { files } = g;
                for f in &files {
                    if let Some(b) = f.size_bytes {
                        s.sizes.insert(f.path.clone(), b);
                    }
                }
                s.groups.push(GroupState { files, reference_gone: false });
            }
        }
        self.thumbs.clear_queue();
        self.rebuild(0);
    }

    /// Keyboard to the page: the images, or the list when there are none.
    pub fn focus(&self) {
        let first = self.flow.child_at_index(0);
        match first {
            Some(c) => {
                c.grab_focus();
            }
            None => {
                self.list.grab_focus();
            }
        }
    }

    /// The list of groups, from the state, with `select` selected.
    fn rebuild(self: &Rc<Self>, select: usize) {
        {
            let mut s = self.state.borrow_mut();
            let mut member_of: HashMap<PathBuf, Vec<usize>> = HashMap::new();
            for (gi, g) in s.groups.iter().enumerate() {
                for f in &g.files {
                    member_of.entry(f.path.clone()).or_default().push(gi);
                }
            }
            s.member_of = member_of;
        }
        // Rows are replaced while the list is unselected, so that each one
        // added does not show its group.
        self.list.unselect_all();
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        let n = self.state.borrow().groups.len();
        let mut labels = Vec::with_capacity(n);
        for _ in 0..n {
            let label = gtk::Label::builder().xalign(0.0).margin_start(8).margin_end(8).margin_top(6).margin_bottom(6).build();
            self.list.append(&label);
            labels.push(label);
        }
        *self.row_labels.borrow_mut() = labels;
        for gi in 0..n {
            self.update_row(gi);
        }
        let empty = n == 0;
        for b in &self.group_buttons {
            b.set_sensitive(!empty);
        }
        if empty {
            self.clear_cards();
            let s = self.state.borrow();
            self.title.set_text("");
            let mut text = format!("No duplicates found among {} images.", s.analysed);
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
        if let Some(row) = self.list.row_at_index(i as i32) {
            self.list.select_row(Some(&row));
        }
        self.update_status();
    }

    fn update_row(&self, gi: usize) {
        let s = self.state.borrow();
        let Some(g) = s.groups.get(gi) else { return };
        let marked = g.files.iter().filter(|f| s.marked.contains(&f.path)).count();
        let mut text = format!("Group {} · {} images", gi + 1, g.files.len());
        if marked > 0 {
            text.push_str(&format!(" · {marked} marked"));
        }
        if let Some(label) = self.row_labels.borrow().get(gi) {
            label.set_text(&text);
        }
    }

    fn clear_cards(&self) {
        while let Some(c) = self.flow.child_at_index(0) {
            self.flow.remove(&c);
        }
        self.cards.borrow_mut().clear();
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
        let had_focus = self.flow.focus_child().is_some();
        self.thumbs.clear_queue();
        self.clear_cards();
        self.title.set_text(&format!("Group {} of {} · {} images", gi + 1, n, files.len()));
        let mut note = String::new();
        if reference_gone {
            note.push_str("The image the others in this group were matched against has been moved to the Trash. The images left were not compared with each other. ");
        }
        if problems {
            note.push_str("The scan had problems with some files; see the scan log.");
        }
        self.note.set_text(note.trim());
        self.note.set_visible(!note.is_empty());

        let scale = self.app.window.scale_factor().max(1) as u32;
        let mut cards = Vec::with_capacity(files.len());
        for (i, m) in files.iter().enumerate() {
            let card = self.card(i, m);
            self.thumbs.request(&m.path, CARD as u32 * scale, Shown::Picture(card.2.clone()), false);
            self.flow.append(&card.0);
            cards.push(Card { path: m.path.clone(), child: card.0, check: card.1 });
        }
        *self.cards.borrow_mut() = cards;
        self.pick(0);
        // The next group, ahead of being asked for.
        let ahead: Vec<PathBuf> = {
            let s = self.state.borrow();
            s.groups.get(gi + 1).map(|g| g.files.iter().take(AHEAD).map(|f| f.path.clone()).collect()).unwrap_or_default()
        };
        for p in ahead {
            self.thumbs.request(&p, CARD as u32 * scale, Shown::Callback(Box::new(|_| {})), false);
        }
        if had_focus {
            self.focus();
        }
    }

    /// One image of the shown group.
    fn card(self: &Rc<Self>, i: usize, m: &Member) -> (gtk::FlowBoxChild, gtk::CheckButton, gtk::Picture) {
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .can_shrink(true)
            .width_request(CARD)
            .height_request(CARD * 3 / 4)
            .alternative_text(file_name(&m.path))
            .build();
        let name = gtk::Label::builder()
            .label(file_name(&m.path))
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .max_width_chars(24)
            .build();
        name.add_css_class("heading");
        let dir = m.path.parent().map(|p| p.display().to_string()).unwrap_or_default();
        let dir = gtk::Label::builder().label(&dir).ellipsize(gtk::pango::EllipsizeMode::Start).max_width_chars(28).css_classes(["dim-label", "caption"]).build();
        let facts = gtk::Label::builder().label(facts(m)).css_classes(["caption"]).build();
        let check = gtk::CheckButton::with_label("Move to Trash");
        check.set_halign(gtk::Align::Center);
        check.set_focusable(false);
        let marked = self.state.borrow().marked.contains(&m.path);
        check.set_active(marked);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
        body.append(&picture);
        body.append(&name);
        body.append(&dir);
        body.append(&facts);
        body.append(&check);
        let child = gtk::FlowBoxChild::new();
        child.set_child(Some(&body));
        child.add_css_class("card");
        if marked {
            child.add_css_class("marked");
        }
        child.set_tooltip_text(Some(&m.path.display().to_string()));
        child.update_property(&[gtk::accessible::Property::Label(&file_name(&m.path))]);

        // Reached by the keyboard, or clicked anywhere on it, including its
        // check box: either way it is the image the buttons now mean.
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter({
            let weak = Rc::downgrade(self);
            move |_| {
                if let Some(me) = weak.upgrade() {
                    me.pick(i);
                }
            }
        });
        child.add_controller(focus);
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        click.connect_pressed({
            let weak = Rc::downgrade(self);
            move |_, _, _, _| {
                if let Some(me) = weak.upgrade() {
                    me.pick(i);
                }
            }
        });
        child.add_controller(click);

        check.connect_toggled({
            let weak = Rc::downgrade(self);
            move |c| {
                let Some(me) = weak.upgrade() else { return };
                if !me.quiet.get() {
                    me.set_marked(i, c.is_active());
                }
            }
        });
        (child, check, picture)
    }

    /// Make card `i` the one the buttons act on, and show it.
    fn pick(&self, i: usize) {
        let cards = self.cards.borrow();
        if let Some(c) = cards.get(self.picked.get()) {
            c.child.remove_css_class("picked");
        }
        if let Some(c) = cards.get(i) {
            c.child.add_css_class("picked");
            self.picked.set(i);
        }
    }

    fn focused_card(&self) -> Option<usize> {
        let focus = self.flow.focus_child()?;
        let child = focus.downcast_ref::<gtk::FlowBoxChild>()?;
        Some(child.index() as usize)
    }

    /// Cards on a line of the grid, as laid out now.
    fn columns(&self) -> i64 {
        let Some(first) = self.flow.child_at_index(0) else { return 1 };
        let y = first.allocation().y();
        let mut n = 1;
        while let Some(c) = self.flow.child_at_index(n) {
            if c.allocation().y() != y {
                break;
            }
            n += 1;
        }
        n as i64
    }

    /// The keyboard to card `i`, clamped to the group.
    fn move_to(&self, i: i64) {
        let n = self.cards.borrow().len() as i64;
        if n == 0 {
            return;
        }
        if let Some(c) = self.flow.child_at_index(i.clamp(0, n - 1) as i32) {
            c.grab_focus();
        }
    }

    fn toggle(self: &Rc<Self>, i: usize) {
        let Some(path) = self.cards.borrow().get(i).map(|c| c.path.clone()) else { return };
        let now = !self.state.borrow().marked.contains(&path);
        self.set_marked(i, now);
    }

    /// Mark or unmark card `i`'s file, everywhere it is shown.
    fn set_marked(self: &Rc<Self>, i: usize, on: bool) {
        let Some(path) = self.cards.borrow().get(i).map(|c| c.path.clone()) else { return };
        self.mark_path(&path, on);
        self.update_status();
    }

    fn mark_path(&self, path: &Path, on: bool) {
        let groups = {
            let mut s = self.state.borrow_mut();
            if on {
                s.marked.insert(path.to_path_buf());
            } else {
                s.marked.remove(path);
            }
            s.member_of.get(path).cloned().unwrap_or_default()
        };
        self.quiet.set(true);
        for c in self.cards.borrow().iter().filter(|c| c.path == path) {
            c.check.set_active(on);
            if on {
                c.child.add_css_class("marked");
            } else {
                c.child.remove_css_class("marked");
            }
        }
        self.quiet.set(false);
        for gi in groups {
            self.update_row(gi);
        }
    }

    fn mark_all_but_focused(self: &Rc<Self>) {
        let keep = self.picked.get();
        let paths: Vec<PathBuf> = self.cards.borrow().iter().map(|c| c.path.clone()).collect();
        for (i, p) in paths.iter().enumerate() {
            self.mark_path(p, i != keep);
        }
        self.update_status();
        // Back to the image that was kept, so the keyboard carries on there.
        self.move_to(keep as i64);
    }

    fn unmark_group(self: &Rc<Self>) {
        let paths: Vec<PathBuf> = self.cards.borrow().iter().map(|c| c.path.clone()).collect();
        for p in &paths {
            self.mark_path(p, false);
        }
        self.update_status();
    }

    fn step(self: &Rc<Self>, by: i32) {
        let (cur, n) = {
            let s = self.state.borrow();
            (s.current as i32, s.groups.len() as i32)
        };
        if n == 0 {
            return;
        }
        let next = (cur + by).clamp(0, n - 1);
        if let Some(row) = self.list.row_at_index(next) {
            self.list.select_row(Some(&row));
            // The list scrolls to it; the keyboard goes to the images.
            row.grab_focus();
            self.focus();
        }
    }

    fn update_status(&self) {
        let s = self.state.borrow();
        let n = s.marked.len();
        let bytes: u64 = s.marked.iter().filter_map(|p| s.sizes.get(p)).sum();
        let groups = s.groups.len();
        let text = if n == 0 {
            format!("{groups} group{}. Nothing marked.", if groups == 1 { "" } else { "s" })
        } else {
            format!("{n} image{} marked, {}.", if n == 1 { "" } else { "s" }, size(bytes))
        };
        self.status.set_text(&text);
        self.trash.set_sensitive(n > 0);
    }

    /// Open the picked image, or its folder.
    fn launch(self: &Rc<Self>, folder: bool) {
        let i = self.picked.get();
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
        let show = {
            let (me, picture, info, mark, win, at, quiet) = (Rc::downgrade(self), picture.clone(), info.clone(), mark.clone(), win.clone(), at.clone(), quiet.clone());
            Rc::new(move || {
                let Some(me) = me.upgrade() else { return };
                let cards = me.cards.borrow();
                let n = cards.len();
                let Some(card) = cards.get(at.get()) else { return };
                let path = card.path.clone();
                let s = me.state.borrow();
                let member = s.groups.get(s.current).and_then(|g| g.files.iter().find(|f| f.path == path)).cloned();
                let marked = s.marked.contains(&path);
                drop(s);
                drop(cards);
                win.set_title(Some(&format!("{} ({} of {n})", file_name(&path), at.get() + 1)));
                let text = match &member {
                    Some(m) => format!("{}  ·  {}", path.display(), facts(m)),
                    None => path.display().to_string(),
                };
                info.set_text(&text);
                info.set_tooltip_text(Some(&text));
                quiet.set(true);
                mark.set_active(marked);
                quiet.set(false);
                picture.set_paintable(None::<&gdk::Paintable>);
                let scale = win.scale_factor().max(1) as u32;
                me.thumbs.request(&path, LARGE * scale, Shown::Picture(picture.clone()), true);
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
                    if let Some(c) = me.flow.child_at_index(at.get() as i32) {
                        c.grab_focus();
                    }
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
        let (paths, bytes, whole) = {
            let s = self.state.borrow();
            let mut paths: Vec<PathBuf> = s.marked.iter().cloned().collect();
            paths.sort();
            let bytes: u64 = paths.iter().filter_map(|p| s.sizes.get(p)).sum();
            let whole = s.groups.iter().filter(|g| g.files.iter().all(|f| s.marked.contains(&f.path))).count();
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
        if whole > 0 {
            detail.push_str(&format!(
                "\n\nIn {whole} group{} every image is marked, so no copy of {} would be left.",
                if whole == 1 { "" } else { "s" },
                if whole == 1 { "that picture" } else { "those pictures" }
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
        self.trash.set_sensitive(false);
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
        for p in &gone {
            self.thumbs.forget(p);
            self.app.log_line(&format!("moved to the Trash: {}", p.display()));
        }
        let select = {
            let mut s = self.state.borrow_mut();
            for p in &gone {
                s.marked.remove(p);
                s.sizes.remove(p);
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
        self.thumbs.clear_queue();
        self.rebuild(select);
        self.focus();
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

/// Dimensions and size, as far as the report knows them.
fn facts(m: &Member) -> String {
    let mut parts = Vec::new();
    if let (Some(w), Some(h)) = (m.width, m.height) {
        parts.push(format!("{w} × {h}"));
    }
    if let Some(b) = m.size_bytes {
        parts.push(size(b));
    }
    parts.join(" · ")
}

