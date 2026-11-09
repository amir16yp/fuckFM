fn main() {
    // Windows packaging name; same nested shell host as fuckWM until a real
    // Desktop Window Manager / shell session backend exists.
    fuckfm::fatal::install_panic_hook("dwm");
    if let Err(err) = fuckfm::wm::boot("dwm") {
        fuckfm::fatal::show_error("dwm", &err);
        std::process::exit(1);
    }
}
