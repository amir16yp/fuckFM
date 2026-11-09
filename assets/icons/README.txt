fuckFM / fuckWM binary icon packs (`.fficons`)
==============================================

Converted from B00merang-Artwork freedesktop themes with:

  cargo run --bin b00merang2fficons -- <theme-dir> -o assets/icons/<name>.fficons

Bundled packs:

  winxp_luna.fficons  ← https://github.com/B00merang-Artwork/Windows-XP
  win7_aero.fficons   ← https://github.com/B00merang-Artwork/Windows-7
  modern.fficons      ← https://github.com/B00merang-Artwork/Windows-10

Theme → pack:

  Win95 / Win98 / XP Luna  → winxp_luna.fficons
    (B00merang has no Win95/98 icon theme; XP is the classic stand-in)
  Win7 Aero                → win7_aero.fficons
  Modern Light / Dark      → modern.fficons

Override with Settings appearance.icon_pack, or drop files under
~/.config/fuckfm/icons/ (Linux) / %APPDATA%\fuckFM\icons\ (Windows).
