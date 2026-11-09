use crate::render::{Frame, Rect};
use crate::ui::Button;
use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    DeletePermanent,
    EmptyTrash,
    /// Pending payload lives on App (`pending_drag`).
    DragDrop,
    /// Pending device path on App (`pending_format`).
    FormatVolume,
    /// Pending mount path on App (`pending_media_open`).
    OpenMedia,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmHit {
    None,
    Yes,
    No,
}

pub struct ConfirmDialog {
    pub open: bool,
    pub rect: Rect,
    parent: Rect,
    message: String,
    pub action: ConfirmAction,
    yes: Button,
    no: Button,
}

impl ConfirmDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            rect: Rect::default(),
            parent: Rect::default(),
            message: String::new(),
            action: ConfirmAction::DeletePermanent,
            yes: Button::new(Rect::default(), "Yes"),
            no: Button::new(Rect::default(), "No"),
        }
    }

    pub fn show(&mut self, parent: Rect, action: ConfirmAction, message: impl Into<String>) {
        self.open = true;
        self.parent = parent;
        self.action = action;
        self.message = message.into();
        self.rect = Rect::new(0, 0, 420, 140);
        self.rect.x = parent.x + (parent.w - self.rect.w) / 2;
        self.rect.y = parent.y + (parent.h - self.rect.h) / 2;
    }

    pub fn hide(&mut self) {
        self.open = false;
    }

    pub fn hit(&self, x: i32, y: i32) -> ConfirmHit {
        if !self.open {
            return ConfirmHit::None;
        }
        if self.yes.hit(x, y) {
            return ConfirmHit::Yes;
        }
        if self.no.hit(x, y) {
            return ConfirmHit::No;
        }
        ConfirmHit::None
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

        frame.text("Confirm", inner.x + 12, inner.y + 12, theme.text)?;
        frame.text(
            &self.message,
            inner.x + 12,
            inner.y + 12 + theme.row_height + 4,
            theme.text,
        )?;

        let btn_w = 88;
        let btn_h = (theme.toolbar_height - 6).max(22);
        let by = inner.y + inner.h - btn_h - 14;
        self.yes.rect = Rect::new(inner.x + inner.w - btn_w * 2 - 24, by, btn_w, btn_h);
        self.no.rect = Rect::new(inner.x + inner.w - btn_w - 12, by, btn_w, btn_h);
        self.yes.label = "Yes".into();
        self.no.label = "No".into();
        self.yes.draw(frame)?;
        self.no.draw(frame)?;
        Ok(())
    }
}
