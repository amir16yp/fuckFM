fn main() {
    fuckfm::fatal::install_panic_hook("fuckFM");
    if let Err(err) = fuckfm::app::boot() {
        fuckfm::fatal::show_error("fuckFM", &err);
        std::process::exit(1);
    }
}
