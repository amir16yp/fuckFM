use crate::browser::Location;
use crate::platform::trash_label;
use crate::render::{Frame, Rect};
use crate::ui::widgets::ButtonIcon;
use crate::ui::Button;
use anyhow::Result;
use std::path::{Component, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeHit {
    None,
    Close,
    Settings,
    ViewCycle,
    PreviewToggle,
    DetailsToggle,
    Up,
    Back,
    BackMenu,
    Forward,
    ForwardMenu,
    NewTab,
    Address,
    Search,
    PathCrumb(usize),
}

#[derive(Clone, Debug)]
pub enum CrumbTarget {
    ThisPc,
    Trash,
    Path(PathBuf),
}

#[derive(Clone, Debug)]
pub struct PathCrumb {
    pub label: String,
    pub target: CrumbTarget,
    pub rect: Rect,
}

pub struct Layout {
    pub tabs: Rect,
    pub side: Rect,
    pub list: Rect,
    pub preview: Rect,
    pub details: Rect,
}

pub struct WindowChrome {
    pub window: Rect,
    pub title: String,
    pub address: String,
    pub address_focused: bool,
    pub address_rect: Rect,
    pub search: String,
    pub search_focused: bool,
    pub search_rect: Rect,
    pub crumbs: Vec<PathCrumb>,
    pub pathbar_height: i32,
    pub pathbar_rect: Rect,
    pub status: String,
    pub view_label: String,
    /// When false, skip custom title bar / outer framed window (OS decorations only).
    pub classic_framed: bool,
    pub close: Button,
    pub settings_btn: Button,
    pub view_btn: Button,
    pub preview_btn: Button,
    pub up_btn: Button,
    pub back_btn: Button,
    pub back_menu_btn: Button,
    pub forward_btn: Button,
    pub forward_menu_btn: Button,
    pub new_tab_btn: Button,
    pub side_pane_width: i32,
    pub preview_width: i32,
    pub preview_visible: bool,
    pub details_visible: bool,
    pub details_height: i32,
    pub details_btn: Button,
    pub side_visible: bool,
    pub tab_strip_height: i32,
}

impl WindowChrome {
    pub fn new(window: Rect, title: impl Into<String>) -> Self {
        let mut chrome = Self {
            window,
            title: title.into(),
            address: String::new(),
            address_focused: false,
            address_rect: Rect::default(),
            search: String::new(),
            search_focused: false,
            search_rect: Rect::default(),
            crumbs: Vec::new(),
            pathbar_height: 0,
            pathbar_rect: Rect::default(),
            status: String::new(),
            view_label: "List".into(),
            classic_framed: true,
            close: Button::new(Rect::default(), "X"),
            settings_btn: Button::new(Rect::default(), "Settings"),
            view_btn: Button::new(Rect::default(), "View"),
            preview_btn: Button::new(Rect::default(), "Prev"),
            up_btn: Button::with_icon(Rect::default(), ButtonIcon::UpArrow),
            back_btn: Button::with_icon(Rect::default(), ButtonIcon::BackArrow),
            back_menu_btn: Button::new(Rect::default(), "▾"),
            forward_btn: Button::with_icon(Rect::default(), ButtonIcon::ForwardArrow),
            forward_menu_btn: Button::new(Rect::default(), "▾"),
            new_tab_btn: Button::new(Rect::default(), "+"),
            side_pane_width: 150,
            preview_width: 260,
            preview_visible: false,
            details_visible: false,
            details_height: 120,
            details_btn: Button::new(Rect::default(), "Det"),
            side_visible: true,
            tab_strip_height: 26,
        };
        chrome.relayout();
        chrome
    }

    pub fn sync_path_crumbs(&mut self, location: &Location, row_h: i32) {
        self.crumbs = build_path_crumbs(location);
        self.pathbar_height = if self.crumbs.is_empty() {
            0
        } else {
            (row_h + 2).max(20)
        };
    }

    pub fn set_window(&mut self, window: Rect) {
        self.window = window;
        self.relayout();
    }

    pub fn relayout(&mut self) {}

    fn client_origin(&self, border: i32, title_h: i32) -> (i32, i32, i32, i32) {
        if self.classic_framed {
            let client = self.window.shrink(border);
            (
                client.x + 2,
                client.y + 2 + title_h + 4,
                client.w - 4,
                client.h - 2 - title_h - 4,
            )
        } else {
            (
                self.window.x,
                self.window.y,
                self.window.w,
                self.window.h,
            )
        }
    }

    pub fn layout(
        &self,
        theme_title: i32,
        theme_toolbar: i32,
        theme_status: i32,
        border: i32,
    ) -> Layout {
        let (cx, cy, cw, ch) = self.client_origin(border, theme_title);
        let pathbar_gap = if self.pathbar_height > 0 { 2 } else { 0 };
        let top = theme_toolbar + 4 + self.pathbar_height + pathbar_gap;
        let bottom = theme_status + 4;
        let body = Rect::new(cx, cy + top, cw.max(0), (ch - top - bottom).max(0));
        let tabs = Rect::new(body.x, body.y, body.w, self.tab_strip_height);
        let content_y = tabs.y + tabs.h + 2;
        let content_h = (body.h - tabs.h - 2).max(0);
        let side_w = if self.side_visible {
            self.side_pane_width
        } else {
            0
        };
        let details_h = if self.details_visible {
            self.details_height.min(content_h / 2).max(0)
        } else {
            0
        };
        let details_gap = if details_h > 0 { 4 } else { 0 };
        let upper_h = (content_h - details_h - details_gap).max(0);
        let side = Rect::new(body.x, content_y, side_w, upper_h);
        let preview_w = if self.preview_visible {
            self.preview_width
        } else {
            0
        };
        let gap_after_side = if side_w > 0 { 4 } else { 0 };
        let list_w = (body.w
            - side_w
            - gap_after_side
            - if preview_w > 0 { preview_w + 4 } else { 0 })
        .max(0);
        let list = Rect::new(side.x + side.w + gap_after_side, content_y, list_w, upper_h);
        let preview = Rect::new(
            list.x + list.w + if preview_w > 0 { 4 } else { 0 },
            content_y,
            preview_w,
            upper_h,
        );
        let details = Rect::new(
            list.x,
            content_y + upper_h + details_gap,
            (list_w + if preview_w > 0 { preview_w + 4 } else { 0 }).max(0),
            details_h,
        );
        Layout {
            tabs,
            side,
            list,
            preview,
            details,
        }
    }

    pub fn hit(&self, x: i32, y: i32) -> ChromeHit {
        if self.classic_framed && self.close.hit(x, y) {
            ChromeHit::Close
        } else if self.settings_btn.hit(x, y) {
            ChromeHit::Settings
        } else if self.preview_btn.hit(x, y) {
            ChromeHit::PreviewToggle
        } else if self.details_btn.hit(x, y) {
            ChromeHit::DetailsToggle
        } else if self.view_btn.hit(x, y) {
            ChromeHit::ViewCycle
        } else if self.up_btn.hit(x, y) {
            ChromeHit::Up
        } else if self.back_menu_btn.hit(x, y) {
            ChromeHit::BackMenu
        } else if self.back_btn.hit(x, y) {
            ChromeHit::Back
        } else if self.forward_menu_btn.hit(x, y) {
            ChromeHit::ForwardMenu
        } else if self.forward_btn.hit(x, y) {
            ChromeHit::Forward
        } else if self.new_tab_btn.hit(x, y) {
            ChromeHit::NewTab
        } else if self.address_rect.contains(x, y) {
            ChromeHit::Address
        } else if self.search_rect.contains(x, y) {
            ChromeHit::Search
        } else if let Some(i) = self
            .crumbs
            .iter()
            .position(|c| c.rect.w > 0 && c.rect.contains(x, y))
        {
            ChromeHit::PathCrumb(i)
        } else {
            ChromeHit::None
        }
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        let theme = frame.theme;
        let border = theme.border_width;
        let title_h = theme.title_height;

        if self.classic_framed {
            // Desktop + floating framed window (classic look).
            frame.fill(self.window, theme.desktop);
            frame.raised(self.window, theme.chrome);
            let inner = self.window.shrink(1);
            frame.fill(inner, theme.window);
            let client_frame = self.window.shrink(border);
            frame.raised(client_frame, theme.chrome);

            let title = Rect::new(
                client_frame.x + 2,
                client_frame.y + 2,
                client_frame.w - 4,
                title_h,
            );
            frame.title_bar(title, true);
            let btn = title_h - 4;
            self.close.rect = Rect::new(title.x + title.w - btn - 2, title.y + 2, btn, btn);
            self.close.label = "X".into();
            self.close.draw(frame)?;
            let title_color = theme.title.text;
            let max_title_w = title.w - btn - 12;
            let mut shown = self.title.clone();
            while frame.text_width(&shown) > max_title_w && shown.len() > 1 {
                shown.pop();
            }
            frame.text(
                &shown,
                title.x + 4,
                title.y + (title_h - theme.font_size as i32) / 2,
                title_color,
            )?;
            let toolbar_y = title.y + title_h + 4;
            self.draw_toolbar(frame, client_frame.x + 4, toolbar_y, client_frame)?;
            self.draw_pathbar(
                frame,
                client_frame.x + 4,
                toolbar_y + theme.toolbar_height,
                client_frame.w - 8,
            )?;
            self.draw_status(frame, client_frame)?;
        } else {
            // OS frame only — fill client flat.
            frame.fill(self.window, theme.window);
            let toolbar_y = self.window.y + 4;
            self.draw_toolbar(frame, self.window.x + 4, toolbar_y, self.window)?;
            self.draw_pathbar(
                frame,
                self.window.x + 4,
                toolbar_y + theme.toolbar_height,
                self.window.w - 8,
            )?;
            self.draw_status_native(frame)?;
        }

        Ok(())
    }

    fn draw_pathbar(
        &mut self,
        frame: &mut Frame<'_>,
        x: i32,
        y: i32,
        w: i32,
    ) -> Result<()> {
        for crumb in &mut self.crumbs {
            crumb.rect = Rect::default();
        }
        if self.pathbar_height <= 0 || self.crumbs.is_empty() || w <= 0 {
            self.pathbar_rect = Rect::default();
            return Ok(());
        }

        let theme = frame.theme;
        self.pathbar_rect = Rect::new(x, y, w, self.pathbar_height);
        frame.fill(self.pathbar_rect, theme.address_bar);
        frame.flat_border(self.pathbar_rect, theme.border);

        let font_h = theme.font_size as i32;
        let pad = 4;
        let text_y = y + (self.pathbar_height - font_h) / 2;
        let right = x + w - 4;
        let mut cx = x + 6;
        let last = self.crumbs.len().saturating_sub(1);
        let sep = "›";
        let sep_w = frame.text_width(sep);

        for i in 0..self.crumbs.len() {
            if i > 0 {
                if cx + sep_w + 4 >= right {
                    break;
                }
                frame.text(sep, cx, text_y, theme.text_disabled)?;
                cx += sep_w + 4;
            }

            let label = self.crumbs[i].label.clone();
            let tw = frame.text_width(&label);
            let cw = (tw + pad * 2).max(pad * 2 + 8);
            if cx + cw > right {
                break;
            }

            let rect = Rect::new(cx, y + 1, cw, self.pathbar_height - 2);
            if i == last {
                frame.fill(rect, theme.selection);
                frame.text(
                    &label,
                    rect.x + pad,
                    text_y,
                    theme.selection_text,
                )?;
            } else {
                frame.text(&label, rect.x + pad, text_y, theme.text)?;
            }
            self.crumbs[i].rect = rect;
            cx += cw + 2;
        }
        Ok(())
    }

    fn draw_toolbar(
        &mut self,
        frame: &mut Frame<'_>,
        start_x: i32,
        toolbar_y: i32,
        bounds: Rect,
    ) -> Result<()> {
        let theme = frame.theme;
        let nav_h = theme.toolbar_height - 6;
        let drop_w = (nav_h / 2).max(12);
        let mut x = start_x;
        self.back_btn.rect = Rect::new(x, toolbar_y, nav_h, nav_h);
        self.back_btn.icon = ButtonIcon::BackArrow;
        x += nav_h;
        self.back_menu_btn.rect = Rect::new(x, toolbar_y, drop_w, nav_h);
        self.back_menu_btn.label = "▾".into();
        x += drop_w + 4;
        self.forward_btn.rect = Rect::new(x, toolbar_y, nav_h, nav_h);
        self.forward_btn.icon = ButtonIcon::ForwardArrow;
        x += nav_h;
        self.forward_menu_btn.rect = Rect::new(x, toolbar_y, drop_w, nav_h);
        self.forward_menu_btn.label = "▾".into();
        x += drop_w + 4;
        self.up_btn.rect = Rect::new(x, toolbar_y, nav_h, nav_h);
        self.up_btn.icon = ButtonIcon::UpArrow;
        x += nav_h + 4;
        self.new_tab_btn.rect = Rect::new(x, toolbar_y, nav_h, nav_h);
        x += nav_h + 8;

        let right = bounds.x + bounds.w;
        self.settings_btn.rect = Rect::new(right - 80, toolbar_y, 72, nav_h);
        self.settings_btn.label = "Settings".into();
        self.preview_btn.rect = Rect::new(right - 80 - 52, toolbar_y, 48, nav_h);
        self.preview_btn.label = if self.preview_visible { "Prev*" } else { "Prev" }.into();
        self.details_btn.rect = Rect::new(right - 80 - 52 - 48, toolbar_y, 44, nav_h);
        self.details_btn.label = if self.details_visible { "Det*" } else { "Det" }.into();
        self.view_btn.rect = Rect::new(right - 80 - 52 - 48 - 68, toolbar_y, 64, nav_h);
        self.view_btn.label = self.view_label.clone();

        self.back_btn.draw(frame)?;
        self.back_menu_btn.draw(frame)?;
        self.forward_btn.draw(frame)?;
        self.forward_menu_btn.draw(frame)?;
        self.up_btn.draw(frame)?;
        self.new_tab_btn.draw(frame)?;
        self.view_btn.draw(frame)?;
        self.details_btn.draw(frame)?;
        self.preview_btn.draw(frame)?;
        self.settings_btn.draw(frame)?;

        let search_w = 168.min((self.view_btn.rect.x - x - 16).max(0) / 3).max(100);
        let gap = 8;
        let addr_w = (self.view_btn.rect.x - x - gap - search_w - gap).max(40);
        self.address_rect = Rect::new(x, toolbar_y, addr_w, nav_h);
        self.search_rect = Rect::new(
            self.address_rect.x + self.address_rect.w + gap,
            toolbar_y,
            search_w,
            nav_h,
        );

        frame.sunken(self.address_rect, theme.sunken);
        frame.fill(self.address_rect.shrink(2), theme.address_bar);
        let addr_text = truncate_to_width(frame, &self.address, self.address_rect.w - 10);
        frame.text(
            &addr_text,
            self.address_rect.x + 4,
            self.address_rect.y + (self.address_rect.h - theme.font_size as i32) / 2,
            theme.text,
        )?;
        if self.address_focused {
            let caret_label =
                truncate_to_width(frame, &self.address, self.address_rect.w - 14);
            let caret_x = self.address_rect.x
                + 4
                + frame.text_width(&caret_label).min(self.address_rect.w - 10);
            frame.fill(
                Rect::new(
                    caret_x,
                    self.address_rect.y + 4,
                    1,
                    self.address_rect.h - 8,
                ),
                theme.text,
            );
        }

        frame.sunken(self.search_rect, theme.sunken);
        frame.fill(self.search_rect.shrink(2), theme.address_bar);
        let placeholder = self.search.is_empty() && !self.search_focused;
        let search_shown = if placeholder {
            "Search…".to_string()
        } else {
            truncate_to_width(frame, &self.search, self.search_rect.w - 14)
        };
        let search_color = if placeholder {
            theme.text_disabled
        } else {
            theme.text
        };
        frame.text(
            &search_shown,
            self.search_rect.x + 4,
            self.search_rect.y + (self.search_rect.h - theme.font_size as i32) / 2,
            search_color,
        )?;
        if self.search_focused {
            let caret_label =
                truncate_to_width(frame, &self.search, self.search_rect.w - 14);
            let caret_x = self.search_rect.x
                + 4
                + frame.text_width(&caret_label).min(self.search_rect.w - 10);
            frame.fill(
                Rect::new(
                    caret_x,
                    self.search_rect.y + 4,
                    1,
                    self.search_rect.h - 8,
                ),
                theme.text,
            );
        }
        Ok(())
    }

    fn draw_status(&mut self, frame: &mut Frame<'_>, client_frame: Rect) -> Result<()> {
        let theme = frame.theme;
        let status = Rect::new(
            client_frame.x + 2,
            client_frame.y + client_frame.h - theme.status_height - 2,
            client_frame.w - 4,
            theme.status_height,
        );
        frame.sunken(status, theme.chrome);
        frame.fill(status.shrink(2), theme.status_bar);
        frame.text(
            &self.status,
            status.x + 6,
            status.y + (status.h - theme.font_size as i32) / 2,
            theme.text,
        )?;
        Ok(())
    }

    fn draw_status_native(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        let theme = frame.theme;
        let status = Rect::new(
            self.window.x + 2,
            self.window.y + self.window.h - theme.status_height - 2,
            self.window.w - 4,
            theme.status_height,
        );
        frame.fill(status, theme.status_bar);
        frame.flat_border(status, theme.border);
        frame.text(
            &self.status,
            status.x + 6,
            status.y + (status.h - theme.font_size as i32) / 2,
            theme.text,
        )?;
        Ok(())
    }
}

fn build_path_crumbs(location: &Location) -> Vec<PathCrumb> {
    match location {
        Location::ThisPc => vec![PathCrumb {
            label: "This PC".into(),
            target: CrumbTarget::ThisPc,
            rect: Rect::default(),
        }],
        Location::Trash => vec![
            PathCrumb {
                label: "This PC".into(),
                target: CrumbTarget::ThisPc,
                rect: Rect::default(),
            },
            PathCrumb {
                label: trash_label().into(),
                target: CrumbTarget::Trash,
                rect: Rect::default(),
            },
        ],
        Location::Path(path) => {
            let mut crumbs = Vec::new();
            let mut accumulated = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::Prefix(prefix) => {
                        accumulated.push(prefix.as_os_str());
                    }
                    Component::RootDir => {
                        accumulated.push(component.as_os_str());
                        let label = if cfg!(windows) {
                            accumulated.display().to_string()
                        } else {
                            "/".into()
                        };
                        crumbs.push(PathCrumb {
                            label,
                            target: CrumbTarget::Path(accumulated.clone()),
                            rect: Rect::default(),
                        });
                    }
                    Component::Normal(name) => {
                        accumulated.push(name);
                        crumbs.push(PathCrumb {
                            label: name.to_string_lossy().into_owned(),
                            target: CrumbTarget::Path(accumulated.clone()),
                            rect: Rect::default(),
                        });
                    }
                    Component::CurDir | Component::ParentDir => {
                        accumulated.push(component.as_os_str());
                    }
                }
            }
            if crumbs.is_empty() {
                crumbs.push(PathCrumb {
                    label: path.display().to_string(),
                    target: CrumbTarget::Path(path.clone()),
                    rect: Rect::default(),
                });
            }
            crumbs
        }
    }
}

fn truncate_to_width(frame: &mut Frame<'_>, text: &str, max_w: i32) -> String {
    if frame.text_width(text) <= max_w {
        return text.to_string();
    }
    let mut out = text.to_string();
    while !out.is_empty() && frame.text_width(&format!("…{out}")) > max_w {
        out.remove(0);
    }
    format!("…{out}")
}