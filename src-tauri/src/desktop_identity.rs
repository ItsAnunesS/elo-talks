//! Shared native window identity and embedded branding, independent of icon themes.
use anyhow::Result;
use gtk::prelude::*;

pub const APP_ID: &str = "dev.anuness.elotalks";

pub fn prepare() {
    static PREPARE: std::sync::Once = std::sync::Once::new();
    PREPARE.call_once(|| {
        gtk::glib::set_prgname(Some(APP_ID));
        gtk::glib::set_application_name("Elo Talks");
    });
}

pub fn apply(window: &impl IsA<gtk::Window>) -> Result<()> {
    static RESOURCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if RESOURCE.get().is_none() {
        let resource = gtk::gio::Resource::from_data(&gtk::glib::Bytes::from_static(
            include_bytes!(concat!(env!("OUT_DIR"), "/desktop.gresource")),
        ))?;
        RESOURCE.get_or_init(|| {
            // The global registry retains its own reference to the resource.
            gtk::gio::resources_register(&resource);
        });
    }
    let window = window.as_ref();
    let theme = gtk::IconTheme::for_display(&gtk::prelude::WidgetExt::display(window));
    theme.add_resource_path("/dev/anuness/elotalks/icons");
    gtk::Window::set_default_icon_name(APP_ID);
    window.set_icon_name(Some(APP_ID));
    Ok(())
}
