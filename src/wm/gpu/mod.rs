//! Hardware-accelerated chrome painting for native fuckWM / dwm.
//!
//! - Linux: EGL + OpenGL ES 2 on a shared Xlib/xcb connection (Xephyr+Glamor OK)
//! - Windows: WGL + OpenGL on HWND
//!
//! Falls back to software X11/GDI fills when GPU init fails.

use anyhow::Result;
use sdl2::pixels::Color;

#[cfg(unix)]
mod egl_xcb;
#[cfg(windows)]
mod wgl;

#[cfg(unix)]
pub use egl_xcb::{open_shared_display, SharedX11};

#[derive(Clone, Copy, Debug)]
pub struct GpuColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl GpuColor {
    pub fn from_sdl(c: Color) -> Self {
        Self {
            r: c.r as f32 / 255.0,
            g: c.g as f32 / 255.0,
            b: c.b as f32 / 255.0,
            a: c.a as f32 / 255.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub u64);

pub struct Gpu {
    #[cfg(unix)]
    inner: egl_xcb::EglXcb,
    #[cfg(windows)]
    inner: wgl::WglHost,
    pub active: bool,
}

impl Gpu {
    #[cfg(unix)]
    pub fn from_shared(shared: &SharedX11) -> Result<Self> {
        match egl_xcb::EglXcb::new(shared) {
            Ok(inner) => {
                eprintln!("fuckWM: GPU accel ON (EGL/GLES2)");
                Ok(Self {
                    inner,
                    active: true,
                })
            }
            Err(err) => {
                eprintln!("fuckWM: GPU accel OFF ({err}); using software fills");
                Err(err)
            }
        }
    }

    #[cfg(windows)]
    pub fn connect_win32() -> Result<Self> {
        match wgl::WglHost::new() {
            Ok(inner) => {
                eprintln!("dwm: GPU accel ON (WGL/OpenGL)");
                Ok(Self {
                    inner,
                    active: true,
                })
            }
            Err(err) => {
                eprintln!("dwm: GPU accel OFF ({err}); using GDI fills");
                Err(err)
            }
        }
    }

    #[cfg(unix)]
    pub fn visual_id(&self) -> Option<u32> {
        Some(self.inner.visual_id)
    }

    #[cfg(unix)]
    pub fn create_window_surface(&mut self, xcb_window: u32, w: u32, h: u32) -> Result<SurfaceId> {
        self.inner.create_surface(xcb_window, w, h)
    }

    #[cfg(windows)]
    pub fn create_hwnd_surface(
        &mut self,
        hwnd: windows_sys::Win32::Foundation::HWND,
        w: u32,
        h: u32,
    ) -> Result<SurfaceId> {
        self.inner.create_surface(hwnd, w, h)
    }

    pub fn resize(&mut self, id: SurfaceId, w: u32, h: u32) -> Result<()> {
        #[cfg(unix)]
        {
            self.inner.resize(id, w, h)
        }
        #[cfg(windows)]
        {
            self.inner.resize(id, w, h)
        }
    }

    pub fn clear(&mut self, id: SurfaceId, color: GpuColor) -> Result<()> {
        #[cfg(unix)]
        {
            self.inner.clear(id, color)
        }
        #[cfg(windows)]
        {
            self.inner.clear(id, color)
        }
    }

    pub fn clear_and_rects(
        &mut self,
        id: SurfaceId,
        bg: GpuColor,
        rects: &[(f32, f32, f32, f32, GpuColor)],
    ) -> Result<()> {
        #[cfg(unix)]
        {
            self.inner.clear_and_rects(id, bg, rects)
        }
        #[cfg(windows)]
        {
            self.inner.clear_and_rects(id, bg, rects)
        }
    }

    pub fn destroy_surface(&mut self, id: SurfaceId) {
        #[cfg(unix)]
        {
            self.inner.destroy_surface(id);
        }
        #[cfg(windows)]
        {
            self.inner.destroy_surface(id);
        }
    }
}
