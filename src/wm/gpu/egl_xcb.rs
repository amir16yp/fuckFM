//! EGL + GLES2 bound to the same X connection as the WM.
//!
//! Opens Xlib Display, shares it via `XGetXCBConnection` (xcb owns the queue),
//! then creates an EGL display with `EGL_PLATFORM_X11` (fallback:
//! `EGL_PLATFORM_XCB`). Surfaces must use windows from that connection.

use super::{GpuColor, SurfaceId};
use anyhow::{bail, Context, Result};
use khronos_egl as egl;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_void;
use std::ptr;
use x11_dl::xlib;
use x11_dl::xlib_xcb::{XEventQueueOwner, Xlib_xcb};
use x11rb::xcb_ffi::XCBConnection;

const EGL_PLATFORM_X11_EXT: egl::Enum = 0x31D5;
const EGL_PLATFORM_XCB_EXT: egl::Enum = 0x31DC;
const EGL_PLATFORM_XCB_SCREEN_EXT: egl::Attrib = 0x31DE;

pub struct SharedX11 {
    pub conn: XCBConnection,
    pub screen_num: usize,
    /// Kept alive for EGL_PLATFORM_X11; xcb owns the event queue.
    #[allow(dead_code)]
    xlib_dpy: *mut xlib::Display,
    _xlib: xlib::Xlib,
    _xlib_xcb: Xlib_xcb,
}

pub struct EglXcb {
    egl: egl::DynamicInstance<egl::EGL1_4>,
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
    pub visual_id: u32,
    surfaces: HashMap<SurfaceId, Surface>,
    next_id: u64,
}

struct Surface {
    egl_surface: egl::Surface,
    w: u32,
    h: u32,
}

/// Open Xlib+xcb shared connection (required for EGL window surfaces).
pub fn open_shared_display() -> Result<SharedX11> {
    let xlib = xlib::Xlib::open().context("load libX11")?;
    let xlib_xcb = Xlib_xcb::open().context("load libX11-xcb")?;
    let display_name = std::env::var("DISPLAY").ok();
    let c_name = display_name
        .as_ref()
        .map(|s| CString::new(s.as_str()))
        .transpose()
        .context("DISPLAY")?;
    let ptr = c_name
        .as_ref()
        .map(|c| c.as_ptr())
        .unwrap_or(ptr::null());
    let xlib_dpy = unsafe { (xlib.XOpenDisplay)(ptr) };
    if xlib_dpy.is_null() {
        bail!("XOpenDisplay failed for DISPLAY={:?}", display_name);
    }
    let xcb_ptr = unsafe { (xlib_xcb.XGetXCBConnection)(xlib_dpy) };
    if xcb_ptr.is_null() {
        unsafe { (xlib.XCloseDisplay)(xlib_dpy) };
        bail!("XGetXCBConnection returned null");
    }
    unsafe {
        (xlib_xcb.XSetEventQueueOwner)(xlib_dpy, XEventQueueOwner::XCBOwnsEventQueue)
    };
    let screen_num = unsafe { (xlib.XDefaultScreen)(xlib_dpy) as usize };
    // false = we do not own/close the xcb connection (Xlib still owns Display).
    let conn = unsafe { XCBConnection::from_raw_xcb_connection(xcb_ptr.cast(), false) }
        .context("wrap xcb connection")?;
    Ok(SharedX11 {
        conn,
        screen_num,
        xlib_dpy,
        _xlib: xlib,
        _xlib_xcb: xlib_xcb,
    })
}

impl EglXcb {
    pub fn new(shared: &SharedX11) -> Result<Self> {
        let egl = unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required() }
            .map_err(|e| anyhow::anyhow!("load libEGL: {e}"))?;

        let display = unsafe {
            // Prefer X11 platform with the live Display* (same connection as WM).
            match get_platform_display(
                &egl,
                EGL_PLATFORM_X11_EXT,
                shared.xlib_dpy.cast(),
                &[egl::ATTRIB_NONE],
            ) {
                Ok(dpy) => dpy,
                Err(err_x11) => {
                    let xcb = shared.conn.get_raw_xcb_connection();
                    let attribs = [
                        EGL_PLATFORM_XCB_SCREEN_EXT,
                        shared.screen_num as egl::Attrib,
                        egl::ATTRIB_NONE,
                    ];
                    get_platform_display(&egl, EGL_PLATFORM_XCB_EXT, xcb.cast(), &attribs)
                        .with_context(|| {
                            format!("EGL_PLATFORM_X11 ({err_x11}) and EGL_PLATFORM_XCB failed")
                        })?
                }
            }
        };
        egl.initialize(display)
            .map_err(|e| anyhow::anyhow!("eglInitialize: {e:?}"))?;
        egl.bind_api(egl::OPENGL_ES_API)
            .map_err(|e| anyhow::anyhow!("eglBindAPI: {e:?}"))?;

        let mut configs = Vec::with_capacity(8);
        let config_attribs = [
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES2_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            0,
            egl::NONE,
        ];
        egl.choose_config(display, &config_attribs, &mut configs)
            .map_err(|e| anyhow::anyhow!("eglChooseConfig: {e:?}"))?;
        let config = *configs.first().context("no EGL config")?;

        let visual_id = egl
            .get_config_attrib(display, config, egl::NATIVE_VISUAL_ID)
            .map_err(|e| anyhow::anyhow!("NATIVE_VISUAL_ID: {e:?}"))? as u32;

        let ctx_attribs = [egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE];
        let context = egl
            .create_context(display, config, None, &ctx_attribs)
            .map_err(|e| anyhow::anyhow!("eglCreateContext: {e:?}"))?;

        Ok(Self {
            egl,
            display,
            config,
            context,
            visual_id,
            surfaces: HashMap::new(),
            next_id: 1,
        })
    }

    pub fn create_surface(&mut self, xcb_window: u32, w: u32, h: u32) -> Result<SurfaceId> {
        let egl_surface = unsafe {
            self.egl
                .create_window_surface(
                    self.display,
                    self.config,
                    xcb_window as egl::NativeWindowType,
                    None,
                )
                .map_err(|e| anyhow::anyhow!("eglCreateWindowSurface: {e:?}"))?
        };
        self.egl
            .make_current(
                self.display,
                Some(egl_surface),
                Some(egl_surface),
                Some(self.context),
            )
            .map_err(|e| anyhow::anyhow!("eglMakeCurrent: {e:?}"))?;
        load_gl(|name| match CString::new(name) {
            Ok(c) => self
                .egl
                .get_proc_address(c.to_str().unwrap_or(""))
                .map(|f| f as *const c_void)
                .unwrap_or(std::ptr::null()),
            Err(_) => std::ptr::null(),
        });
        let id = SurfaceId(self.next_id);
        self.next_id += 1;
        self.surfaces.insert(
            id,
            Surface {
                egl_surface,
                w: w.max(1),
                h: h.max(1),
            },
        );
        Ok(id)
    }

    pub fn resize(&mut self, id: SurfaceId, w: u32, h: u32) -> Result<()> {
        if let Some(s) = self.surfaces.get_mut(&id) {
            s.w = w.max(1);
            s.h = h.max(1);
        }
        Ok(())
    }

    pub fn clear(&mut self, id: SurfaceId, color: GpuColor) -> Result<()> {
        self.clear_and_rects(id, color, &[])
    }

    pub fn clear_and_rects(
        &mut self,
        id: SurfaceId,
        bg: GpuColor,
        rects: &[(f32, f32, f32, f32, GpuColor)],
    ) -> Result<()> {
        let surface = self
            .surfaces
            .get(&id)
            .context("unknown GPU surface")?
            .egl_surface;
        let (w, h) = {
            let s = self.surfaces.get(&id).unwrap();
            (s.w, s.h)
        };
        self.egl
            .make_current(
                self.display,
                Some(surface),
                Some(surface),
                Some(self.context),
            )
            .map_err(|e| anyhow::anyhow!("eglMakeCurrent: {e:?}"))?;
        unsafe {
            gl::Viewport(0, 0, w as i32, h as i32);
            gl::ClearColor(bg.r, bg.g, bg.b, bg.a);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            for &(x, y, rw, rh, c) in rects {
                let px = (x * w as f32) as i32;
                let py_top = (y * h as f32) as i32;
                let pw = (rw * w as f32) as i32;
                let ph = (rh * h as f32) as i32;
                let py = h as i32 - py_top - ph;
                gl::Enable(gl::SCISSOR_TEST);
                gl::Scissor(px, py.max(0), pw.max(1), ph.max(1));
                gl::ClearColor(c.r, c.g, c.b, c.a);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                gl::Disable(gl::SCISSOR_TEST);
            }
        }
        self.egl
            .swap_buffers(self.display, surface)
            .map_err(|e| anyhow::anyhow!("eglSwapBuffers: {e:?}"))?;
        Ok(())
    }

    pub fn destroy_surface(&mut self, id: SurfaceId) {
        if let Some(s) = self.surfaces.remove(&id) {
            let _ = self.egl.destroy_surface(self.display, s.egl_surface);
        }
    }
}

impl Drop for EglXcb {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        for (_, s) in self.surfaces.drain() {
            let _ = self.egl.destroy_surface(self.display, s.egl_surface);
        }
        let _ = self.egl.destroy_context(self.display, self.context);
        let _ = self.egl.terminate(self.display);
    }
}

unsafe fn get_platform_display(
    egl_inst: &egl::DynamicInstance<egl::EGL1_4>,
    platform: egl::Enum,
    native: *mut c_void,
    attribs: &[egl::Attrib],
) -> Result<egl::Display> {
    type FnGetPlatformDisplay = unsafe extern "system" fn(
        egl::Enum,
        *mut c_void,
        *const egl::Attrib,
    ) -> egl::EGLDisplay;
    let ptr = egl_inst
        .get_proc_address("eglGetPlatformDisplayEXT")
        .or_else(|| egl_inst.get_proc_address("eglGetPlatformDisplay"));
    let Some(ptr) = ptr else {
        bail!("eglGetPlatformDisplay(EXT) missing");
    };
    let f: FnGetPlatformDisplay = std::mem::transmute(ptr);
    let dpy = f(platform, native, attribs.as_ptr());
    if dpy.is_null() {
        let err = egl_inst.get_error();
        bail!("eglGetPlatformDisplay({platform:#x}) null (egl error {err:?})");
    }
    Ok(egl::Display::from_ptr(dpy))
}

fn load_gl(loader: impl Fn(&str) -> *const c_void) {
    gl::load_with(|s| loader(s).cast());
}

#[allow(dead_code)]
fn _ext_check(egl: &egl::DynamicInstance<egl::EGL1_4>) -> String {
    egl.query_string(None, egl::EXTENSIONS)
        .ok()
        .map(|p| unsafe { CStr::from_ptr(p.as_ptr()) }.to_string_lossy().into_owned())
        .unwrap_or_default()
}
