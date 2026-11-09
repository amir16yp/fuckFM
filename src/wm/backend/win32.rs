//! Win32 shell host (`fuckwm.exe`) with optional WGL GPU accel.
//!
//! Fullscreen desktop HWND (icons from ~/Desktop) + topmost taskbar + Start.
//! Replaces Explorer as the Winlogon shell when installed — not Microsoft's
//! Desktop Window Manager compositor.

use crate::iconpack::{purpose, purpose_for_file, IconPack, IconRgba};
use crate::settings::Settings;
use crate::theme::{theme, Theme};
use crate::wm::gpu::{Gpu, GpuColor, SurfaceId};
use crate::wm::start_menu::{self, StartAction, StartEntry, BRAND_W, ICON_SZ, MENU_W};
use crate::wm::wallpaper::{WallpaperCache, WallpaperRgba};
use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::ptr;
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SYSTEMTIME, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreateSolidBrush, DeleteObject, EndPaint, FillRect, GetStockObject,
    InvalidateRect, SetBkMode, SetTextColor, StretchDIBits, TextOutW, UpdateWindow, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLACK_BRUSH, DIB_RGB_COLORS, HDC, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetClientRect, GetMessageW, GetSystemMetrics, LoadCursorW, PostQuitMessage, RegisterClassW,
    SetTimer, SetWindowPos, ShowWindow, TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_OWNDC,
    CS_VREDRAW, CW_USEDEFAULT, HWND_TOPMOST, IDC_ARROW, MF_SEPARATOR, MF_STRING, MSG, SM_CXSCREEN,
    SM_CYSCREEN, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_SHOWWINDOW, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_DESTROY, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONUP, WM_SIZE,
    WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const TASKBAR_H: i32 = 40;
const START_W: i32 = 72;
const FM_BTN_X: i32 = 84;
const FM_BTN_W: i32 = 88;
const START_ROW_H: i32 = 28;
const CLASS_DESKTOP: &str = "FuckWMDesktop";
const CLASS_TASKBAR: &str = "FuckWMTaskbar";
const CLASS_START: &str = "FuckWMStart";
const TIMER_CLOCK: usize = 1;

const IDM_REFRESH: usize = 1;
const IDM_OPEN_FM: usize = 2;
const IDM_NEW_FOLDER: usize = 3;
const IDM_EXIT: usize = 4;

struct DeskIcon {
    name: String,
    path: PathBuf,
    is_dir: bool,
    rect: RECT,
}

struct Host {
    theme: Theme,
    settings: Settings,
    product: String,
    gpu: Option<Gpu>,
    desktop_surf: Option<SurfaceId>,
    taskbar_surf: Option<SurfaceId>,
    desktop_hwnd: HWND,
    taskbar_hwnd: HWND,
    start_hwnd: HWND,
    screen_w: i32,
    screen_h: i32,
    icons: Vec<DeskIcon>,
    icon_pack: IconPack,
    wallpaper: WallpaperCache,
    start_entries: Vec<StartEntry>,
    start_open: bool,
    start_hover: Option<usize>,
    clock: String,
}

static mut HOST: Option<Host> = None;

pub fn run(product: &str) -> Result<()> {
    let settings = Settings::load_or_default();
    let th = theme(settings.theme_id);
    let gpu = Gpu::connect_win32().ok();
    let icons = load_desktop_icons();
    let icon_pack = IconPack::load_for_theme(settings.theme_id, &settings.icon_pack);
    let wallpaper = WallpaperCache::new(settings.theme_id);
    let clock = format_clock();
    eprintln!(
        "{product}: icon pack {} ({} icons)",
        crate::iconpack::pack_name_for_theme(settings.theme_id),
        icon_pack.len()
    );

    unsafe {
        HOST = Some(Host {
            theme: th,
            settings,
            product: product.to_string(),
            gpu,
            desktop_surf: None,
            taskbar_surf: None,
            desktop_hwnd: ptr::null_mut(),
            taskbar_hwnd: ptr::null_mut(),
            start_hwnd: ptr::null_mut(),
            screen_w: 0,
            screen_h: 0,
            icons,
            icon_pack,
            wallpaper,
            start_entries: start_menu::default_entries(),
            start_open: false,
            start_hover: None,
            clock,
        });
    }

    let hinst = unsafe { GetModuleHandleW(ptr::null()) };
    register_class(hinst, CLASS_DESKTOP, Some(desktop_proc))?;
    register_class(hinst, CLASS_TASKBAR, Some(taskbar_proc))?;
    register_class(hinst, CLASS_START, Some(start_proc))?;

    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if sw <= 0 || sh <= 0 {
        bail!("GetSystemMetrics failed");
    }

    let desktop = create_popup(
        hinst,
        CLASS_DESKTOP,
        product,
        WS_EX_TOOLWINDOW,
        0,
        0,
        sw,
        sh - TASKBAR_H,
    )?;
    let taskbar = create_popup(
        hinst,
        CLASS_TASKBAR,
        product,
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        0,
        sh - TASKBAR_H,
        sw,
        TASKBAR_H,
    )?;
    let start_h = unsafe {
        HOST.as_ref()
            .map(|h| start_menu::menu_height(&h.start_entries, START_ROW_H))
            .unwrap_or(200)
    };
    let start_popup = create_popup(
        hinst,
        CLASS_START,
        "Start",
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        4,
        sh - TASKBAR_H - start_h - 2,
        MENU_W,
        start_h,
    )?;
    unsafe {
        ShowWindow(start_popup, SW_HIDE);
    }

    unsafe {
        if let Some(host) = HOST.as_mut() {
            host.desktop_hwnd = desktop;
            host.taskbar_hwnd = taskbar;
            host.start_hwnd = start_popup;
            host.screen_w = sw;
            host.screen_h = sh;
            layout_icons(host, sw, sh - TASKBAR_H);
            if let Some(gpu) = host.gpu.as_mut() {
                host.desktop_surf = gpu
                    .create_hwnd_surface(desktop, sw as u32, (sh - TASKBAR_H) as u32)
                    .ok();
                host.taskbar_surf = gpu
                    .create_hwnd_surface(taskbar, sw as u32, TASKBAR_H as u32)
                    .ok();
            }
        }
        ShowWindow(desktop, SW_SHOW);
        ShowWindow(taskbar, SW_SHOW);
        UpdateWindow(desktop);
        UpdateWindow(taskbar);
        SetTimer(taskbar, TIMER_CLOCK, 1000, None);
    }

    let accel = unsafe {
        HOST.as_ref()
            .map(|h| {
                if h.gpu.is_some() {
                    "WGL/OpenGL"
                } else {
                    "GDI"
                }
            })
            .unwrap_or("?")
    };
    eprintln!(
        "{product}: Win32 shell host {sw}×{sh} ({accel}, {} desktop icons)",
        unsafe { HOST.as_ref().map(|h| h.icons.len()).unwrap_or(0) }
    );

    let mut msg = MSG {
        hwnd: ptr::null_mut(),
        message: 0,
        wParam: 0,
        lParam: 0,
        time: 0,
        pt: POINT { x: 0, y: 0 },
    };
    unsafe {
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

fn register_class(
    hinst: windows_sys::Win32::Foundation::HINSTANCE,
    name: &str,
    proc: windows_sys::Win32::UI::WindowsAndMessaging::WNDPROC,
) -> Result<()> {
    let class_name = wide(name);
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
        lpfnWndProc: proc,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: ptr::null_mut(),
        hCursor: unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW) },
        hbrBackground: unsafe { GetStockObject(BLACK_BRUSH) as _ },
        lpszMenuName: ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };
    let _ = unsafe { RegisterClassW(&wc) };
    Ok(())
}

fn create_popup(
    hinst: windows_sys::Win32::Foundation::HINSTANCE,
    class: &str,
    title: &str,
    ex: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> Result<HWND> {
    let class_w = wide(class);
    let title_w = wide(title);
    let hwnd = unsafe {
        CreateWindowExW(
            ex,
            class_w.as_ptr(),
            title_w.as_ptr(),
            WS_POPUP | WS_VISIBLE,
            x,
            y,
            w,
            h,
            ptr::null_mut(),
            ptr::null_mut(),
            hinst,
            ptr::null(),
        )
    };
    if hwnd.is_null() {
        bail!("CreateWindowExW({class}) failed");
    }
    let _ = CW_USEDEFAULT;
    Ok(hwnd)
}

unsafe extern "system" fn desktop_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_desktop(hwnd);
            0
        }
        WM_SIZE => {
            if let Some(host) = HOST.as_mut() {
                let w = (lparam & 0xFFFF) as u32;
                let h = ((lparam >> 16) & 0xFFFF) as u32;
                layout_icons(host, w as i32, h as i32);
                if let (Some(gpu), Some(id)) = (host.gpu.as_mut(), host.desktop_surf) {
                    let _ = gpu.resize(id, w.max(1), h.max(1));
                }
            }
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            close_start();
            if let Some(host) = HOST.as_ref() {
                if let Some(icon) = host.icons.iter().find(|ic| hit_rect(&ic.rect, x, y)) {
                    let _ = open_desk_item(&icon.path, icon.is_dir);
                }
            }
            0
        }
        WM_RBUTTONUP => {
            close_start();
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            desktop_context_menu(hwnd, x, y);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn taskbar_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_taskbar(hwnd);
            0
        }
        WM_TIMER if wparam == TIMER_CLOCK => {
            if let Some(host) = HOST.as_mut() {
                host.clock = format_clock();
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            if x < START_W + 4 {
                toggle_start();
            } else if x >= FM_BTN_X && x < FM_BTN_X + FM_BTN_W {
                close_start();
                let _ = launch_fm(None);
            } else {
                close_start();
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn start_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint_start(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            if let Some(host) = HOST.as_mut() {
                let hover = if x >= BRAND_W {
                    start_menu::hit_index(&host.start_entries, y, START_ROW_H)
                } else {
                    None
                };
                if hover != host.start_hover {
                    host.start_hover = hover;
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let action = if x >= BRAND_W {
                HOST.as_ref().and_then(|host| {
                    start_menu::hit_index(&host.start_entries, y, START_ROW_H)
                        .map(|i| host.start_entries[i].action.clone())
                })
            } else {
                None
            };
            close_start();
            if let Some(action) = action {
                apply_start_action(action);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint_desktop(hwnd: HWND) {
    let Some(host) = HOST.as_mut() else {
        return;
    };
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    GetClientRect(hwnd, &mut rc);
    let dw = (rc.right - rc.left).max(1);
    let dh = (rc.bottom - rc.top).max(1);
    // HKCU\Control Panel\Desktop (Wallpaper / WallpaperStyle / TileWallpaper) when
    // settings.wallpaper is unset; theme procedural default otherwise.
    host.wallpaper.ensure(
        host.settings.theme_id,
        host.settings.wallpaper.as_deref(),
        dw as u32,
        dh as u32,
    );

    let mut ps = empty_ps();
    let hdc = BeginPaint(hwnd, &mut ps);
    blit_wallpaper(hdc, 0, 0, dw, dh, host.wallpaper.body());
    draw_icons_gdi(hdc, host);
    EndPaint(hwnd, &ps);

    // GPU surface stays as a solid underlay; wallpaper+icons are GDI for now.
    if let (Some(gpu), Some(id)) = (host.gpu.as_mut(), host.desktop_surf) {
        let _ = gpu.clear(id, GpuColor::from_sdl(host.theme.desktop));
    }
}

unsafe fn draw_icons_gdi(hdc: HDC, host: &Host) {
    SetBkMode(hdc, TRANSPARENT as _);
    SetTextColor(hdc, colorref(host.theme.text));
    let folder_brush = CreateSolidBrush(colorref(host.theme.icon_folder));
    let file_brush = CreateSolidBrush(colorref(host.theme.icon_file));
    for icon in &host.icons {
        let size = 32;
        let ix = icon.rect.left + (icon.rect.right - icon.rect.left - size) / 2;
        let iy = icon.rect.top + 4;
        let purpose = desk_purpose(icon.is_dir, &icon.path);
        if let Some(rgba) = host.icon_pack.resolve(purpose, size as u32) {
            blit_rgba(hdc, ix, iy, size, size, rgba);
        } else {
            let glyph = RECT {
                left: ix,
                top: iy,
                right: ix + size,
                bottom: iy + size,
            };
            FillRect(
                hdc,
                &glyph,
                if icon.is_dir {
                    folder_brush
                } else {
                    file_brush
                },
            );
        }
        text_out(
            hdc,
            icon.rect.left + 4,
            icon.rect.top + 40,
            &truncate(&icon.name, 12),
        );
    }
    DeleteObject(folder_brush);
    DeleteObject(file_brush);
}

fn desk_purpose(is_dir: bool, path: &Path) -> &'static str {
    if is_dir {
        purpose::FOLDER
    } else {
        purpose_for_file(path)
    }
}

/// Stretch an RGBA icon onto `hdc` via a 32-bpp top-down DIB (BGRA).
unsafe fn blit_rgba(hdc: HDC, x: i32, y: i32, dw: i32, dh: i32, icon: &IconRgba) {
    blit_rgba_raw(hdc, x, y, dw, dh, icon.width, icon.height, &icon.rgba);
}

unsafe fn blit_wallpaper(hdc: HDC, x: i32, y: i32, dw: i32, dh: i32, paper: &WallpaperRgba) {
    blit_rgba_raw(hdc, x, y, dw, dh, paper.width, paper.height, &paper.rgba);
}

unsafe fn blit_rgba_raw(
    hdc: HDC,
    x: i32,
    y: i32,
    dw: i32,
    dh: i32,
    width: u32,
    height: u32,
    rgba: &[u8],
) {
    if width == 0 || height == 0 || rgba.len() < 4 {
        return;
    }
    let mut bgra = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB as u32,
            biSizeImage: bgra.len() as u32,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD {
            rgbBlue: 0,
            rgbGreen: 0,
            rgbRed: 0,
            rgbReserved: 0,
        }],
    };
    StretchDIBits(
        hdc,
        x,
        y,
        dw,
        dh,
        0,
        0,
        width as i32,
        height as i32,
        bgra.as_ptr() as *const _,
        &bmi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
}

unsafe fn paint_taskbar(hwnd: HWND) {
    let Some(host) = HOST.as_mut() else {
        return;
    };
    if let (Some(gpu), Some(id)) = (host.gpu.as_mut(), host.taskbar_surf) {
        let w = host.screen_w.max(1) as f32;
        let h = TASKBAR_H as f32;
        let btn = GpuColor::from_sdl(host.theme.button.face);
        let rects = [
            (4.0 / w, 4.0 / h, START_W as f32 / w, (h - 8.0) / h, btn),
            (
                FM_BTN_X as f32 / w,
                4.0 / h,
                FM_BTN_W as f32 / w,
                (h - 8.0) / h,
                btn,
            ),
        ];
        let _ = gpu.clear_and_rects(id, GpuColor::from_sdl(host.theme.chrome.face), &rects);
        let mut ps = empty_ps();
        let hdc = BeginPaint(hwnd, &mut ps);
        SetBkMode(hdc, TRANSPARENT as _);
        SetTextColor(hdc, colorref(host.theme.text));
        paint_taskbar_labels(hdc, host, host.screen_w);
        EndPaint(hwnd, &ps);
        return;
    }

    let mut ps = empty_ps();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    GetClientRect(hwnd, &mut rc);
    let brush = CreateSolidBrush(colorref(host.theme.chrome.face));
    FillRect(hdc, &rc, brush);
    DeleteObject(brush);
    let btn = CreateSolidBrush(colorref(host.theme.button.face));
    let start = RECT {
        left: 4,
        top: 4,
        right: 4 + START_W,
        bottom: rc.bottom - 4,
    };
    FillRect(hdc, &start, btn);
    let fm = RECT {
        left: FM_BTN_X,
        top: 4,
        right: FM_BTN_X + FM_BTN_W,
        bottom: rc.bottom - 4,
    };
    FillRect(hdc, &fm, btn);
    DeleteObject(btn);
    SetBkMode(hdc, TRANSPARENT as _);
    SetTextColor(hdc, colorref(host.theme.text));
    paint_taskbar_labels(hdc, host, rc.right);
    EndPaint(hwnd, &ps);
}

unsafe fn paint_taskbar_labels(hdc: HDC, host: &Host, width: i32) {
    let glyph = 18;
    if let Some(icon) = host
        .icon_pack
        .resolve("apps/system-file-manager", glyph as u32)
        .or_else(|| host.icon_pack.resolve(purpose::FOLDER, glyph as u32))
    {
        blit_rgba(hdc, 10, (TASKBAR_H - glyph) / 2, glyph, glyph, icon);
        text_out(hdc, 32, 12, "Start");
    } else {
        text_out(hdc, 18, 12, "Start");
    }
    if let Some(icon) = host
        .icon_pack
        .resolve("apps/system-file-manager", glyph as u32)
        .or_else(|| host.icon_pack.resolve(purpose::COMPUTER, glyph as u32))
    {
        blit_rgba(hdc, FM_BTN_X + 6, (TASKBAR_H - glyph) / 2, glyph, glyph, icon);
        text_out(hdc, FM_BTN_X + 28, 12, "fuckFM");
    } else {
        text_out(hdc, 96, 12, "fuckFM");
    }
    text_out(hdc, width - 70, 12, &host.clock);
}

unsafe fn paint_start(hwnd: HWND) {
    let Some(host) = HOST.as_ref() else {
        return;
    };
    let mut ps = empty_ps();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    GetClientRect(hwnd, &mut rc);

    let face = CreateSolidBrush(colorref(host.theme.window));
    FillRect(hdc, &rc, face);
    DeleteObject(face);

    // Brand strip.
    let brand = CreateSolidBrush(colorref(host.theme.title.active_left));
    FillRect(
        hdc,
        &RECT {
            left: 0,
            top: 0,
            right: BRAND_W,
            bottom: rc.bottom,
        },
        brand,
    );
    DeleteObject(brand);
    SetBkMode(hdc, TRANSPARENT as _);
    SetTextColor(hdc, colorref(host.theme.title.text));
    let mut by = 10;
    for ch in host.product.chars().take(10) {
        text_out(hdc, 8, by, &ch.to_string());
        by += 14;
    }

    let edge = CreateSolidBrush(colorref(host.theme.border));
    FillRect(
        hdc,
        &RECT {
            left: 0,
            top: 0,
            right: rc.right,
            bottom: 1,
        },
        edge,
    );
    FillRect(
        hdc,
        &RECT {
            left: 0,
            top: rc.bottom - 1,
            right: rc.right,
            bottom: rc.bottom,
        },
        edge,
    );
    FillRect(
        hdc,
        &RECT {
            left: rc.right - 1,
            top: 0,
            right: rc.right,
            bottom: rc.bottom,
        },
        edge,
    );
    DeleteObject(edge);

    for (i, entry) in host.start_entries.iter().enumerate() {
        let y = start_menu::entry_y(&host.start_entries, i, START_ROW_H);
        if entry.separator_before {
            let sy = y - 4;
            let sep = CreateSolidBrush(colorref(host.theme.border));
            FillRect(
                hdc,
                &RECT {
                    left: BRAND_W + 8,
                    top: sy,
                    right: rc.right - 8,
                    bottom: sy + 1,
                },
                sep,
            );
            DeleteObject(sep);
        }
        if host.start_hover == Some(i) {
            let sel = CreateSolidBrush(colorref(host.theme.selection));
            FillRect(
                hdc,
                &RECT {
                    left: BRAND_W,
                    top: y,
                    right: rc.right - 1,
                    bottom: y + START_ROW_H,
                },
                sel,
            );
            DeleteObject(sel);
            SetTextColor(hdc, colorref(host.theme.selection_text));
        } else {
            SetTextColor(hdc, colorref(host.theme.text));
        }
        let mut text_x = BRAND_W + 8;
        if let Some(purpose) = entry.icon {
            if let Some(icon) = host
                .icon_pack
                .resolve(purpose, ICON_SZ as u32)
                .or_else(|| host.icon_pack.resolve(purpose, 32))
            {
                let iy = y + (START_ROW_H - ICON_SZ) / 2;
                blit_rgba(hdc, text_x, iy, ICON_SZ, ICON_SZ, icon);
                text_x += ICON_SZ + 6;
            }
        }
        text_out(hdc, text_x, y + 6, entry.label);
    }
    EndPaint(hwnd, &ps);
}

unsafe fn desktop_context_menu(hwnd: HWND, client_x: i32, client_y: i32) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let w_refresh = wide("Refresh");
    let w_open = wide("Open fuckFM");
    let w_new = wide("New Folder");
    let w_exit = wide("Exit fuckWM");
    AppendMenuW(menu, MF_STRING, IDM_REFRESH, w_refresh.as_ptr());
    AppendMenuW(menu, MF_STRING, IDM_OPEN_FM, w_open.as_ptr());
    AppendMenuW(menu, MF_STRING, IDM_NEW_FOLDER, w_new.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
    AppendMenuW(menu, MF_STRING, IDM_EXIT, w_exit.as_ptr());

    let mut pt = POINT {
        x: client_x,
        y: client_y,
    };
    ClientToScreen(hwnd, &mut pt);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
        pt.x,
        pt.y,
        0,
        hwnd,
        ptr::null(),
    ) as usize;
    DestroyMenu(menu);

    match cmd {
        IDM_REFRESH => refresh_desktop_icons(hwnd),
        IDM_OPEN_FM => {
            let _ = launch_fm(None);
        }
        IDM_NEW_FOLDER => {
            let _ = new_desktop_folder();
            refresh_desktop_icons(hwnd);
        }
        IDM_EXIT => {
            PostQuitMessage(0);
        }
        _ => {}
    }
}

unsafe fn refresh_desktop_icons(hwnd: HWND) {
    if let Some(host) = HOST.as_mut() {
        host.icons = load_desktop_icons();
        layout_icons(host, host.screen_w, host.screen_h - TASKBAR_H);
        InvalidateRect(hwnd, ptr::null(), 1);
    }
}

fn new_desktop_folder() -> Result<()> {
    let desk = desktop_dir().context("Desktop folder missing")?;
    for i in 0..64 {
        let name = if i == 0 {
            "New Folder".to_string()
        } else {
            format!("New Folder ({i})")
        };
        let path = desk.join(&name);
        if !path.exists() {
            fs::create_dir(&path).with_context(|| format!("create {}", path.display()))?;
            return Ok(());
        }
    }
    bail!("too many New Folder copies");
}

fn empty_ps() -> PAINTSTRUCT {
    PAINTSTRUCT {
        hdc: ptr::null_mut(),
        fErase: 0,
        rcPaint: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        fRestore: 0,
        fIncUpdate: 0,
        rgbReserved: [0; 32],
    }
}

unsafe fn text_out(hdc: HDC, x: i32, y: i32, s: &str) {
    let w = wide(s);
    TextOutW(hdc, x, y, w.as_ptr(), (w.len() - 1) as i32);
}

fn colorref(c: sdl2::pixels::Color) -> COLORREF {
    (c.r as u32) | ((c.g as u32) << 8) | ((c.b as u32) << 16)
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn hit_rect(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn truncate(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn format_clock() -> String {
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut st);
        format!("{:02}:{:02}", st.wHour, st.wMinute)
    }
}

fn desktop_dir() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let desk = PathBuf::from(home).join("Desktop");
    if desk.is_dir() {
        Some(desk)
    } else {
        None
    }
}

fn load_desktop_icons() -> Vec<DeskIcon> {
    let mut icons = Vec::new();
    let Some(dir) = desktop_dir() else {
        return icons;
    };
    let Ok(rd) = fs::read_dir(&dir) else {
        return icons;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for ent in entries.into_iter().take(48) {
        let path = ent.path();
        let name = ent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        icons.push(DeskIcon {
            is_dir: path.is_dir(),
            name,
            path,
            rect: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
        });
    }
    icons
}

fn layout_icons(host: &mut Host, area_w: i32, area_h: i32) {
    let cell_w = 84;
    let cell_h = 72;
    let cols = ((area_w - 16) / cell_w).max(1);
    let _ = area_h;
    for (i, icon) in host.icons.iter_mut().enumerate() {
        let col = (i as i32) % cols;
        let row = (i as i32) / cols;
        let x = 12 + col * cell_w;
        let y = 12 + row * cell_h;
        icon.rect = RECT {
            left: x,
            top: y,
            right: x + cell_w - 8,
            bottom: y + cell_h - 8,
        };
    }
}

unsafe fn toggle_start() {
    let Some(host) = HOST.as_mut() else {
        return;
    };
    host.start_open = !host.start_open;
    if host.start_open {
        let h = start_menu::menu_height(&host.start_entries, START_ROW_H);
        let y = host.screen_h - TASKBAR_H - h - 2;
        SetWindowPos(
            host.start_hwnd,
            HWND_TOPMOST,
            4,
            y,
            MENU_W,
            h,
            SWP_SHOWWINDOW | SWP_NOACTIVATE,
        );
        host.start_hover = None;
        InvalidateRect(host.start_hwnd, ptr::null(), 1);
    } else {
        ShowWindow(host.start_hwnd, SW_HIDE);
    }
}

unsafe fn close_start() {
    let Some(host) = HOST.as_mut() else {
        return;
    };
    if host.start_open {
        host.start_open = false;
        host.start_hover = None;
        ShowWindow(host.start_hwnd, SW_HIDE);
    }
}

unsafe fn apply_start_action(action: StartAction) {
    match action {
        StartAction::LaunchFm => {
            let _ = start_menu::launch_fm(None);
        }
        StartAction::OpenThisPc => {
            let _ = start_menu::launch_fm_this_pc();
        }
        StartAction::OpenDesktop => {
            if let Some(p) = start_menu::desktop_dir().or_else(desktop_dir) {
                let _ = start_menu::launch_fm(Some(&p));
            }
        }
        StartAction::OpenDocuments => {
            if let Some(p) = start_menu::documents_dir() {
                let _ = start_menu::launch_fm(Some(&p));
            }
        }
        StartAction::OpenPictures => {
            if let Some(p) = start_menu::pictures_dir() {
                let _ = start_menu::launch_fm(Some(&p));
            }
        }
        StartAction::OpenCmd => {
            let _ = start_menu::open_command_prompt();
        }
        StartAction::CycleTheme => {
            if let Some(host) = HOST.as_mut() {
                host.settings.theme_id = host.settings.theme_id.next();
                host.theme = theme(host.settings.theme_id);
                host.icon_pack =
                    IconPack::load_for_theme(host.settings.theme_id, &host.settings.icon_pack);
                let _ = host.settings.save();
                InvalidateRect(host.desktop_hwnd, ptr::null(), 1);
                InvalidateRect(host.taskbar_hwnd, ptr::null(), 1);
            }
        }
        StartAction::LogOff => start_menu::power_action("logoff"),
        StartAction::Restart => start_menu::power_action("restart"),
        StartAction::Shutdown => start_menu::power_action("shutdown"),
        StartAction::QuitSession => PostQuitMessage(0),
    }
}

fn open_desk_item(path: &std::path::Path, is_dir: bool) -> Result<()> {
    if is_dir {
        launch_fm(Some(path))
    } else {
        Command::new("cmd.exe")
            .args(["/C", "start", "", &path.display().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }
}

fn launch_fm(path: Option<&std::path::Path>) -> Result<()> {
    let exe = std::env::current_exe().context("current_exe")?;
    let fm = exe.with_file_name("fuckfm.exe");
    let mut cmd = Command::new(if fm.is_file() { fm } else { exe });
    if let Some(path) = path {
        cmd.arg(path);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

