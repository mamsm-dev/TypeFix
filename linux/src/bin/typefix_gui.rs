//! TypeFix settings window (GTK 3).
//!
//! Lets the user record the conversion shortcut, toggle typing suggestions and
//! pick theme (system/light/dark) and UI language (English/Persian). Closing the
//! window keeps TypeFix running in the background; starting TypeFix again shows
//! the window, and a tray icon (StatusNotifierItem) offers "Open" and "Quit".
//! Settings are stored in ~/.config/typefix/config, shared with the
//! command-line `typefix`. Selected and typed text is kept in memory only.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf};
use typefix_core::{config::Config, service::Service, shortcut::Shortcut, Language};

const APP_ID: &str = "io.github.mamsmdev.TypeFix";
const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Space";
const ICON_PNG: &[u8] = include_bytes!("../../packaging/icons/typefix-256.png");

/// Brand color of the interface.
const ACCENT: &str = "#18B0B0";
/// Text color on top of the accent (dark, for readable contrast).
const ON_ACCENT: &str = "#052727";

struct Text {
    subtitle: &'static str,
    shortcut_title: &'static str,
    shortcut_hint: &'static str,
    change: &'static str,
    recording: &'static str,
    recording_prompt: &'static str,
    need_modifier: &'static str,
    reset: &'static str,
    suggest_title: &'static str,
    suggest_hint: &'static str,
    direction: &'static str,
    dir_en: &'static str,
    dir_fa: &'static str,
    appearance: &'static str,
    theme: &'static str,
    theme_system: &'static str,
    theme_light: &'static str,
    theme_dark: &'static str,
    language: &'static str,
    background_hint: &'static str,
    quit: &'static str,
}

fn text(persian: bool) -> Text {
    if persian {
        Text {
            subtitle: "تبدیل متن بین چیدمان صفحه‌کلید انگلیسی و فارسی",
            shortcut_title: "میان‌بر تبدیل",
            shortcut_hint: "در هر برنامه‌ای متن را انتخاب کنید و این میان‌بر را بزنید.",
            change: "تغییر میان‌بر",
            recording: "میان‌بر جدید را بزنید…",
            recording_prompt: "ترکیب کلید دلخواه را فشار دهید (Esc برای لغو)",
            need_modifier: "میان‌بر باید حداقل یکی از Ctrl، Alt، Shift یا Super را داشته باشد.",
            reset: "بازنشانی",
            suggest_title: "پیشنهاد هنگام تایپ",
            suggest_hint: "وقتی کلمه‌ای با چیدمان اشتباه تایپ شده باشد اعلان نشان می‌دهد. برای این کار کلیدهای فشرده‌شده روی این سرور X زیر نظر گرفته می‌شود.",
            direction: "متن تایپ‌شده با",
            dir_en: "چیدمان انگلیسی ← فارسی",
            dir_fa: "چیدمان فارسی ← انگلیسی",
            appearance: "ظاهر",
            theme: "تم",
            theme_system: "پیروی از سیستم",
            theme_light: "روشن",
            theme_dark: "تیره",
            language: "زبان",
            background_hint: "بستن پنجره، TypeFix را در پس‌زمینه فعال نگه می‌دارد. برای نمایش دوباره، برنامه را دوباره باز کنید.",
            quit: "خروج",
        }
    } else {
        Text {
            subtitle: "Convert text between English and Persian keyboard layouts",
            shortcut_title: "Conversion shortcut",
            shortcut_hint: "Select text in any application and press this shortcut.",
            change: "Change shortcut",
            recording: "Press the new shortcut…",
            recording_prompt: "Press the key combination you want (Esc cancels)",
            need_modifier: "The shortcut needs at least one of Ctrl, Alt, Shift or Super.",
            reset: "Reset",
            suggest_title: "Suggest while typing",
            suggest_hint: "Shows a notification when a word looks typed in the wrong layout. This observes key presses on this X server.",
            direction: "Text typed with",
            dir_en: "English layout \u{2192} Persian",
            dir_fa: "Persian layout \u{2192} English",
            appearance: "Appearance",
            theme: "Theme",
            theme_system: "Follow system",
            theme_light: "Light",
            theme_dark: "Dark",
            language: "Language",
            background_hint: "Closing the window keeps TypeFix running in the background. Open TypeFix again to show it.",
            quit: "Quit",
        }
    }
}

fn active_message(persian: bool, shortcut: &str) -> String {
    if persian {
        format!("فعال — برای تبدیل متن انتخاب‌شده {shortcut} را بزنید")
    } else {
        format!("Active — press {shortcut} to convert the selected text")
    }
}

fn error_message(persian: bool, error: &str) -> String {
    if persian {
        format!("خطا: {error}")
    } else {
        format!("Problem: {error}")
    }
}

fn system_prefers_dark() -> bool {
    let query = |key: &str| {
        std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", key])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .unwrap_or_default()
            .to_lowercase()
    };
    query("color-scheme").contains("dark") || query("gtk-theme").contains("dark")
}

fn css(dark: bool) -> String {
    // (background, card, border, text, muted, accent text, keycap, error)
    let (bg, card, border, fg, muted, accent_text, keycap, error) = if dark {
        ("#0C1819", "#122324", "#1E3C3D", "#E4F3F3", "#93B3B3", "#3FD2D2", "#0F3536", "#FF8A80")
    } else {
        ("#F2F9F9", "#FFFFFF", "#CBE5E5", "#0F2B2B", "#4A6B6B", "#0B7373", "#E3F6F6", "#C62828")
    };
    format!(
        r#"
window.tf-window {{ background-color: {bg}; }}
.tf-window label {{ color: {fg}; }}
.tf-window label.tf-muted {{ color: {muted}; }}
.tf-window label.tf-title {{ font-size: 24px; font-weight: 700; }}
.tf-window label.tf-section {{ font-size: 15px; font-weight: 700; color: {accent_text}; }}
.tf-window label.tf-ok {{ color: {accent_text}; font-weight: 600; }}
.tf-window label.tf-error {{ color: {error}; font-weight: 600; }}
.tf-window .tf-card {{ background-color: {card}; border: 1px solid {border}; border-radius: 14px; padding: 16px; }}
.tf-window label.tf-keycap {{ background-color: {keycap}; color: {fg}; border: 1px solid {ACCENT}; border-radius: 9px; padding: 7px 14px; font-weight: 700; }}
.tf-window label.tf-keycap-recording {{ background-color: {ACCENT}; color: {ON_ACCENT}; border: 1px solid {ACCENT}; border-radius: 9px; padding: 7px 14px; font-weight: 700; }}
.tf-window button.tf-primary {{ background-image: none; background-color: {ACCENT}; border: 1px solid {ACCENT}; border-radius: 10px; padding: 7px 16px; box-shadow: none; text-shadow: none; }}
.tf-window button.tf-primary label {{ color: {ON_ACCENT}; font-weight: 700; }}
.tf-window button.tf-primary:hover {{ background-color: #2BC9C9; border-color: #2BC9C9; }}
.tf-window button.tf-primary:active {{ background-color: #129797; border-color: #129797; }}
.tf-window button.tf-secondary {{ background-image: none; background-color: transparent; border: 1px solid {border}; border-radius: 10px; padding: 7px 16px; box-shadow: none; text-shadow: none; }}
.tf-window button.tf-secondary:hover {{ border-color: {ACCENT}; }}
.tf-window switch {{ background-image: none; background-color: {border}; border: 1px solid {border}; }}
.tf-window switch:checked {{ background-color: {ACCENT}; border-color: {ACCENT}; }}
.tf-window switch slider {{ background-image: none; background-color: #FFFFFF; border: 1px solid {border}; }}
.tf-window combobox button {{ background-image: none; background-color: {keycap}; border: 1px solid {border}; border-radius: 9px; box-shadow: none; }}
.tf-window combobox button:hover {{ border-color: {ACCENT}; }}
.tf-window combobox button cellview, .tf-window combobox button label {{ color: {fg}; text-shadow: none; }}
"#
    )
}

/// Result of interpreting one key press while recording a new shortcut.
enum Captured {
    Ignore,
    Cancel,
    NeedModifier,
    Shortcut(String),
}

fn capture(event: &gdk::EventKey) -> Captured {
    let keyval = event.keyval();
    let name = keyval.name().map(|n| n.to_string()).unwrap_or_default();
    let state = event.state();
    let has_modifier = state.intersects(
        gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::MOD1_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::MOD4_MASK,
    );

    if name == "Escape" && !has_modifier {
        return Captured::Cancel;
    }
    const MODIFIER_KEYS: [&str; 11] = [
        "Shift_", "Control_", "Alt_", "Super_", "Meta_", "Hyper_", "ISO_Level", "Caps_Lock",
        "Num_Lock", "Scroll_Lock", "Mode_switch",
    ];
    if name.is_empty() || MODIFIER_KEYS.iter().any(|prefix| name.starts_with(prefix)) {
        return Captured::Ignore;
    }
    if !has_modifier {
        return Captured::NeedModifier;
    }

    // Use the key's name in the first layout group without modifiers, so the
    // shortcut reads the same whichever layout is active while recording.
    let base_name = gdk::Display::default()
        .and_then(|display| gdk::Keymap::for_display(&display))
        .and_then(|keymap| {
            keymap.translate_keyboard_state(
                u32::from(event.hardware_keycode()),
                gdk::ModifierType::empty(),
                0,
            )
        })
        .and_then(|(key, _, _, _)| gdk::keys::Key::from(key).name())
        .map(|n| n.to_string())
        .unwrap_or_else(|| keyval.to_lower().name().map(|n| n.to_string()).unwrap_or(name));

    let mut parts: Vec<String> = Vec::new();
    if state.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("Ctrl".into());
    }
    if state.contains(gdk::ModifierType::MOD1_MASK) {
        parts.push("Alt".into());
    }
    if state.contains(gdk::ModifierType::SHIFT_MASK) {
        parts.push("Shift".into());
    }
    if state.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::MOD4_MASK) {
        parts.push("Super".into());
    }
    parts.push(base_name);
    Captured::Shortcut(parts.join("+"))
}

fn icon_pixbuf(size: i32) -> Option<gdk_pixbuf::Pixbuf> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.write(ICON_PNG).ok()?;
    loader.close().ok()?;
    loader.pixbuf()?.scale_simple(size, size, gdk_pixbuf::InterpType::Bilinear)
}

fn label(content: &str, classes: &[&str]) -> gtk::Label {
    let label = gtk::Label::new(Some(content));
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_max_width_chars(52);
    for class in classes {
        label.style_context().add_class(class);
    }
    label
}

fn card() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.style_context().add_class("tf-card");
    card
}

fn button(content: &str, class: &str) -> gtk::Button {
    let button = gtk::Button::with_label(content);
    button.style_context().add_class(class);
    button
}

enum TrayCommand {
    Show,
    Quit,
}

/// System tray icon. It runs on its own thread and talks to the GTK main
/// thread through a channel (polled by a timer).
struct TypeFixTray {
    persian: bool,
    icon: Vec<ksni::Icon>,
    tx: mpsc::Sender<TrayCommand>,
}

impl ksni::Tray for TypeFixTray {
    fn id(&self) -> String {
        "typefix".into()
    }

    fn title(&self) -> String {
        "TypeFix".into()
    }

    // The icon is sent as pixels so it also shows when the icon theme does not
    // contain the TypeFix icon (for example when run as an AppImage).
    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayCommand::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let (open, quit) = if self.persian {
            ("باز کردن TypeFix", "خروج")
        } else {
            ("Open TypeFix", "Quit")
        };
        vec![
            StandardItem {
                label: open.into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayCommand::Show);
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: quit.into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Converts the embedded icon to the ARGB32 pixmaps used by StatusNotifierItem.
fn tray_icons() -> Vec<ksni::Icon> {
    let mut icons = Vec::new();
    for size in [24, 32, 48, 64] {
        let Some(pixbuf) = icon_pixbuf(size) else { continue };
        let pixbuf = if pixbuf.has_alpha() {
            pixbuf
        } else {
            match pixbuf.add_alpha(false, 0, 0, 0) {
                Ok(with_alpha) => with_alpha,
                Err(_) => continue,
            }
        };
        let (width, height) = (pixbuf.width() as usize, pixbuf.height() as usize);
        let stride = pixbuf.rowstride() as usize;
        let channels = pixbuf.n_channels() as usize;
        let bytes = pixbuf.read_pixel_bytes();
        let mut data = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let offset = y * stride + x * channels;
                let (r, g, b, a) = (bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]);
                data.extend_from_slice(&[a, r, g, b]);
            }
        }
        icons.push(ksni::Icon { width: width as i32, height: height as i32, data });
    }
    icons
}

struct State {
    app: gtk::Application,
    window: gtk::ApplicationWindow,
    provider: gtk::CssProvider,
    config: RefCell<Config>,
    service: RefCell<Service>,
    recording: Cell<bool>,
    /// A message to show instead of the "active" status.
    problem: RefCell<Option<String>>,
    tray: RefCell<Option<ksni::Handle<TypeFixTray>>>,
}

impl State {
    fn persian(&self) -> bool {
        self.config.borrow().language.as_deref() == Some("fa")
    }

    fn shortcut_text(&self) -> String {
        self.config
            .borrow()
            .shortcut
            .clone()
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_owned())
    }

    fn source(&self) -> Language {
        match self.config.borrow().source.as_deref() {
            Some("fa") => Language::Persian,
            _ => Language::English,
        }
    }

    fn suggest_on(&self) -> bool {
        self.config.borrow().suggest.unwrap_or(false)
    }

    fn dark(&self) -> bool {
        match self.config.borrow().theme.as_deref() {
            Some("dark") => true,
            Some("light") => false,
            _ => system_prefers_dark(),
        }
    }

    fn save(&self) {
        if let Err(error) = self.config.borrow().save() {
            *self.problem.borrow_mut() = Some(error);
        }
    }

    fn apply_theme(&self) {
        let dark = self.dark();
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(dark);
        }
        let _ = self.provider.load_from_data(css(dark).as_bytes());
    }

    /// Grabs `text` as the new shortcut. If it cannot be used (for example it
    /// belongs to another application), the previous shortcut is restored.
    fn apply_shortcut(&self, text: &str) -> Result<(), String> {
        let shortcut = Shortcut::parse(text)?;
        let previous = self.shortcut_text();
        let result = self.service.borrow_mut().set_shortcut(&shortcut);
        match result {
            Ok(()) => {
                self.config.borrow_mut().shortcut = Some(shortcut.to_string());
                self.save();
                Ok(())
            }
            Err(error) => {
                if let Ok(old) = Shortcut::parse(&previous) {
                    let _ = self.service.borrow_mut().set_shortcut(&old);
                }
                Err(error)
            }
        }
    }

    fn restart_suggest(&self) -> Result<(), String> {
        let enabled = self.suggest_on();
        let source = self.source();
        self.service.borrow_mut().set_suggest(enabled, source, None)
    }

    fn render_later(self: &Rc<Self>) {
        let state = Rc::clone(self);
        glib::idle_add_local_once(move || state.render());
    }

    fn render(self: &Rc<Self>) {
        let persian = self.persian();
        gtk::Widget::set_default_direction(if persian {
            gtk::TextDirection::Rtl
        } else {
            gtk::TextDirection::Ltr
        });
        self.apply_theme();
        let t = text(persian);

        for child in self.window.children() {
            self.window.remove(&child);
        }
        let root = gtk::Box::new(gtk::Orientation::Vertical, 14);
        root.set_margin_top(20);
        root.set_margin_bottom(20);
        root.set_margin_start(20);
        root.set_margin_end(20);

        // Header: icon, name, subtitle.
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        if let Some(pixbuf) = icon_pixbuf(64) {
            header.pack_start(&gtk::Image::from_pixbuf(Some(&pixbuf)), false, false, 0);
        }
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 2);
        titles.set_valign(gtk::Align::Center);
        titles.pack_start(&label("TypeFix", &["tf-title"]), false, false, 0);
        titles.pack_start(&label(t.subtitle, &["tf-muted"]), false, false, 0);
        header.pack_start(&titles, true, true, 0);
        root.pack_start(&header, false, false, 0);

        // Shortcut card.
        let shortcut_card = card();
        shortcut_card.pack_start(&label(t.shortcut_title, &["tf-section"]), false, false, 0);
        shortcut_card.pack_start(&label(t.shortcut_hint, &["tf-muted"]), false, false, 0);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let recording = self.recording.get();
        let keycap_text = if recording {
            t.recording.to_owned()
        } else {
            self.shortcut_text().replace('+', " + ")
        };
        let keycap = label(
            &keycap_text,
            &[if recording { "tf-keycap-recording" } else { "tf-keycap" }],
        );
        keycap.set_xalign(0.5);
        row.pack_start(&keycap, false, false, 0);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.pack_start(&spacer, true, true, 0);
        let change = button(t.change, "tf-primary");
        change.set_sensitive(!recording);
        {
            let state = Rc::clone(self);
            change.connect_clicked(move |_| {
                *state.problem.borrow_mut() = None;
                state.recording.set(true);
                state.render_later();
            });
        }
        let reset = button(t.reset, "tf-secondary");
        {
            let state = Rc::clone(self);
            reset.connect_clicked(move |_| {
                state.recording.set(false);
                *state.problem.borrow_mut() = match state.apply_shortcut(DEFAULT_SHORTCUT) {
                    Ok(()) => None,
                    Err(error) => Some(error),
                };
                state.render_later();
            });
        }
        row.pack_end(&reset, false, false, 0);
        row.pack_end(&change, false, false, 0);
        shortcut_card.pack_start(&row, false, false, 0);

        let (status_text, status_class) = if recording {
            match self.problem.borrow().as_ref() {
                Some(hint) => (hint.clone(), "tf-error"),
                None => (t.recording_prompt.to_owned(), "tf-muted"),
            }
        } else if let Some(problem) = self.problem.borrow().as_ref() {
            (error_message(persian, problem), "tf-error")
        } else {
            (active_message(persian, &self.shortcut_text()), "tf-ok")
        };
        shortcut_card.pack_start(&label(&status_text, &[status_class]), false, false, 0);
        root.pack_start(&shortcut_card, false, false, 0);

        // Suggestions card.
        let suggest_card = card();
        let suggest_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let suggest_text = gtk::Box::new(gtk::Orientation::Vertical, 4);
        suggest_text.pack_start(&label(t.suggest_title, &["tf-section"]), false, false, 0);
        suggest_text.pack_start(&label(t.suggest_hint, &["tf-muted"]), false, false, 0);
        suggest_row.pack_start(&suggest_text, true, true, 0);
        let switch = gtk::Switch::new();
        switch.set_valign(gtk::Align::Center);
        switch.set_active(self.suggest_on());
        {
            let state = Rc::clone(self);
            switch.connect_state_set(move |_, on| {
                state.config.borrow_mut().suggest = Some(on);
                match state.restart_suggest() {
                    Ok(()) => *state.problem.borrow_mut() = None,
                    Err(error) => {
                        state.config.borrow_mut().suggest = Some(false);
                        *state.problem.borrow_mut() = Some(error);
                    }
                }
                state.save();
                state.render_later();
                glib::Propagation::Proceed
            });
        }
        suggest_row.pack_end(&switch, false, false, 0);
        suggest_card.pack_start(&suggest_row, false, false, 0);

        let direction_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        direction_row.pack_start(&label(t.direction, &[]), false, false, 0);
        let direction = gtk::ComboBoxText::new();
        direction.append(Some("en"), t.dir_en);
        direction.append(Some("fa"), t.dir_fa);
        direction.set_active_id(Some(if self.source() == Language::Persian { "fa" } else { "en" }));
        direction.set_sensitive(self.suggest_on());
        {
            let state = Rc::clone(self);
            direction.connect_changed(move |combo| {
                if let Some(id) = combo.active_id() {
                    if state.config.borrow().source.as_deref() == Some(id.as_str()) {
                        return;
                    }
                    state.config.borrow_mut().source = Some(id.to_string());
                    state.save();
                    if let Err(error) = state.restart_suggest() {
                        *state.problem.borrow_mut() = Some(error);
                        state.render_later();
                    }
                }
            });
        }
        direction_row.pack_end(&direction, false, false, 0);
        suggest_card.pack_start(&direction_row, false, false, 0);
        root.pack_start(&suggest_card, false, false, 0);

        // Appearance card.
        let look_card = card();
        look_card.pack_start(&label(t.appearance, &["tf-section"]), false, false, 0);

        let theme_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        theme_row.pack_start(&label(t.theme, &[]), false, false, 0);
        let theme = gtk::ComboBoxText::new();
        theme.append(Some("system"), t.theme_system);
        theme.append(Some("light"), t.theme_light);
        theme.append(Some("dark"), t.theme_dark);
        theme.set_active_id(Some(self.config.borrow().theme.as_deref().unwrap_or("system")));
        {
            let state = Rc::clone(self);
            theme.connect_changed(move |combo| {
                if let Some(id) = combo.active_id() {
                    state.config.borrow_mut().theme = Some(id.to_string());
                    state.save();
                    state.apply_theme();
                }
            });
        }
        theme_row.pack_end(&theme, false, false, 0);
        look_card.pack_start(&theme_row, false, false, 0);

        let language_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        language_row.pack_start(&label(t.language, &[]), false, false, 0);
        let language = gtk::ComboBoxText::new();
        language.append(Some("en"), "English");
        language.append(Some("fa"), "فارسی");
        language.set_active_id(Some(if persian { "fa" } else { "en" }));
        {
            let state = Rc::clone(self);
            language.connect_changed(move |combo| {
                if let Some(id) = combo.active_id() {
                    if state.config.borrow().language.as_deref().unwrap_or("en") == id.as_str() {
                        return;
                    }
                    state.config.borrow_mut().language = Some(id.to_string());
                    state.save();
                    let persian = id.as_str() == "fa";
                    if let Some(tray) = state.tray.borrow().as_ref() {
                        tray.update(|tray: &mut TypeFixTray| tray.persian = persian);
                    }
                    state.render_later();
                }
            });
        }
        language_row.pack_end(&language, false, false, 0);
        look_card.pack_start(&language_row, false, false, 0);
        root.pack_start(&look_card, false, false, 0);

        // Footer.
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        footer.pack_start(&label(t.background_hint, &["tf-muted"]), true, true, 0);
        let quit = button(t.quit, "tf-secondary");
        quit.set_valign(gtk::Align::Center);
        {
            let state = Rc::clone(self);
            quit.connect_clicked(move |_| {
                state.service.borrow_mut().stop();
                state.app.quit();
            });
        }
        footer.pack_end(&quit, false, false, 0);
        root.pack_start(&footer, false, false, 0);

        self.window.add(&root);
        self.window.show_all();
    }

    /// Handles a key press while the shortcut recorder is active.
    fn handle_key(self: &Rc<Self>, event: &gdk::EventKey) -> glib::Propagation {
        if !self.recording.get() {
            return glib::Propagation::Proceed;
        }
        let t = text(self.persian());
        match capture(event) {
            Captured::Ignore => {}
            Captured::Cancel => {
                self.recording.set(false);
                self.render_later();
            }
            Captured::NeedModifier => {
                // Keep recording and show the hint.
                *self.problem.borrow_mut() = Some(t.need_modifier.to_owned());
                self.render_later();
            }
            Captured::Shortcut(candidate) => {
                self.recording.set(false);
                *self.problem.borrow_mut() = match self.apply_shortcut(&candidate) {
                    Ok(()) => None,
                    Err(error) => Some(error),
                };
                self.render_later();
            }
        }
        glib::Propagation::Stop
    }
}

fn build(app: &gtk::Application) -> Rc<State> {
    let window = gtk::ApplicationWindow::new(app);
    window.set_title("TypeFix");
    window.set_default_size(500, -1);
    window.set_position(gtk::WindowPosition::Center);
    window.style_context().add_class("tf-window");
    if let Some(pixbuf) = icon_pixbuf(128) {
        window.set_icon(Some(&pixbuf));
    }

    let provider = gtk::CssProvider::new();
    if let Some(screen) = gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let mut problem = None;
    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            problem = Some(error);
            Config::default()
        }
    };

    let state = Rc::new(State {
        app: app.clone(),
        window: window.clone(),
        provider,
        config: RefCell::new(config),
        service: RefCell::new(Service::new()),
        recording: Cell::new(false),
        problem: RefCell::new(problem),
        tray: RefCell::new(None),
    });

    // Start the background workers with the saved settings.
    let shortcut = state.shortcut_text();
    if let Err(error) = state.apply_shortcut(&shortcut) {
        *state.problem.borrow_mut() = Some(error);
    }
    if state.suggest_on() {
        if let Err(error) = state.restart_suggest() {
            state.config.borrow_mut().suggest = Some(false);
            *state.problem.borrow_mut() = Some(error);
        }
    }

    {
        let state = Rc::clone(&state);
        window.connect_key_press_event(move |_, event| state.handle_key(event));
    }
    // Closing the window hides it; TypeFix keeps running in the background.
    window.connect_delete_event(|window, _| {
        window.hide();
        glib::Propagation::Stop
    });

    // Tray icon: left click or "Open" shows the window, "Quit" stops TypeFix.
    let (tx, rx) = mpsc::channel::<TrayCommand>();
    let tray_service = ksni::TrayService::new(TypeFixTray {
        persian: state.persian(),
        icon: tray_icons(),
        tx,
    });
    *state.tray.borrow_mut() = Some(tray_service.handle());
    tray_service.spawn();
    {
        let state = Rc::clone(&state);
        glib::timeout_add_local(Duration::from_millis(150), move || {
            while let Ok(command) = rx.try_recv() {
                match command {
                    TrayCommand::Show => state.window.present(),
                    TrayCommand::Quit => {
                        state.service.borrow_mut().stop();
                        state.app.quit();
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    state.render();
    state
}

fn main() {
    let app = gtk::Application::new(Some(APP_ID), Default::default());
    let slot: Rc<RefCell<Option<Rc<State>>>> = Rc::new(RefCell::new(None));

    app.connect_activate(move |app| {
        let existing = slot.borrow().clone();
        match existing {
            // A second launch shows the window of the running instance.
            Some(state) => state.window.present(),
            None => {
                // Keep the application alive while the window is hidden.
                std::mem::forget(app.hold());
                *slot.borrow_mut() = Some(build(app));
            }
        }
    });

    app.run();
}
