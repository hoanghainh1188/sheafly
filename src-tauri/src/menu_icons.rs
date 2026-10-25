//! SF Symbols beside the native menu items on macOS. Tauri's menus take only bitmaps or a
//! few named images, and a bitmap does not follow a dark menu, so the symbols are set on
//! the built `NSMenu` instead: each item is found by its label in the current language.
//! A symbol the system does not have (an older macOS) is skipped and the item shows none.
//! macOS never draws images in the menu bar itself, only in the menus it opens.

use crate::i18n::{self, Entry, NativeItem};
use sheafly_engine::lang::Lang;

/// Menu id (or submenu id) → SF Symbol name.
pub const SYMBOLS: &[(&str, &str)] = &[
    ("new", "doc.badge.plus"),
    ("open", "folder"),
    ("save", "square.and.arrow.down"),
    ("save-as", "square.and.arrow.down.on.square"),
    ("import", "tray.and.arrow.down"),
    ("import-xml", "doc.text"),
    ("import-table", "tablecells"),
    ("pull-tracker", "arrow.down.circle"),
    ("export", "square.and.arrow.up"),
    ("export-xml", "doc.text"),
    ("export-excel", "tablecells"),
    ("export-pdf", "doc.richtext"),
    ("push-tracker", "arrow.up.circle"),
    ("undo", "arrow.uturn.backward"),
    ("redo", "arrow.uturn.forward"),
    ("copy-task", "plus.square.on.square"),
    ("paste-task", "doc.on.clipboard"),
    ("fill-down", "arrow.down.to.line"),
    ("indent", "increase.indent"),
    ("outdent", "decrease.indent"),
    ("delete", "trash"),
    ("task-tools", "square.stack"),
    ("recurring", "repeat"),
    ("split-task", "scissors"),
    ("view-gantt", "chart.bar.xaxis"),
    ("view-tracking", "checklist"),
    ("view-resources", "person.2"),
    ("view-matrix", "rectangle.split.3x3"),
    ("view-usage", "tablecells"),
    ("view-network", "point.3.connected.trianglepath.dotted"),
    ("view-load", "square.grid.3x3"),
    ("view-curves", "chart.xyaxis.line"),
    ("view-forecast", "chart.bar.xaxis.ascending"),
    ("view-evm", "chart.line.uptrend.xyaxis"),
    ("view-ai", "sparkles.rectangle.stack"),
    ("timescale", "calendar"),
    ("zoom-in", "plus.magnifyingglass"),
    ("zoom-out", "minus.magnifyingglass"),
    ("zoom-reset", "1.magnifyingglass"),
    ("project-setup", "gearshape"),
    ("project-info", "info.circle"),
    ("custom-fields", "list.bullet.rectangle"),
    ("working-time", "clock"),
    ("tracker-settings", "arrow.triangle.2.circlepath"),
    ("baseline", "flag"),
    ("leveling", "slider.horizontal.3"),
    ("level", "wand.and.stars"),
    ("clear-level", "xmark.circle"),
    ("leveling-order", "list.number"),
    ("assigning", "person.badge.plus"),
    ("assign-by-rules", "person.crop.circle.badge.checkmark"),
    ("assignment-rules", "list.bullet.clipboard"),
    ("tracking", "chart.bar.doc.horizontal"),
    ("reschedule-uncompleted", "calendar.badge.clock"),
    ("schedule-health", "stethoscope"),
    ("compact-history", "archivebox"),
    ("license", "key"),
    ("ai-settings", "sparkles"),
    ("third-party-licenses", "doc.plaintext"),
];

/// Symbols for the menus built outside the tables (`lib.rs`) and the system edit items.
fn extra_symbols(lang: Lang) -> Vec<(&'static str, &'static str)> {
    let text = i18n::text(lang);
    vec![
        (text.open_recent, "clock.arrow.circlepath"),
        (text.menu_theme, "circle.lefthalf.filled"),
        (text.menu_language, "globe"),
    ]
}

fn native_symbol(kind: NativeItem) -> &'static str {
    match kind {
        NativeItem::Cut => "scissors",
        NativeItem::Copy => "doc.on.doc",
        NativeItem::Paste => "doc.on.clipboard",
        NativeItem::SelectAll => "checkmark.rectangle",
    }
}

fn symbol_of(id: &str) -> Option<&'static str> {
    SYMBOLS.iter().find(|(key, _)| *key == id).map(|(_, symbol)| *symbol)
}

fn collect(entries: &'static [Entry], lang: Lang, out: &mut Vec<(&'static str, &'static str)>) {
    for entry in entries {
        match entry {
            Entry::Item { id, vi, en, .. } => {
                if let Some(symbol) = symbol_of(id) {
                    out.push((lang.pick(*vi, *en), symbol));
                }
            }
            Entry::Native { kind, vi, en } => out.push((lang.pick(*vi, *en), native_symbol(*kind))),
            Entry::Submenu { id, vi, en, entries } => {
                if let Some(symbol) = symbol_of(id) {
                    out.push((lang.pick(*vi, *en), symbol));
                }
                collect(entries, lang, out);
            }
            Entry::Separator => {}
        }
    }
}

/// Label → symbol for every menu item that has one, in `lang`.
pub fn symbols_by_label(lang: Lang) -> Vec<(&'static str, &'static str)> {
    let mut out = extra_symbols(lang);
    for menu in [i18n::FILE_MENU, i18n::EDIT_MENU, i18n::VIEW_MENU, i18n::PROJECT_MENU, i18n::HELP_MENU] {
        collect(menu, lang, &mut out);
    }
    out
}

/// Put the symbols on the app's menu; call after `set_menu`, from any thread.
pub fn apply(app: &tauri::AppHandle, lang: Lang) {
    #[cfg(target_os = "macos")]
    {
        let labels = symbols_by_label(lang);
        let _ = app.run_on_main_thread(move || macos::apply(&labels));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, lang);
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage, NSMenu};
    use objc2_foundation::NSString;

    pub fn apply(labels: &[(&str, &str)]) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        if let Some(menu) = NSApplication::sharedApplication(mtm).mainMenu() {
            walk(&menu, labels);
        }
    }

    fn walk(menu: &NSMenu, labels: &[(&str, &str)]) {
        for item in menu.itemArray().iter() {
            let title = item.title().to_string();
            if let Some((_, symbol)) = labels.iter().find(|(label, _)| *label == title) {
                let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None);
                if let Some(image) = image {
                    item.setImage(Some(&image));
                }
            }
            if let Some(sub) = item.submenu() {
                walk(&sub, labels);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(entries: &'static [Entry], out: &mut Vec<&'static str>) {
        for entry in entries {
            match entry {
                Entry::Item { id, .. } => out.push(id),
                Entry::Submenu { id, entries, .. } => {
                    out.push(id);
                    ids(entries, out);
                }
                Entry::Native { .. } | Entry::Separator => {}
            }
        }
    }

    #[test]
    fn every_symbol_names_a_menu_item() {
        let mut known = Vec::new();
        for menu in [i18n::FILE_MENU, i18n::EDIT_MENU, i18n::VIEW_MENU, i18n::PROJECT_MENU, i18n::HELP_MENU] {
            ids(menu, &mut known);
        }
        for (id, symbol) in SYMBOLS {
            assert!(known.contains(id), "{id} is not a menu item");
            assert!(!symbol.is_empty());
        }
    }

    #[test]
    fn labels_are_unique_so_each_item_gets_its_own_symbol() {
        for lang in [Lang::Vi, Lang::En] {
            let labels = symbols_by_label(lang);
            for (index, (label, _)) in labels.iter().enumerate() {
                let clash = labels[index + 1..].iter().find(|(other, _)| other == label);
                assert!(clash.is_none_or(|(_, symbol)| *symbol == labels[index].1), "{label} has two symbols");
            }
        }
    }
}
