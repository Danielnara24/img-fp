//! The first page: what to scan, how, and the scan's progress.
//!
//! Two tabs. General holds what most scans change — the folders to scan and
//! to leave out, the work size, the pixel correlation and the mirrored look.
//! Advanced holds the rest of img-fp's command line: threads, candidates per
//! image, the aligned points and frame overlap, which files a walk takes, a
//! report file, the cache, and a log file. The progress bar, the log and
//! Scan/Cancel sit below both.

use crate::labels as l;
use crate::last;
use crate::results::{self, Prepared, Results};
use crate::scan::{self, Event, Scan};
use crate::options::{self as o, Kind, Opt, Value};
use crate::settings::Settings;
use crate::App;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How long a cancelled scan has to answer before it is killed. img-fp answers
/// Ctrl-C in well under a tenth of a second; this is for a disk that has
/// stopped answering it.
const KILL_AFTER: Duration = Duration::from_secs(3);

/// A list of folders with Add and Remove, for the folders to scan and the
/// ones to leave out.
struct PathList {
    root: gtk::Box,
    list: gtk::ListBox,
    paths: RefCell<Vec<PathBuf>>,
    add: gtk::Button,
}

impl PathList {
    fn new(window: &gtk::ApplicationWindow, add_label: &str, remove_label: &str, empty: &str, height: i32) -> Rc<PathList> {
        let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::Browse).build();
        list.add_css_class("boxed-list");
        list.set_placeholder(Some(&gtk::Label::builder().label(empty).css_classes(["dim-label"]).margin_top(12).margin_bottom(12).build()));
        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .min_content_height(height)
            .hexpand(true)
            .has_frame(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let add = gtk::Button::with_mnemonic(add_label);
        let remove = gtk::Button::with_mnemonic(remove_label);
        let buttons = gtk::Box::new(gtk::Orientation::Vertical, 6);
        buttons.append(&add);
        buttons.append(&remove);
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.append(&scroll);
        root.append(&buttons);
        let me = Rc::new(PathList { root, list, paths: RefCell::new(Vec::new()), add: add.clone() });

        let window = window.downgrade();
        add.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                let (Some(me), Some(window)) = (me.upgrade(), window.upgrade()) else { return };
                glib::spawn_future_local(async move {
                    let dialog = gtk::FileDialog::builder().title("Choose folders").modal(true).build();
                    if let Ok(files) = dialog.select_multiple_folders_future(Some(&window)).await {
                        let paths = files.iter::<gio::File>().filter_map(|f| f.ok()?.path());
                        me.add_paths(paths);
                    }
                });
            }
        });
        remove.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.remove_selected();
                }
            }
        });
        // Delete removes the selected folder, as it would in a file manager.
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed({
            let me = Rc::downgrade(&me);
            move |_, key, _, _| {
                if matches!(key, gdk::Key::Delete | gdk::Key::KP_Delete | gdk::Key::BackSpace) {
                    if let Some(me) = me.upgrade() {
                        me.remove_selected();
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        });
        me.list.add_controller(keys);
        // Folders dropped from a file manager.
        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        drop.connect_drop({
            let me = Rc::downgrade(&me);
            move |_, value, _, _| {
                let (Some(me), Ok(files)) = (me.upgrade(), value.get::<gdk::FileList>()) else { return false };
                me.add_paths(files.files().iter().filter_map(|f| f.path()));
                true
            }
        });
        me.root.add_controller(drop);
        me
    }

    fn add_paths(&self, paths: impl Iterator<Item = PathBuf>) {
        let mut v = self.paths.borrow().clone();
        for p in paths {
            if !v.contains(&p) {
                v.push(p);
            }
        }
        self.set(v);
    }

    fn set(&self, v: Vec<PathBuf>) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        for p in &v {
            let label = gtk::Label::builder()
                .label(p.display().to_string())
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::Middle)
                .tooltip_text(p.display().to_string())
                .margin_start(8)
                .margin_end(8)
                .margin_top(4)
                .margin_bottom(4)
                .build();
            self.list.append(&label);
        }
        *self.paths.borrow_mut() = v;
    }

    fn remove_selected(&self) {
        let Some(row) = self.list.selected_row() else { return };
        let i = row.index() as usize;
        let mut v = self.paths.borrow().clone();
        if i < v.len() {
            v.remove(i);
        }
        self.set(v);
        // Keep the keyboard where it was: on the next row, or on Add.
        let n = self.paths.borrow().len();
        if n == 0 {
            self.add.grab_focus();
        } else if let Some(r) = self.list.row_at_index(i.min(n - 1) as i32) {
            self.list.select_row(Some(&r));
            r.grab_focus();
        }
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.paths.borrow().clone()
    }
}

/// A label on the left of a grid, whose mnemonic reaches `target`.
fn label_for(text: &str, target: &impl IsA<gtk::Widget>) -> gtk::Label {
    let label = gtk::Label::with_mnemonic(text);
    label.set_mnemonic_widget(Some(target));
    label.set_xalign(0.0);
    label
}

fn hint(text: &str) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).wrap(true).css_classes(["dim-label", "caption"]).build()
}

fn spin(min: f64, max: f64, step: f64, digits: u32) -> gtk::SpinButton {
    let s = gtk::SpinButton::with_range(min, max, step);
    s.set_digits(digits);
    s.set_numeric(true);
    // Enter starts the scan, as it does in an entry. (GTK 4.14 has a
    // property for this; the window asks for 4.10.)
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(|c, key, _, mods| {
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) && !mods.contains(gdk::ModifierType::CONTROL_MASK) {
            if let Some(s) = c.widget().and_downcast::<gtk::SpinButton>() {
                s.update();
                let _ = s.activate_action("default.activate", None);
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    s.add_controller(keys);
    s.set_valign(gtk::Align::Center);
    s
}

fn entry() -> gtk::Entry {
    gtk::Entry::builder().hexpand(true).activates_default(true).build()
}

fn grid() -> gtk::Grid {
    gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build()
}

fn section(text: &str) -> gtk::Label {
    let l = gtk::Label::builder().label(text).xalign(0.0).margin_top(8).build();
    l.add_css_class("heading");
    l
}

/// The label on the left of an option's row, with its tooltip.
fn label_of(opt: &Opt, target: &gtk::Widget) -> gtk::Label {
    let label = label_for(opt.label, target);
    label.set_tooltip_text(opt.tip);
    label
}

/// What is written beside or under an option: its hint, or else its tooltip.
fn hint_of(opt: &Opt) -> &'static str {
    opt.hint.or(opt.tip).unwrap_or("")
}

/// An option's widget, made from its entry in `options::ALL`.
enum Control {
    Check(gtk::CheckButton),
    Spin(gtk::SpinButton),
    Entry(gtk::Entry),
    Choice(gtk::DropDown),
}

impl Control {
    fn of(opt: &Opt) -> Control {
        let c = match opt.kind {
            Kind::Check { .. } => Control::Check(gtk::CheckButton::with_mnemonic(opt.label)),
            Kind::Number { min, max, step, digits } => Control::Spin(spin(min, max, step, digits)),
            Kind::Text => Control::Entry(entry()),
            Kind::Choice(cs) => Control::Choice(gtk::DropDown::from_strings(&cs.iter().map(|c| c.2).collect::<Vec<_>>())),
        };
        c.widget().set_tooltip_text(opt.tip);
        c
    }

    fn widget(&self) -> gtk::Widget {
        match self {
            Control::Check(c) => c.clone().upcast(),
            Control::Spin(s) => s.clone().upcast(),
            Control::Entry(e) => e.clone().upcast(),
            Control::Choice(d) => d.clone().upcast(),
        }
    }

    fn show(&self, opt: &Opt, v: &Value) {
        match (self, &opt.kind) {
            (Control::Check(c), _) => c.set_active(v.on()),
            (Control::Spin(s), _) => s.set_value(v.number()),
            (Control::Entry(e), _) => e.set_text(v.text()),
            (Control::Choice(d), Kind::Choice(cs)) => d.set_selected(cs.iter().position(|c| c.0 == v.text()).unwrap_or(0) as u32),
            (Control::Choice(_), _) => unreachable!("a drop-down is made for a choice"),
        }
    }

    fn read(&self, opt: &Opt) -> Value {
        match (self, &opt.kind) {
            (Control::Check(c), _) => Value::On(c.is_active()),
            (Control::Spin(s), _) => {
                // A value typed but not yet committed with Enter or Tab is
                // still the value meant.
                s.update();
                opt.number(s.value())
            }
            (Control::Entry(e), _) => Value::Text(e.text().trim().to_string()),
            (Control::Choice(d), Kind::Choice(cs)) => Value::Text(cs[(d.selected() as usize).min(cs.len() - 1)].0.to_string()),
            (Control::Choice(_), _) => unreachable!("a drop-down is made for a choice"),
        }
    }
}

pub struct Setup {
    pub root: gtk::Box,
    app: Rc<App>,
    results: Rc<Results>,

    folders: Rc<PathList>,
    exclude: Rc<PathList>,
    report_path: gtk::Entry,
    cache_path: gtk::Entry,
    log_path: gtk::Entry,
    /// One per option, in `options::ALL`'s order.
    options: Vec<Control>,

    notebook: gtk::Notebook,
    scan_button: gtk::Button,
    cancel_button: gtk::Button,
    back_button: gtk::Button,
    defaults_button: gtk::Button,
    progress: gtk::ProgressBar,
    status: gtk::Label,
    elapsed: gtk::Label,

    scan: RefCell<Option<Scan>>,
    started: Cell<Option<Instant>>,
    cancelling: Cell<bool>,
}

impl Setup {
    pub fn new(app: Rc<App>, settings: Settings, results: Rc<Results>) -> Rc<Setup> {
        let window = &app.window;

        let options: Vec<Control> = o::ALL.iter().map(|opt| Control::of(opt)).collect();
        let w = |opt: &Opt| options[opt.at()].widget();

        // ---- General
        let folders = PathList::new(window, l::ADD_FOLDER, l::REMOVE_FOLDER, "No folders yet. Add one, or drop it here.", 130);
        let exclude = PathList::new(window, l::ADD_EXCLUDE, l::REMOVE_EXCLUDE, "Nothing left out.", 90);

        let g = grid();
        let folders_label = label_for(l::FOLDERS, &folders.list);
        folders_label.set_valign(gtk::Align::Start);
        g.attach(&folders_label, 0, 0, 1, 1);
        g.attach(&folders.root, 1, 0, 2, 1);
        let walk_box = gtk::Box::new(gtk::Orientation::Horizontal, 18);
        walk_box.append(&w(&o::RECURSIVE));
        walk_box.append(&w(&o::HIDDEN));
        g.attach(&walk_box, 1, 1, 2, 1);
        let exclude_label = label_for(l::EXCLUDE, &exclude.list);
        exclude_label.set_valign(gtk::Align::Start);
        g.attach(&exclude_label, 0, 2, 1, 1);
        g.attach(&exclude.root, 1, 2, 2, 1);

        g.attach(&section("What counts as a duplicate"), 0, 3, 3, 1);
        for (r, opt) in [(4, &o::WORK_SIZE), (5, &o::MIN_CORRELATION)] {
            g.attach(&label_of(opt, &w(opt)), 0, r, 1, 1);
            g.attach(&w(opt), 1, r, 1, 1);
            g.attach(&hint(hint_of(opt)), 2, r, 1, 1);
        }
        g.attach(&w(&o::MIRRORED), 1, 6, 2, 1);
        g.attach(&hint(hint_of(&o::MIRRORED)), 1, 7, 2, 1);
        let general = gtk::ScrolledWindow::builder().child(&g).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();

        // ---- Advanced
        let report_path = entry();
        report_path.set_placeholder_text(Some("results.txt, results.csv or results.json"));
        let report_choose = gtk::Button::with_mnemonic(l::REPORT_CHOOSE);
        let cache_path = entry();
        cache_path.set_placeholder_text(Some(&format!("the default: ~/.cache/img-fp/{}", img_fp::cache_file_name())));
        let cache_choose = gtk::Button::with_mnemonic(l::CACHE_CHOOSE);
        let log_path = entry();
        let log_choose = gtk::Button::with_mnemonic(l::LOG_CHOOSE);

        let a = grid();
        for (r, opt) in [(0, &o::THREADS), (1, &o::CANDIDATES), (2, &o::MIN_POINTS), (3, &o::MIN_OVERLAP)] {
            a.attach(&label_of(opt, &w(opt)), 0, r, 1, 1);
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.append(&w(opt));
            let h = hint(hint_of(opt));
            h.set_hexpand(true);
            row.append(&h);
            a.attach(&row, 1, r, 2, 1);
        }
        a.attach(&section("Which files"), 0, 4, 3, 1);
        a.attach(&label_of(&o::EXTENSIONS, &w(&o::EXTENSIONS)), 0, 5, 1, 1);
        a.attach(&w(&o::EXTENSIONS), 1, 5, 2, 1);
        a.attach(&hint(hint_of(&o::EXTENSIONS)), 1, 6, 2, 1);
        a.attach(&w(&o::SYMLINKS), 1, 7, 2, 1);
        a.attach(&section("Report"), 0, 8, 3, 1);
        a.attach(&w(&o::REPORT), 0, 9, 1, 1);
        let report_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        report_box.append(&report_path);
        report_box.append(&report_choose);
        a.attach(&report_box, 1, 9, 2, 1);
        a.attach(&label_of(&o::REPORT_FORMAT, &w(&o::REPORT_FORMAT)), 0, 10, 1, 1);
        w(&o::REPORT_FORMAT).set_halign(gtk::Align::Start);
        a.attach(&w(&o::REPORT_FORMAT), 1, 10, 2, 1);
        a.attach(&section("Cache"), 0, 11, 3, 1);
        a.attach(&w(&o::USE_CACHE), 0, 12, 3, 1);
        a.attach(&hint(hint_of(&o::USE_CACHE)), 0, 13, 3, 1);
        a.attach(&label_for(l::CACHE_FILE, &cache_path), 0, 14, 1, 1);
        let cache_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        cache_box.append(&cache_path);
        cache_box.append(&cache_choose);
        a.attach(&cache_box, 1, 14, 2, 1);
        a.attach(&w(&o::CLEAR_CACHE), 1, 15, 2, 1);
        a.attach(&w(&o::PRUNE_CACHE), 1, 16, 2, 1);
        a.attach(&hint("Both apply to the next scan only."), 1, 17, 2, 1);
        a.attach(&section("Log"), 0, 18, 3, 1);
        a.attach(&w(&o::LOG), 0, 19, 1, 1);
        let log_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        log_box.append(&log_path);
        log_box.append(&log_choose);
        a.attach(&log_box, 1, 19, 2, 1);
        a.attach(&hint(hint_of(&o::LOG)), 1, 20, 2, 1);
        let version = hint(concat!("img-fp ", env!("CARGO_PKG_VERSION")));
        version.set_margin_top(18);
        a.attach(&version, 0, 21, 3, 1);
        let advanced = gtk::ScrolledWindow::builder().child(&a).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();

        let notebook = gtk::Notebook::new();
        notebook.append_page(&general, Some(&gtk::Label::with_mnemonic(l::TAB_GENERAL)));
        notebook.append_page(&advanced, Some(&gtk::Label::with_mnemonic(l::TAB_ADVANCED)));
        notebook.set_vexpand(true);

        // ---- The bar at the bottom
        let progress = gtk::ProgressBar::builder().hexpand(true).show_text(true).text("").valign(gtk::Align::Center).build();
        let status = gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).hexpand(true).build();
        let elapsed = gtk::Label::builder().css_classes(["dim-label", "numeric"]).build();
        let scan_button = gtk::Button::with_mnemonic(l::SCAN);
        scan_button.add_css_class("suggested-action");
        scan_button.set_tooltip_text(Some("Start the scan (Ctrl+Enter)"));
        let cancel_button = gtk::Button::with_mnemonic(l::CANCEL);
        cancel_button.add_css_class("destructive-action");
        cancel_button.set_tooltip_text(Some("Stop now (Escape). Images analysed so far are kept in the cache."));
        cancel_button.set_visible(false);
        let back_button = gtk::Button::with_mnemonic(l::BACK_TO_RESULTS);
        back_button.set_visible(false);
        let defaults_button = gtk::Button::with_mnemonic(l::DEFAULTS);
        defaults_button.set_tooltip_text(Some("Every option back to its default. The folders are kept."));
        let log_button = gtk::Button::with_mnemonic(l::SCAN_LOG);

        let bottom = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_start(16)
            .margin_end(16)
            .margin_top(8)
            .margin_bottom(12)
            .build();
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.append(&progress);
        bar.append(&elapsed);
        bottom.append(&bar);
        bottom.append(&status);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.append(&defaults_button);
        buttons.append(&log_button);
        let spacer = gtk::Box::builder().hexpand(true).build();
        buttons.append(&spacer);
        buttons.append(&gtk::Label::builder().label("F1 keyboard shortcuts").css_classes(["dim-label", "caption"]).build());
        buttons.append(&back_button);
        buttons.append(&cancel_button);
        buttons.append(&scan_button);
        bottom.append(&buttons);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&notebook);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&bottom);
        window.set_default_widget(Some(&scan_button));

        let me = Rc::new(Setup {
            root,
            app: app.clone(),
            results,
            folders,
            exclude,
            report_path,
            cache_path,
            log_path,
            options,
            notebook,
            scan_button,
            cancel_button,
            back_button,
            defaults_button,
            progress,
            status,
            elapsed,
            scan: RefCell::new(None),
            started: Cell::new(None),
            cancelling: Cell::new(false),
        });
        me.show(&settings);
        me.idle(if me.folders.paths().is_empty() { "Add the folders to scan, then press Scan." } else { "Press Scan to start." });

        // What depends on what.
        for (check, deps) in [
            (me.check(&o::REPORT), vec![me.report_path.clone().upcast::<gtk::Widget>(), report_choose.clone().upcast(), me.control(&o::REPORT_FORMAT).widget()]),
            (me.check(&o::USE_CACHE), vec![me.cache_path.clone().upcast(), cache_choose.clone().upcast(), me.control(&o::PRUNE_CACHE).widget()]),
            (me.check(&o::LOG), vec![me.log_path.clone().upcast(), log_choose.clone().upcast()]),
        ] {
            let sync = move |c: &gtk::CheckButton| {
                for d in &deps {
                    d.set_sensitive(c.is_active());
                }
            };
            sync(check);
            check.connect_toggled(sync);
        }
        report_choose.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.choose_file(me.report_path.clone(), "Save the report as", "results.txt");
                }
            }
        });
        cache_choose.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.choose_file(me.cache_path.clone(), "Cache file", &img_fp::cache_file_name());
                }
            }
        });
        log_choose.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.choose_file(me.log_path.clone(), "Write the log to", "img-fp.log");
                }
            }
        });
        me.scan_button.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.start();
                }
            }
        });
        me.cancel_button.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.cancel();
                }
            }
        });
        me.back_button.connect_clicked({
            let app = app.clone();
            let results = me.results.clone();
            move |_| {
                app.show_results();
                results.focus();
            }
        });
        me.defaults_button.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.show(&me.read().with_defaults());
                }
            }
        });
        log_button.connect_clicked({
            let app = app.clone();
            move |_| app.show_log()
        });

        // Escape cancels a running scan, and Ctrl+Enter starts one from
        // anywhere on the page.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let me = Rc::downgrade(&me);
            move |_, key, _, mods| {
                let Some(me) = me.upgrade() else { return glib::Propagation::Proceed };
                if me.app.stack.visible_child_name().as_deref() != Some("setup") {
                    return glib::Propagation::Proceed;
                }
                if key == gdk::Key::Escape && me.scanning() {
                    me.cancel();
                    return glib::Propagation::Stop;
                }
                let enter = matches!(key, gdk::Key::Return | gdk::Key::KP_Enter);
                if enter && mods.contains(gdk::ModifierType::CONTROL_MASK) && !me.scanning() {
                    me.start();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        });
        app.window.add_controller(keys);
        me
    }

    /// Put the Scan button (or Cancel) under the keyboard.
    pub fn focus_scan(&self) {
        if self.scanning() {
            self.cancel_button.grab_focus();
        } else if self.folders.paths().is_empty() {
            self.folders.add.grab_focus();
        } else {
            self.scan_button.grab_focus();
        }
    }

    /// Whether "Back to results" is offered.
    pub fn results_available(&self, yes: bool) {
        self.back_button.set_visible(yes && !self.scanning());
    }

    fn scanning(&self) -> bool {
        self.scan.borrow().is_some()
    }

    /// Stop a running scan without waiting for it: the window is closing.
    pub fn stop_scan(&self) {
        if let Some(s) = self.scan.borrow().as_ref() {
            s.interrupt();
        }
    }

    fn show(&self, s: &Settings) {
        self.folders.set(s.folders.clone());
        self.exclude.set(s.exclude.clone());
        self.report_path.set_text(&s.report_path);
        self.cache_path.set_text(&s.cache_path);
        self.log_path.set_text(&s.log_path);
        for (opt, c) in o::ALL.iter().zip(&self.options) {
            c.show(opt, s.get(opt));
        }
    }

    fn read(&self) -> Settings {
        let mut s = Settings::default();
        s.folders = self.folders.paths();
        s.exclude = self.exclude.paths();
        s.report_path = self.report_path.text().to_string();
        s.cache_path = self.cache_path.text().to_string();
        s.log_path = self.log_path.text().to_string();
        for (opt, c) in o::ALL.iter().zip(&self.options) {
            s.set(opt, c.read(opt));
        }
        s
    }

    fn control(&self, opt: &Opt) -> &Control {
        &self.options[opt.at()]
    }

    /// The check button of a check option.
    fn check(&self, opt: &Opt) -> &gtk::CheckButton {
        match self.control(opt) {
            Control::Check(c) => c,
            _ => panic!("{} is not a check", opt.key),
        }
    }

    fn choose_file(self: &Rc<Self>, target: gtk::Entry, title: &str, name: &str) {
        let window = self.app.window.clone();
        let dialog = gtk::FileDialog::builder().title(title).modal(true).initial_name(name).build();
        let current = target.text();
        if !current.is_empty() {
            dialog.set_initial_file(Some(&gio::File::for_path(current.as_str())));
        }
        glib::spawn_future_local(async move {
            if let Ok(f) = dialog.save_future(Some(&window)).await {
                if let Some(p) = f.path() {
                    target.set_text(&p.display().to_string());
                }
            }
            target.grab_focus();
        });
    }

    fn alert(&self, message: &str, detail: &str) {
        let d = gtk::AlertDialog::builder().message(message).detail(detail).modal(true).build();
        d.show(Some(&self.app.window));
    }

    /// Nothing running: the options can be changed and Scan pressed.
    fn idle(&self, status: &str) {
        self.scan.replace(None);
        self.cancelling.set(false);
        self.started.set(None);
        self.notebook.set_sensitive(true);
        self.defaults_button.set_sensitive(true);
        self.scan_button.set_visible(true);
        self.cancel_button.set_visible(false);
        self.cancel_button.set_sensitive(true);
        self.back_button.set_visible(self.results.has_results());
        self.status.set_text(status);
        let refocus = self.cancel_button.has_focus() || GtkWindowExt::focus(&self.app.window).is_none();
        if refocus {
            self.scan_button.grab_focus();
        }
    }

    fn start(self: &Rc<Self>) {
        if self.scanning() {
            return;
        }
        let settings = self.read().with_absolute_paths();
        // Shown where they were typed, so the path a file is written to is the
        // path on screen.
        self.report_path.set_text(&settings.report_path);
        self.log_path.set_text(&settings.log_path);
        self.cache_path.set_text(&settings.cache_path);
        if settings.folders.is_empty() {
            self.alert("Nothing to scan", "Add at least one folder to scan.");
            self.folders.add.grab_focus();
            return;
        }
        let argv = settings.argv();
        if let Err(e) = img_fp::check_args(argv.iter().cloned()) {
            let e = e.trim_start_matches("error: ").lines().next().unwrap_or("").to_string();
            self.alert("These options cannot be used together", &e);
            return;
        }
        settings.save();
        // Before the scan can read anything: a file changed since is not the
        // file it judged (`last::Notes::changed_after`).
        let started = img_fp::last::now_nanos();
        let (scan, events) = match Scan::start(&argv) {
            Ok(x) => x,
            Err(e) => {
                self.alert("Could not start the scan", &e.to_string());
                return;
            }
        };
        self.app.log.set_text("");
        self.app.log_line(concat!("img-fp ", env!("CARGO_PKG_VERSION")));
        let shown: Vec<String> = argv.iter().skip(1).map(|a| shell_word(&a.to_string_lossy())).collect();
        self.app.log_line(&format!("$ img-fp {}", shown.join(" ")));
        self.scan.replace(Some(scan));
        self.started.set(Some(Instant::now()));
        self.notebook.set_sensitive(false);
        self.defaults_button.set_sensitive(false);
        self.back_button.set_visible(false);
        self.scan_button.set_visible(false);
        self.cancel_button.set_visible(true);
        self.cancel_button.grab_focus();
        self.progress.set_fraction(0.0);
        self.progress.set_text(Some("0%"));
        self.status.set_text("Starting…");
        self.elapsed.set_text("0:00");

        // The clock beside the bar, twice a second while the scan runs.
        glib::timeout_add_local(Duration::from_millis(500), {
            let me = Rc::downgrade(self);
            move || {
                let Some(me) = me.upgrade() else { return glib::ControlFlow::Break };
                let Some(t) = me.started.get() else { return glib::ControlFlow::Break };
                let s = t.elapsed().as_secs();
                me.elapsed.set_text(&format!("{}:{:02}", s / 60, s % 60));
                glib::ControlFlow::Continue
            }
        });

        // The folders scanned, as the report spells its paths: the tree view
        // starts from them.
        let roots = settings.folders.clone();
        // Kept for `--last` and the next window as the command line keeps a
        // scan: not one run without the cache, and not one that found
        // nothing (see `img_fp::last`). Kept regardless, a scan of an empty
        // folder replaced the last one worth having, and its marks with it.
        let keep = settings.on(&o::USE_CACHE);
        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let mut stderr: Vec<String> = Vec::new();
            while let Ok(ev) = events.recv().await {
                let Some(me) = me.upgrade() else { return };
                match ev {
                    Event::Progress(p, m) => {
                        if !me.cancelling.get() {
                            me.progress.set_fraction(p.clamp(0.0, 1.0));
                            me.progress.set_text(Some(&format!("{:.0}%", p * 100.0)));
                            me.status.set_text(&capitalise(&m));
                        }
                    }
                    Event::Log(line) => me.app.log_line(&line),
                    Event::Stderr(line) => {
                        me.app.log_line(&line);
                        stderr.push(line);
                    }
                    Event::Exited { code, signal } => {
                        // The result is read on a thread of its own: on a large
                        // library it is megabytes of JSON, and parsing it here
                        // froze the window.
                        let result = me.scan.borrow().as_ref().map(|s| s.result.clone());
                        // Exit 1 with a result is a scan that finished and
                        // then failed at the end — an `-o` that could not be
                        // written — and the window's copy is written before
                        // that, so that the scan can still be shown.
                        let found = match (code, result) {
                            (Some(0) | Some(scan::EXIT_PROBLEMS), Some(path)) | (Some(scan::EXIT_FATAL), Some(path))
                                if code != Some(scan::EXIT_FATAL) || path.exists() =>
                            {
                                me.status.set_text("Reading the results…");
                                // Kept for the next window, once it has been
                                // read: a report that will not read is not
                                // worth restoring.
                                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                                let notes = last::Notes {
                                    problems: code != Some(0),
                                    took: me.took(),
                                    finished: now.as_secs(),
                                    started_ns: started,
                                    id: now.as_nanos() as u64,
                                    log: me.app.log.text(&me.app.log.start_iter(), &me.app.log.end_iter(), false).to_string(),
                                    roots: roots.clone(),
                                };
                                // Made ready for the page here too, off the
                                // main thread (`results::prepare`).
                                let read = gio::spawn_blocking(move || {
                                    let found = scan::read_result(&path);
                                    let keep = keep && found.as_ref().is_ok_and(|f| f.head.files_enumerated > 0);
                                    let mut prepared = found.map(results::prepare);
                                    if let Ok(p) = prepared.as_mut() {
                                        p.set_roots(notes.roots.clone());
                                        p.set_changed_after(notes.changed_after());
                                        if keep && path.exists() && last::save(&path, &notes) {
                                            p.set_kept(notes.id);
                                        }
                                    }
                                    prepared
                                })
                                .await;
                                Some(read.unwrap_or_else(|_| Err("reading the scan's results failed".into())))
                            }
                            _ => None,
                        };
                        me.finished(code, signal, &stderr, found);
                        return;
                    }
                }
            }
        });
    }

    fn cancel(self: &Rc<Self>) {
        let scan = self.scan.borrow();
        let Some(s) = scan.as_ref() else { return };
        if self.cancelling.replace(true) {
            return;
        }
        s.interrupt();
        self.status.set_text("Cancelling…");
        self.cancel_button.set_sensitive(false);
        // Only this scan: by the time this fires, it has almost always ended
        // and another may have started.
        let id = s.id;
        glib::timeout_add_local_once(KILL_AFTER, {
            let me = Rc::downgrade(self);
            move || {
                if let Some(me) = me.upgrade() {
                    if let Some(s) = me.scan.borrow().as_ref().filter(|s| s.id == id) {
                        s.kill();
                    }
                }
            }
        });
    }

    /// The scan has ended. `found` is its report, read, when it ended with one.
    fn finished(self: &Rc<Self>, code: Option<i32>, signal: Option<i32>, stderr: &[String], found: Option<Result<Prepared, String>>) {
        let scan = self.scan.borrow_mut().take();
        let took = self.took();
        // The one-off cache requests have been carried out, or given up on.
        self.check(&o::CLEAR_CACHE).set_active(false);
        self.check(&o::PRUNE_CACHE).set_active(false);
        // The error a run that failed printed last, if it printed one.
        let error_line = || stderr.iter().rev().find(|l| l.starts_with("Error")).map(|l| l.trim_start_matches("Error:").trim().to_string());
        // A scan that finished, and then could not write what it was asked to
        // at the end: its results are shown, and what failed is said.
        if code == Some(scan::EXIT_FATAL) {
            if let Some(Ok(found)) = found {
                drop(scan);
                self.show_found(found, true, &took);
                self.status_after_failure(error_line());
                return;
            }
        }
        match code {
            Some(0) | Some(scan::EXIT_PROBLEMS) => {
                let found = found.unwrap_or(Err("the scan left no results".into()));
                drop(scan);
                match found {
                    Ok(found) => self.show_found(found, code == Some(scan::EXIT_PROBLEMS), &took),
                    Err(e) => {
                        self.idle("The scan finished, but its results could not be read.");
                        self.alert("Could not read the results", &e);
                    }
                }
            }
            Some(scan::EXIT_INTERRUPTED) => {
                drop(scan);
                let kept = stderr.iter().rev().find(|l| l.starts_with("Interrupted")).cloned().unwrap_or_default();
                let kept = kept.trim_start_matches("Interrupted.").trim();
                self.progress.set_text(Some("Cancelled"));
                self.idle(format!("Cancelled after {took}. {kept}").trim());
            }
            _ => {
                drop(scan);
                self.progress.set_text(Some("Failed"));
                // Ended by the window only if the window asked. The kernel's
                // out-of-memory killer sends the same signal, and a scan it
                // ended was reported as one the user had cancelled.
                if signal == Some(9) && self.cancelling.get() {
                    self.idle("Stopped: the scan did not answer the cancel, and was ended.");
                    return;
                }
                let error = error_line()
                    .unwrap_or_else(|| match (code, signal) {
                        (_, Some(9)) => "The scan was killed (SIGKILL), most likely by the system \
                            running out of memory."
                            .to_string(),
                        (_, Some(s)) => format!("The scan was ended by signal {s}."),
                        (Some(c), _) => format!("The scan exited with code {c}."),
                        _ => "The scan ended unexpectedly.".to_string(),
                    });
                self.idle("The scan failed.");
                self.alert("The scan failed", &error);
            }
        }
    }
}

impl Setup {
    /// How long the running scan has taken, as the clock beside the bar says.
    fn took(&self) -> String {
        let secs = self.started.get().map_or(0, |t| t.elapsed().as_secs());
        format!("{}:{:02}", secs / 60, secs % 60)
    }

    /// The scan kept from the last window, read: on the results page as it
    /// was left, unless a scan has been started since this window opened. If
    /// the window opened on the results page to wait for it, they are shown
    /// there; otherwise they wait behind "Back to results". With none to
    /// show, a window waiting for them goes to the settings.
    pub fn restore(self: &Rc<Self>, kept: Option<last::Last>) {
        let waiting = self.app.stack.visible_child_name().as_deref() == Some("results");
        if self.scanning() || self.results.has_results() {
            return;
        }
        let Some(kept) = kept else {
            if waiting {
                self.app.show_setup();
                self.focus_scan();
            }
            return;
        };
        let last::Last { prepared: mut found, notes, gone, marks } = kept;
        found.set_kept(notes.id);
        found.set_changed_after(notes.changed_after());
        self.app.log.set_text(&notes.log);
        let when = glib::DateTime::from_unix_local(notes.finished as i64)
            .and_then(|t| t.format("%e %b %Y, %H:%M"))
            .map(|t| t.trim().to_string())
            .unwrap_or_default();
        let groups = found.groups();
        self.progress.set_fraction(1.0);
        self.progress.set_text(Some("100%"));
        self.elapsed.set_text(&notes.took);
        self.idle(&format!(
            "The last scan's results, from {when}: {groups} group{} among {} image{}, done in {}.{}",
            if groups == 1 { "" } else { "s" },
            found.analysed(),
            if found.analysed() == 1 { "" } else { "s" },
            notes.took,
            if notes.problems { " It had problems with some files; see the scan log." } else { "" }
        ));
        self.results.restore(found, notes.problems, &marks, &gone);
        self.back_button.set_visible(true);
        if waiting {
            self.results.focus();
        }
    }

    /// A finished scan's groups, on the results page.
    fn show_found(self: &Rc<Self>, found: Prepared, problems: bool, took: &str) {
        self.progress.set_fraction(1.0);
        self.progress.set_text(Some("100%"));
        let summary = format!(
            "Done in {took}: {} group{} among {} image{}.{}",
            found.groups(),
            if found.groups() == 1 { "" } else { "s" },
            found.analysed(),
            if found.analysed() == 1 { "" } else { "s" },
            if problems { " It had problems with some files; see the scan log." } else { "" }
        );
        self.idle(&summary);
        self.results.show(found, problems);
        self.app.show_results();
        self.results.focus();
    }

    /// Say what failed at the end of a scan whose results are shown anyway.
    fn status_after_failure(&self, error: Option<String>) {
        let detail = error.unwrap_or_else(|| "The scan exited with an error after finishing.".into());
        self.alert("The scan finished, but something failed at the end", &detail);
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A command-line word as a shell would need it written, for the log.
fn shell_word(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=,:@%+".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}
