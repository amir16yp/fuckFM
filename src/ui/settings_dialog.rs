use crate::render::{Frame, Rect};
use crate::settings::Settings;
use crate::theme::{Theme, ThemeId};
use crate::ui::Button;
use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsHit {
    None,
    Close,
    Theme(ThemeId),
    IconScale(u8),
    ToggleClassicChrome,
    ToggleVisualEffects,
    ToggleCompactView,
    ToggleHidden,
    TogglePreview,
    ToggleDetailsPane,
    ToggleSidePane,
    ToggleRestoreTabs,
    CycleStartupLocation,
    ToggleSingleClick,
    ToggleConfirmDelete,
    ToggleConfirmDrag,
    CycleConflictPolicy,
    ToggleAutoMount,
    CycleMediaAutoOpen,
}

pub struct SettingsDialog {
    pub open: bool,
    pub rect: Rect,
    parent: Rect,
    close: Button,
    theme_btns: Vec<(ThemeId, Button)>,
    scale_btns: Vec<(u8, Button)>,
    chrome_btn: Button,
    effects_btn: Button,
    compact_btn: Button,
    hidden_btn: Button,
    preview_btn: Button,
    details_btn: Button,
    side_btn: Button,
    restore_btn: Button,
    startup_btn: Button,
    single_click_btn: Button,
    confirm_delete_btn: Button,
    confirm_drag_btn: Button,
    conflict_btn: Button,
    auto_mount_btn: Button,
    media_auto_open_btn: Button,
}

impl SettingsDialog {
    pub fn new() -> Self {
        let themes = ThemeId::ALL
            .iter()
            .map(|&id| (id, Button::new(Rect::default(), id.label())))
            .collect();
        let scales = (1u8..=4)
            .map(|s| (s, Button::new(Rect::default(), format!("{s}×"))))
            .collect();
        Self {
            open: false,
            rect: Rect::default(),
            parent: Rect::default(),
            close: Button::new(Rect::default(), "X"),
            theme_btns: themes,
            scale_btns: scales,
            chrome_btn: Button::new(Rect::default(), ""),
            effects_btn: Button::new(Rect::default(), ""),
            compact_btn: Button::new(Rect::default(), ""),
            hidden_btn: Button::new(Rect::default(), ""),
            preview_btn: Button::new(Rect::default(), ""),
            details_btn: Button::new(Rect::default(), ""),
            side_btn: Button::new(Rect::default(), ""),
            restore_btn: Button::new(Rect::default(), ""),
            startup_btn: Button::new(Rect::default(), ""),
            single_click_btn: Button::new(Rect::default(), ""),
            confirm_delete_btn: Button::new(Rect::default(), ""),
            confirm_drag_btn: Button::new(Rect::default(), ""),
            conflict_btn: Button::new(Rect::default(), ""),
            auto_mount_btn: Button::new(Rect::default(), ""),
            media_auto_open_btn: Button::new(Rect::default(), ""),
        }
    }

    pub fn show(&mut self, parent: Rect) {
        self.open = true;
        self.parent = parent;
        self.rect = Rect::new(parent.x, parent.y, 440, 400);
        self.center_on_parent();
    }

    pub fn hide(&mut self) {
        self.open = false;
    }

    fn center_on_parent(&mut self) {
        self.rect.x = self.parent.x + (self.parent.w - self.rect.w) / 2;
        self.rect.y = self.parent.y + (self.parent.h - self.rect.h) / 2;
    }

    fn fitted_size(theme: &Theme) -> (i32, i32) {
        let w = 460;
        let pad = 16;
        let section_gap = 10;
        let btn_h = (theme.toolbar_height - 6).max(22);
        let label_step = theme.row_height + 2;
        let row_step = btn_h + 6;
        let content_w = w - 4 - 32;
        let theme_btn_w = 92;
        let theme_gap = 6;
        let per_row = ((content_w + theme_gap) / (theme_btn_w + theme_gap)).max(1);
        let theme_rows = ((ThemeId::ALL.len() as i32) + per_row - 1) / per_row;

        let mut h = 4 + theme.title_height + 10;
        h += label_step + theme_rows * row_step + section_gap;
        h += label_step + row_step + section_gap;
        // Window: classic chrome + visual effects + compact
        h += label_step + row_step + row_step + row_step + section_gap;
        // Defaults rows including conflict policy + details pane + confirm drag
        h += label_step + row_step * 10;
        // Media: auto-mount + auto-open
        h += section_gap + label_step + row_step * 2;
        h += pad;
        (w, h)
    }

    fn relayout(&mut self, theme: &Theme) {
        let (w, h) = Self::fitted_size(theme);
        if self.rect.w != w || self.rect.h != h {
            self.rect.w = w;
            self.rect.h = h;
            self.center_on_parent();
        }
    }

    pub fn hit(&self, x: i32, y: i32) -> SettingsHit {
        if !self.open || !self.rect.contains(x, y) {
            return SettingsHit::None;
        }
        if self.close.hit(x, y) {
            return SettingsHit::Close;
        }
        for (id, btn) in &self.theme_btns {
            if btn.hit(x, y) {
                return SettingsHit::Theme(*id);
            }
        }
        for (scale, btn) in &self.scale_btns {
            if btn.hit(x, y) {
                return SettingsHit::IconScale(*scale);
            }
        }
        if self.chrome_btn.hit(x, y) {
            return SettingsHit::ToggleClassicChrome;
        }
        if self.effects_btn.hit(x, y) {
            return SettingsHit::ToggleVisualEffects;
        }
        if self.compact_btn.hit(x, y) {
            return SettingsHit::ToggleCompactView;
        }
        if self.hidden_btn.hit(x, y) {
            return SettingsHit::ToggleHidden;
        }
        if self.preview_btn.hit(x, y) {
            return SettingsHit::TogglePreview;
        }
        if self.details_btn.hit(x, y) {
            return SettingsHit::ToggleDetailsPane;
        }
        if self.side_btn.hit(x, y) {
            return SettingsHit::ToggleSidePane;
        }
        if self.restore_btn.hit(x, y) {
            return SettingsHit::ToggleRestoreTabs;
        }
        if self.startup_btn.hit(x, y) {
            return SettingsHit::CycleStartupLocation;
        }
        if self.single_click_btn.hit(x, y) {
            return SettingsHit::ToggleSingleClick;
        }
        if self.confirm_delete_btn.hit(x, y) {
            return SettingsHit::ToggleConfirmDelete;
        }
        if self.confirm_drag_btn.hit(x, y) {
            return SettingsHit::ToggleConfirmDrag;
        }
        if self.conflict_btn.hit(x, y) {
            return SettingsHit::CycleConflictPolicy;
        }
        if self.auto_mount_btn.hit(x, y) {
            return SettingsHit::ToggleAutoMount;
        }
        if self.media_auto_open_btn.hit(x, y) {
            return SettingsHit::CycleMediaAutoOpen;
        }
        SettingsHit::None
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>, settings: &Settings) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        let theme = frame.theme;
        self.relayout(&theme);

        frame.modal_dim(Rect::new(
            self.parent.x,
            self.parent.y,
            self.parent.w,
            self.parent.h,
        ));
        frame.panel(self.rect, theme.chrome);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);

        let title = Rect::new(inner.x + 2, inner.y + 2, inner.w - 4, theme.title_height);
        frame.title_bar(title, true);
        let btn = theme.title_height - 4;
        self.close.rect = Rect::new(title.x + title.w - btn - 2, title.y + 2, btn, btn);
        self.close.label = "X".into();
        self.close.draw(frame)?;
        frame.text(
            "Settings",
            title.x + 8,
            title.y + (theme.title_height - theme.font_size as i32) / 2,
            theme.title.text,
        )?;

        let btn_h = (theme.toolbar_height - 6).max(22);
        let label_step = theme.row_height + 2;
        let row_step = btn_h + 6;
        let section_gap = 10;
        let x = inner.x + 16;
        let mut y = title.y + theme.title_height + 10;
        let content_w = (inner.w - 32).max(80);

        frame.text("Theme", x, y, theme.text)?;
        y += label_step;
        let mut bx = x;
        let theme_btn_w = 92;
        for (id, btn) in &mut self.theme_btns {
            if bx + theme_btn_w > x + content_w && bx > x {
                bx = x;
                y += row_step;
            }
            btn.rect = Rect::new(bx, y, theme_btn_w, btn_h);
            btn.label = short_theme(*id).into();
            btn.pressed = settings.theme_id == *id;
            btn.draw(frame)?;
            bx += theme_btn_w + 6;
        }
        y += row_step + section_gap;

        frame.text("Icon size (Icons view)", x, y, theme.text)?;
        y += label_step;
        bx = x;
        for (scale, btn) in &mut self.scale_btns {
            btn.rect = Rect::new(bx, y, 48, btn_h);
            btn.label = format!("{scale}×");
            btn.pressed = settings.icon_scale == *scale;
            btn.draw(frame)?;
            bx += 54;
        }
        y += row_step + section_gap;

        frame.text("Window", x, y, theme.text)?;
        y += label_step;
        self.chrome_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.chrome_btn.label = if settings.classic_window_chrome {
            "[x] Classic framed chrome (window-in-window)".into()
        } else {
            "[ ] Classic framed chrome (use OS frame only)".into()
        };
        self.chrome_btn.pressed = false;
        self.chrome_btn.draw(frame)?;
        y += row_step;
        self.effects_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.effects_btn.label = if settings.visual_effects {
            "[x] Visual effects (gloss, shadows, dim)".into()
        } else {
            "[ ] Visual effects (gloss, shadows, dim)".into()
        };
        self.effects_btn.pressed = false;
        self.effects_btn.draw(frame)?;
        y += row_step;
        self.compact_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.compact_btn.label = if settings.compact_view {
            "[x] Compact / dense list spacing".into()
        } else {
            "[ ] Compact / dense list spacing".into()
        };
        self.compact_btn.pressed = false;
        self.compact_btn.draw(frame)?;
        y += row_step + section_gap;

        frame.text("Defaults", x, y, theme.text)?;
        y += label_step;
        self.hidden_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.hidden_btn.label = if settings.show_hidden_default {
            "[x] Show hidden files".into()
        } else {
            "[ ] Show hidden files".into()
        };
        self.hidden_btn.draw(frame)?;
        y += row_step;
        self.preview_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.preview_btn.label = if settings.preview_default {
            "[x] Preview pane open by default".into()
        } else {
            "[ ] Preview pane open by default".into()
        };
        self.preview_btn.draw(frame)?;
        y += row_step;
        self.details_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.details_btn.label = if settings.details_pane_visible {
            "[x] Details pane visible (Alt+Shift+P)".into()
        } else {
            "[ ] Details pane visible (Alt+Shift+P)".into()
        };
        self.details_btn.draw(frame)?;
        y += row_step;
        self.side_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.side_btn.label = if settings.side_pane_visible {
            "[x] Navigation pane visible (F9)".into()
        } else {
            "[ ] Navigation pane visible (F9)".into()
        };
        self.side_btn.draw(frame)?;
        y += row_step;
        self.restore_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.restore_btn.label = if settings.restore_tabs {
            "[x] Restore last session tabs".into()
        } else {
            "[ ] Restore last session tabs".into()
        };
        self.restore_btn.draw(frame)?;
        y += row_step;
        self.startup_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.startup_btn.label = format!(
            "Startup location: {} (click to cycle)",
            settings.startup_location_label()
        );
        self.startup_btn.pressed = false;
        self.startup_btn.draw(frame)?;
        y += row_step;
        self.single_click_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.single_click_btn.label = if settings.single_click_open {
            "[x] Single-click to open".into()
        } else {
            "[ ] Single-click to open (else double-click)".into()
        };
        self.single_click_btn.pressed = false;
        self.single_click_btn.draw(frame)?;
        y += row_step;
        self.confirm_delete_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.confirm_delete_btn.label = if settings.confirm_delete {
            "[x] Confirm permanent delete / empty trash".into()
        } else {
            "[ ] Confirm permanent delete / empty trash".into()
        };
        self.confirm_delete_btn.pressed = false;
        self.confirm_delete_btn.draw(frame)?;
        y += row_step;
        self.confirm_drag_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.confirm_drag_btn.label = if settings.confirm_drag {
            "[x] Confirm drag-and-drop move / copy / trash".into()
        } else {
            "[ ] Confirm drag-and-drop move / copy / trash".into()
        };
        self.confirm_drag_btn.pressed = false;
        self.confirm_drag_btn.draw(frame)?;
        y += row_step;
        self.conflict_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.conflict_btn.label = format!(
            "On name conflict: {} (click to cycle)",
            settings.conflict_policy.label()
        );
        self.conflict_btn.pressed = false;
        self.conflict_btn.draw(frame)?;
        y += row_step + section_gap;

        frame.text("Removable media", x, y, theme.text)?;
        y += label_step;
        self.auto_mount_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.auto_mount_btn.label = if settings.auto_mount {
            "[x] Auto-mount removable media".into()
        } else {
            "[ ] Auto-mount removable media".into()
        };
        self.auto_mount_btn.pressed = false;
        self.auto_mount_btn.draw(frame)?;
        y += row_step;
        self.media_auto_open_btn.rect = Rect::new(x, y, content_w, btn_h);
        self.media_auto_open_btn.label = format!(
            "On media insert: {} (click to cycle)",
            settings.media_auto_open_policy().label()
        );
        self.media_auto_open_btn.pressed = false;
        self.media_auto_open_btn.draw(frame)?;

        Ok(())
    }
}

fn short_theme(id: ThemeId) -> &'static str {
    match id {
        ThemeId::Win95 => "Win95",
        ThemeId::Win98 => "Win98",
        ThemeId::WinXpLuna => "Luna",
        ThemeId::Win7Aero => "Aero",
        ThemeId::ModernLight => "Light",
        ThemeId::ModernDark => "Dark",
    }
}
