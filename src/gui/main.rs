//! img-fp-gui: a window over img-fp.
//!
//! One binary, two roles. Started by a person it is the window; started by the
//! window with `--worker` it is the scan, which is `img-fp`'s own run in a
//! process of its own (`img_fp::worker_main`). A scan is a process rather than
//! a thread so that Cancel can be exactly what Ctrl-C is on the command line:
//! a signal the scan answers at once, keeping every analysis it has finished in
//! the cache, and after which everything the scan held is gone. See `scan.rs`.
//!
//! The window itself is plain GTK 4, with no libadwaita and no builder files:
//!
//! - `setup.rs`   what to scan and how, on two tabs, and the scan's progress
//! - `settings.rs` those choices as a value: the paths kept between runs, the
//!   options starting at img-fp's defaults, and all of it turned into an
//!   img-fp command line
//! - `scan.rs`    the child process and what it says
//! - `results.rs` the groups, for choosing what to move to the Trash
//! - `thumbs.rs`  pictures for the results, decoded off the main thread
//! - `labels.rs`  every mnemonic in the window, checked for clashes by a test
//!
//! Every control can be reached from the keyboard: Tab and the arrow keys move
//! focus, and every labelled control has an Alt mnemonic.

mod labels;
mod results;
mod scan;
mod settings;
mod setup;
mod thumbs;

use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const APP_ID: &str = "io.github.danielnara24.img-fp";

/// The window's own command line: folders to start with, and the two flags
/// every program answers.
///
/// Every argument used to be taken as a folder, so `img-fp-gui --help` opened
/// the window with a folder named `--help` in its list, and `--version` the
/// same. Parsed by clap, as `img-fp`'s is, they print and exit; anything else
/// starting with `-` is refused with clap's message, and `--` ends the flags
/// for a folder whose name starts with one.
#[derive(clap::Parser)]
#[command(
    name = "img-fp-gui",
    version,
    about = "Find duplicate and near-duplicate images, and choose which to move to the Trash.",
    long_about = None
)]
struct GuiArgs {
    /// Folders to scan, in place of the ones remembered from last time.
    #[arg(value_name = "FOLDER")]
    folders: Vec<PathBuf>,
}

fn main() -> glib::ExitCode {
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == scan::WORKER_FLAG) {
        // Before anything else, for the reason `check_cpu` gives.
        if let Err(e) = img_fp::check_cpu_for_window() {
            eprintln!("Error: {e:#}");
            return glib::ExitCode::FAILURE;
        }
        return worker(args.split_off(2));
    }
    // `--help` and `--version` answer on any CPU; nothing else has run yet.
    let folders = <GuiArgs as clap::Parser>::parse_from(&args).folders;
    if let Err(e) = img_fp::check_cpu_for_window() {
        eprintln!("Error: {e:#}");
        return glib::ExitCode::FAILURE;
    }
    // Drawn in software unless someone asks otherwise. The window is a form
    // and a grid of still pictures, which a GPU does not draw any better, and
    // the GL renderer brings Mesa's shader compiler with it: measured idle,
    // 100 MB of PSS against 73, most of the difference being libLLVM.
    // `GSK_RENDERER=gl` (or `ngl`, `vulkan`) still chooses.
    if std::env::var_os("GSK_RENDERER").is_none() {
        // SAFETY: nothing else is running yet; this is the first thing the
        // process does and no thread has been started.
        unsafe { std::env::set_var("GSK_RENDERER", "cairo") };
    }
    quiet_theme_errors();
    // The folders on the command line are the ones to start with, which is
    // what a file manager's "Open With" hands over.
    let app = gtk::Application::builder().application_id(APP_ID).flags(gio::ApplicationFlags::NON_UNIQUE).build();
    let folders = RefCell::new(Some(folders));
    app.connect_activate(move |app| {
        let start = folders.borrow_mut().take().unwrap_or_default();
        build(app, start);
    });
    app.set_accels_for_action("window.close", &["<Control>q", "<Control>w"]);
    // GTK is not handed the arguments: they are folders, not GTK options.
    app.run_with_args(&[args[0].to_string_lossy().into_owned()])
}

/// The scan, in the child. `rest` is the report path and then an img-fp
/// command line, program name first.
fn worker(rest: Vec<std::ffi::OsString>) -> glib::ExitCode {
    let Some((result, argv)) = rest.split_first() else {
        eprintln!("--worker needs a result path");
        return glib::ExitCode::FAILURE;
    };
    match img_fp::worker_main(std::path::Path::new(result), argv.iter().cloned()) {
        Ok(()) => glib::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:#}");
            glib::ExitCode::FAILURE
        }
    }
}

/// The two pages the window moves between, and what they share.
pub struct App {
    pub window: gtk::ApplicationWindow,
    pub stack: gtk::Stack,
    /// Everything the scan said about itself, for the log on both pages.
    pub log: gtk::TextBuffer,
    /// The log's window while it is open, so that asking again raises it.
    log_window: Rc<RefCell<Option<gtk::Window>>>,
}

impl App {
    pub fn show_setup(&self) {
        self.stack.set_visible_child_name("setup");
    }

    pub fn show_results(&self) {
        self.stack.set_visible_child_name("results");
    }

    /// A line for the scan log, kept to its last few thousand lines.
    pub fn log_line(&self, line: &str) {
        let mut end = self.log.end_iter();
        self.log.insert(&mut end, line);
        self.log.insert(&mut end, "\n");
        const KEEP: i32 = 5000;
        let lines = self.log.line_count();
        if lines > KEEP + 500 {
            let mut start = self.log.start_iter();
            if let Some(mut cut) = self.log.iter_at_line(lines - KEEP) {
                self.log.delete(&mut start, &mut cut);
            }
        }
    }

    /// A window that says the log, for either page.
    ///
    /// Not modal: the log fills in while a scan runs, and watching it is no
    /// reason to lose Cancel, or the cards its lines are about. One at a
    /// time, so the button raises the open one rather than stacking copies.
    pub fn show_log(&self) {
        if let Some(win) = self.log_window.borrow().as_ref() {
            win.present();
            return;
        }
        let view = gtk::TextView::builder()
            .buffer(&self.log)
            .editable(false)
            .monospace(true)
            .wrap_mode(gtk::WrapMode::WordChar)
            .left_margin(8)
            .right_margin(8)
            .top_margin(8)
            .bottom_margin(8)
            .build();
        let scroll = gtk::ScrolledWindow::builder().child(&view).vexpand(true).build();
        let win = gtk::Window::builder()
            .title("Scan log")
            .transient_for(&self.window)
            .default_width(864)
            .default_height(608)
            .child(&scroll)
            .build();
        close_on_escape(&win);
        keep_mnemonics_visible(&win);
        let open = self.log_window.clone();
        win.connect_close_request(move |_| {
            open.borrow_mut().take();
            glib::Propagation::Proceed
        });
        *self.log_window.borrow_mut() = Some(win.clone());
        win.present();
        view.grab_focus();
        let mut end = self.log.end_iter();
        view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
    }
}

/// Drop GTK's complaints about the user's own stylesheet.
///
/// `~/.config/gtk-4.0/gtk.css` is loaded into every GTK 4 app, and the ones
/// written for a newer GTK (CSS variables such as `--accent-bg-color` came in
/// with 4.16) make GTK print a "Theme parser error" per rule it cannot read,
/// every start, in every plain GTK app on that desktop. Nothing here can act
/// on them and nothing is wrong with the window, so only those are dropped;
/// every other message is written as it would have been.
fn quiet_theme_errors() {
    glib::log_set_writer_func(|level, fields| {
        let field = |key: &str| fields.iter().find(|f| f.key() == key).and_then(|f| f.value_str());
        if field("GLIB_DOMAIN") == Some("Gtk") && field("MESSAGE").is_some_and(|m| m.starts_with("Theme parser error")) {
            return glib::LogWriterOutput::Handled;
        }
        glib::log_writer_default(level, fields)
    });
}

/// Keep the mnemonics underlined all the time.
///
/// GTK shows them while Alt is held, which needs the app to see a bare Alt
/// press, and not every desktop lets it: Cinnamon holds a passive grab on the
/// left Alt key (the X server's `XF86LogGrabInfo` names it), so under Cinnamon
/// a lone Alt never reaches any application and the underlines only flashed
/// as Alt+letter was pressed. A window meant to be driven from the keyboard is
/// better off saying its keys up front.
///
/// Call it once the window's contents are in place; see `build`.
pub fn keep_mnemonics_visible(window: &impl IsA<gtk::Window>) {
    window.set_mnemonics_visible(true);
    window.connect_mnemonics_visible_notify(|w| {
        if !w.is_mnemonics_visible() {
            w.set_mnemonics_visible(true);
        }
    });
}

/// Use GTK's dark base when the desktop's styling is dark.
///
/// A plain GTK 4 app is drawn by GTK's own theme plus whatever the user has in
/// `~/.config/gtk-4.0/gtk.css`, and desktops that theme libadwaita apps put a
/// whole dark theme there. It sets light text and dark windows, and leaves
/// buttons and entries to the base theme, which is light unless asked
/// otherwise: light text on light buttons, unreadable. So the base follows the
/// text. If the styled window's text is light, or the desktop theme's name
/// says dark (a GTK 3 theme with no GTK 4 variant falls back to the light
/// default), the dark variant is asked for.
fn follow_dark_text(window: &gtk::ApplicationWindow) {
    let Some(settings) = gtk::Settings::default() else { return };
    let named_dark = settings.gtk_theme_name().is_some_and(|n| n.to_lowercase().contains("dark"));
    let c = window.color();
    let light_text = 0.2126 * c.red() + 0.7152 * c.green() + 0.0722 * c.blue() > 0.5;
    if named_dark || light_text {
        settings.set_gtk_application_prefer_dark_theme(true);
    }
}

/// Every key the window answers to, in one place. Opened by F1.
fn show_shortcuts(parent: &gtk::ApplicationWindow) {
    const KEYS: &[(&str, &[(&str, &str)])] = &[
        ("Everywhere", &[
            ("Alt + underlined letter", "Use that button or field"),
            ("Tab, Shift+Tab", "Move between controls"),
            ("F1", "This list"),
            ("Ctrl+Q", "Quit"),
        ]),
        ("Scan", &[
            ("Ctrl+Enter", "Start the scan"),
            ("Escape", "Cancel the scan"),
            ("Delete", "Remove the selected folder from a list"),
        ]),
        ("Results", &[
            ("Arrow keys, Home, End", "Move between the images of a group"),
            ("Space or Delete", "Mark or unmark the image for the Trash"),
            ("Enter", "Open the image large"),
            ("Ctrl+Page Down", "Next group"),
            ("Ctrl+Page Up", "Previous group"),
        ]),
        ("Large view", &[
            ("Left, Right", "Previous or next image"),
            ("Space or Delete", "Mark or unmark it"),
            ("Escape", "Close"),
        ]),
    ];
    let grid = gtk::Grid::builder()
        .row_spacing(6)
        .column_spacing(24)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(20)
        .margin_end(20)
        .build();
    let mut row = 0;
    for (section, keys) in KEYS {
        let head = gtk::Label::builder().label(*section).xalign(0.0).margin_top(if row == 0 { 0 } else { 10 }).build();
        head.add_css_class("heading");
        grid.attach(&head, 0, row, 2, 1);
        row += 1;
        for (key, what) in keys.iter() {
            let k = gtk::Label::builder().label(*key).xalign(0.0).build();
            k.add_css_class("monospace");
            grid.attach(&k, 0, row, 1, 1);
            grid.attach(&gtk::Label::builder().label(*what).xalign(0.0).build(), 1, row, 1, 1);
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
    close_on_escape(&win);
    win.present();
}

/// Escape closes a secondary window, as it does a dialog.
pub fn close_on_escape(win: &gtk::Window) {
    let keys = gtk::EventControllerKey::new();
    let w = win.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            if let Some(w) = w.upgrade() {
                w.close();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    win.add_controller(keys);
}

fn build(app: &gtk::Application, start: Vec<PathBuf>) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("img-fp")
        .default_width(1040)
        .default_height(760)
        .build();
    let stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::None).build();
    window.set_child(Some(&stack));
    let app = Rc::new(App { window: window.clone(), stack: stack.clone(), log: gtk::TextBuffer::new(None), log_window: Rc::default() });

    let mut settings = settings::Settings::load();
    // Folders handed over at start — "Open With" in a file manager — are what
    // to scan, in place of the ones remembered from last time. They used to be
    // added to those, so opening the window on one folder scanned every folder
    // it had ever been pointed at, and nothing on the button said so.
    if !start.is_empty() {
        let mut folders: Vec<PathBuf> = Vec::new();
        for f in start {
            let f = std::fs::canonicalize(&f).unwrap_or(f);
            if !folders.contains(&f) {
                folders.push(f);
            }
        }
        settings.folders = folders;
    }
    follow_dark_text(&window);
    // No animations: the theme's transitions on focus and hover are frames
    // drawn for nothing a person needs to see.
    if let Some(s) = gtk::Settings::default() {
        s.set_gtk_enable_animations(false);
    }
    let results = results::Results::new(app.clone());
    let setup = setup::Setup::new(app.clone(), settings, results.clone());
    stack.add_named(&setup.root, Some("setup"));
    stack.add_named(&results.root, Some("results"));
    results.set_to_settings({
        let app = app.clone();
        let setup = setup.clone();
        move || {
            setup.results_available(true);
            app.show_setup();
            setup.focus_scan();
        }
    });
    app.show_setup();

    // Closing the window stops a scan the way Cancel does, and does not wait
    // for it: the worker answers the signal on its own, and it is also told
    // to stop by the kernel should this process die first.
    window.connect_close_request({
        let setup = setup.clone();
        move |_| {
            setup.stop_scan();
            glib::Propagation::Proceed
        }
    });
    // F1 lists the keys, wherever the keyboard is.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed({
        let w = window.clone();
        move |_, key, _, _| {
            if key == gtk::gdk::Key::F1 {
                show_shortcuts(&w);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);
    // After the pages exist: GTK applies the flag to the labels in the window
    // when it is set, and a label added later does not look it up.
    keep_mnemonics_visible(&window);
    window.present();
    setup.focus_scan();
}
