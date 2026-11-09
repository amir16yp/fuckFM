//! WGL + OpenGL hardware surfaces on HWND.

use super::{GpuColor, SurfaceId};
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{GetDC, ReleaseDC, HDC};
use windows_sys::Win32::Graphics::OpenGL::{
    wglCreateContext, wglDeleteContext, wglGetProcAddress, wglMakeCurrent, ChoosePixelFormat,
    DescribePixelFormat, SetPixelFormat, SwapBuffers, HGLRC, PFD_DOUBLEBUFFER, PFD_DRAW_TO_WINDOW,
    PFD_MAIN_PLANE, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA, PIXELFORMATDESCRIPTOR,
};

pub struct WglHost {
    surfaces: HashMap<SurfaceId, Surface>,
    next_id: u64,
    gl_loaded: bool,
}

struct Surface {
    hwnd: HWND,
    hdc: HDC,
    hglrc: HGLRC,
    w: u32,
    h: u32,
}

impl WglHost {
    pub fn new() -> Result<Self> {
        // Prove opengl32 is loadable; real contexts are per-HWND.
        unsafe {
            let _lib =
                libloading::Library::new("opengl32.dll").context("load opengl32.dll")?;
            std::mem::forget(_lib);
        }
        Ok(Self {
            surfaces: HashMap::new(),
            next_id: 1,
            gl_loaded: false,
        })
    }

    pub fn create_surface(&mut self, hwnd: HWND, w: u32, h: u32) -> Result<SurfaceId> {
        unsafe {
            let hdc = GetDC(hwnd);
            if hdc.is_null() {
                bail!("GetDC failed");
            }
            let mut pfd = PIXELFORMATDESCRIPTOR {
                nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
                nVersion: 1,
                dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
                iPixelType: PFD_TYPE_RGBA as u8,
                cColorBits: 32,
                cRedBits: 0,
                cRedShift: 0,
                cGreenBits: 0,
                cGreenShift: 0,
                cBlueBits: 0,
                cBlueShift: 0,
                cAlphaBits: 0,
                cAlphaShift: 0,
                cAccumBits: 0,
                cAccumRedBits: 0,
                cAccumGreenBits: 0,
                cAccumBlueBits: 0,
                cAccumAlphaBits: 0,
                cDepthBits: 16,
                cStencilBits: 0,
                cAuxBuffers: 0,
                iLayerType: PFD_MAIN_PLANE as u8,
                bReserved: 0,
                dwLayerMask: 0,
                dwVisibleMask: 0,
                dwDamageMask: 0,
            };
            let fmt = ChoosePixelFormat(hdc, &pfd);
            if fmt == 0 {
                ReleaseDC(hwnd, hdc);
                bail!("ChoosePixelFormat failed");
            }
            DescribePixelFormat(
                hdc,
                fmt,
                std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32,
                &mut pfd,
            );
            if SetPixelFormat(hdc, fmt, &pfd) == 0 {
                ReleaseDC(hwnd, hdc);
                bail!("SetPixelFormat failed");
            }
            let hglrc = wglCreateContext(hdc);
            if hglrc.is_null() {
                ReleaseDC(hwnd, hdc);
                bail!("wglCreateContext failed");
            }
            if wglMakeCurrent(hdc, hglrc) == 0 {
                wglDeleteContext(hglrc);
                ReleaseDC(hwnd, hdc);
                bail!("wglMakeCurrent failed");
            }
            if !self.gl_loaded {
                gl::load_with(|name| {
                    let Ok(c) = CString::new(name) else {
                        return ptr::null();
                    };
                    let p = wglGetProcAddress(c.as_ptr() as _);
                    if let Some(f) = p {
                        return f as *const c_void;
                    }
                    load_gl11(name)
                });
                self.gl_loaded = true;
            }
            let id = SurfaceId(self.next_id);
            self.next_id += 1;
            self.surfaces.insert(
                id,
                Surface {
                    hwnd,
                    hdc,
                    hglrc,
                    w: w.max(1),
                    h: h.max(1),
                },
            );
            Ok(id)
        }
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
        let (hdc, hglrc, w, h) = {
            let s = self.surfaces.get(&id).context("unknown GPU surface")?;
            (s.hdc, s.hglrc, s.w, s.h)
        };
        unsafe {
            if wglMakeCurrent(hdc, hglrc) == 0 {
                bail!("wglMakeCurrent failed");
            }
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
            if SwapBuffers(hdc) == 0 {
                bail!("SwapBuffers failed");
            }
        }
        Ok(())
    }

    pub fn destroy_surface(&mut self, id: SurfaceId) {
        if let Some(s) = self.surfaces.remove(&id) {
            unsafe {
                wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                wglDeleteContext(s.hglrc);
                ReleaseDC(s.hwnd, s.hdc);
            }
        }
    }
}

impl Drop for WglHost {
    fn drop(&mut self) {
        let ids: Vec<_> = self.surfaces.keys().copied().collect();
        for id in ids {
            self.destroy_surface(id);
        }
    }
}

fn load_gl11(name: &str) -> *const c_void {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
    unsafe {
        static mut MOD: windows_sys::Win32::Foundation::HMODULE = ptr::null_mut();
        if MOD.is_null() {
            MOD = LoadLibraryA(b"opengl32.dll\0".as_ptr());
        }
        if MOD.is_null() {
            return ptr::null();
        }
        let Ok(c) = CString::new(name) else {
            return ptr::null();
        };
        match GetProcAddress(MOD, c.as_ptr() as _) {
            Some(p) => p as *const c_void,
            None => ptr::null(),
        }
    }
}
