use crate::ops::ConflictChoice;
use crate::render::{Frame, Rect};
use crate::ui::Button;
use anyhow::Result;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictHit {
    None,
    Skip,
    Replace,
    Rename,
    ToggleAll,
}

pub struct ConflictDialog {
    pub open: bool,
    pub rect: Rect,
    parent: Rect,
    message: String,
    pub src: PathBuf,
    pub dest: PathBuf,
    pub apply_all: bool,
    skip: Button,
    replace: Button,
    rename: Button,
    all_btn: Button,
}

impl ConflictDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            rect: Rect::default(),
            parent: Rect::default(),
            message: String::new(),
            src: PathBuf::new(),
            dest: PathBuf::new(),
            apply_all: false,
            skip: Button::new(Rect::default(), "Skip"),
            replace: Button::new(Rect::default(), "Replace"),
            rename: Button::new(Rect::default(), "Rename"),
            all_btn: Button::new(Rect::default(), ""),
        }
    }

    pub fn show(&mut self, parent: Rect, src: PathBuf, dest: PathBuf) {
        self.open = true;
        self.parent = parent;
        self.src = src;
        self.dest = dest;
        let name = self
            .dest
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.dest.display().to_string());
        self.message = format!("“{name}” already exists in the destination.");
        self.rect = Rect::new(0, 0, 460, 170);
        self.rect.x = parent.x + (parent.w - self.rect.w) / 2;
        self.rect.y = parent.y + (parent.h - self.rect.h) / 2;
    }

    pub fn hide(&mut self) {
        self.open = false;
    }

    pub fn hit(&self, x: i32, y: i32) -> ConflictHit {
        if !self.open {
            return ConflictHit::None;
        }
        if self.skip.hit(x, y) {
            return ConflictHit::Skip;
        }
        if self.replace.hit(x, y) {
            return ConflictHit::Replace;
        }
        if self.rename.hit(x, y) {
            return ConflictHit::Rename;
        }
        if self.all_btn.hit(x, y) {
            return ConflictHit::ToggleAll;
        }
        ConflictHit::None
    }

    pub fn choice_from_hit(hit: ConflictHit) -> Option<ConflictChoice> {
        match hit {
            ConflictHit::Skip => Some(ConflictChoice::Skip),
            ConflictHit::Replace => Some(ConflictChoice::Replace),
            ConflictHit::Rename => Some(ConflictChoice::Rename),
            _ => None,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        if !self.open {
            return Ok(());
        }
        let theme = frame.theme;
        frame.modal_dim(self.parent);
        frame.panel(self.rect, theme.chrome);
        let inner = self.rect.shrink(2);
        frame.fill(inner, theme.window);

        frame.text("File conflict", inner.x + 12, inner.y + 12, theme.text)?;
        frame.text(
            &self.message,
            inner.x + 12,
            inner.y + 12 + theme.row_height + 4,
            theme.text,
        )?;

        let btn_h = (theme.toolbar_height - 6).max(22);
        let by = inner.y + inner.h - btn_h - 14;
        self.all_btn.rect = Rect::new(inner.x + 12, by, 140, btn_h);
        self.all_btn.label = if self.apply_all {
            "[x] Apply to all".into()
        } else {
            "[ ] Apply to all".into()
        };
        self.all_btn.draw(frame)?;

        let btn_w = 88;
        self.skip.rect = Rect::new(inner.x + inner.w - btn_w * 3 - 36, by, btn_w, btn_h);
        self.replace.rect = Rect::new(inner.x + inner.w - btn_w * 2 - 24, by, btn_w, btn_h);
        self.rename.rect = Rect::new(inner.x + inner.w - btn_w - 12, by, btn_w, btn_h);
        self.skip.label = "Skip".into();
        self.replace.label = "Replace".into();
        self.rename.label = "Rename".into();
        self.skip.draw(frame)?;
        self.replace.draw(frame)?;
        self.rename.draw(frame)?;
        Ok(())
    }
}
