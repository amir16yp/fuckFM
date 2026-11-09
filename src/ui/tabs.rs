use crate::render::{Frame, Rect};
use anyhow::Result;

#[derive(Clone, Debug)]
pub struct TabStrip {
    pub rect: Rect,
    pub titles: Vec<String>,
    pub active: usize,
    pub hover: Option<usize>,
    /// Tab being reordered (drawn raised / offset hint).
    pub drag_from: Option<usize>,
    /// Insert highlight while reordering or file-drop hover.
    pub drop_hover: Option<usize>,
    tab_width: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabHit {
    None,
    Tab(usize),
    New,
}

impl TabStrip {
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            titles: vec!["Home".into()],
            active: 0,
            hover: None,
            drag_from: None,
            drop_hover: None,
            tab_width: 120,
        }
    }

    pub fn set_titles(&mut self, titles: Vec<String>, active: usize) {
        self.titles = titles;
        self.active = active.min(self.titles.len().saturating_sub(1));
    }

    pub fn hit(&self, x: i32, y: i32) -> TabHit {
        if !self.rect.contains(x, y) {
            return TabHit::None;
        }
        let new_btn = self.new_button_rect();
        if new_btn.contains(x, y) {
            return TabHit::New;
        }
        for i in 0..self.titles.len() {
            if self.tab_rect(i).contains(x, y) {
                return TabHit::Tab(i);
            }
        }
        TabHit::None
    }

    pub fn hover_at(&mut self, x: i32, y: i32) {
        self.hover = match self.hit(x, y) {
            TabHit::Tab(i) => Some(i),
            _ => None,
        };
    }

    /// Index to insert/move a dragged tab to, based on pointer x (midpoints).
    pub fn reorder_index_at(&self, x: i32) -> usize {
        let n = self.titles.len();
        if n == 0 {
            return 0;
        }
        for i in 0..n {
            let r = self.tab_rect(i);
            if x < r.x + r.w / 2 {
                return i;
            }
        }
        n - 1
    }

    pub fn tab_rect(&self, index: usize) -> Rect {
        Rect::new(
            self.rect.x + 2 + index as i32 * (self.tab_width + 2),
            self.rect.y + 2,
            self.tab_width,
            self.rect.h - 4,
        )
    }

    fn new_button_rect(&self) -> Rect {
        let x = self.rect.x + 2 + self.titles.len() as i32 * (self.tab_width + 2);
        Rect::new(x, self.rect.y + 2, 28, self.rect.h - 4)
    }

    pub fn draw(&self, frame: &mut Frame<'_>) -> Result<()> {
        let theme = frame.theme;
        frame.fill(self.rect, theme.window);
        frame.flat_border(self.rect, theme.border);

        for (i, title) in self.titles.iter().enumerate() {
            let rect = self.tab_rect(i);
            let active = i == self.active;
            let drop = self.drop_hover == Some(i);
            let dragging = self.drag_from == Some(i);
            if active || dragging {
                frame.raised(rect, theme.button);
            } else if drop || self.hover == Some(i) {
                frame.fill(rect, theme.selection);
                frame.flat_border(rect, theme.border);
            } else {
                frame.sunken(rect, theme.chrome);
            }
            let color = if active || drop || dragging {
                if drop && !active {
                    theme.selection_text
                } else {
                    theme.text
                }
            } else {
                theme.text_disabled
            };
            let label = truncate_to_width(frame, title, rect.w - 10);
            frame.text(
                &label,
                rect.x + 6,
                rect.y + (rect.h - theme.font_size as i32) / 2,
                color,
            )?;
        }

        let plus = self.new_button_rect();
        frame.raised(plus, theme.button);
        frame.text(
            "+",
            plus.x + 8,
            plus.y + (plus.h - theme.font_size as i32) / 2,
            theme.text,
        )?;
        Ok(())
    }
}

fn truncate_to_width(frame: &mut Frame<'_>, text: &str, max_w: i32) -> String {
    if frame.text_width(text) <= max_w {
        return text.to_string();
    }
    let mut out = text.to_string();
    while out.len() > 1 && frame.text_width(&format!("{out}…")) > max_w {
        out.pop();
    }
    format!("{out}…")
}
