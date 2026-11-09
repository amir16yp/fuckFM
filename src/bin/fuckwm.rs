fn main() {
    fuckfm::fatal::install_panic_hook("fuckWM");
    if let Err(err) = fuckfm::wm::boot("fuckWM") {
        fuckfm::fatal::show_error("fuckWM", &err);
        std::process::exit(1);
    }
}
