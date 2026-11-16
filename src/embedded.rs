//! libmpv core plus a renderer bound to eframe's OpenGL context.
//! Core commands/events use the existing private IPC transport on a worker;
//! only render API calls run inside egui's GL paint callback.
use eframe::{
    egui,
    glow::{self, HasContext},
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct GlInit {
    get_proc: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    ctx: *mut c_void,
}
#[repr(C)]
struct Fbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    format: c_int,
}
#[link(name = "mpv")]
unsafe extern "C" {
    fn mpv_create() -> *mut c_void;
    fn mpv_set_option_string(
        handle: *mut c_void,
        name: *const c_char,
        value: *const c_char,
    ) -> c_int;
    fn mpv_initialize(handle: *mut c_void) -> c_int;
    fn mpv_terminate_destroy(handle: *mut c_void);
    fn mpv_error_string(error: c_int) -> *const c_char;
    fn mpv_render_context_create(
        ctx: *mut *mut c_void,
        handle: *mut c_void,
        params: *mut Param,
    ) -> c_int;
    fn mpv_render_context_render(ctx: *mut c_void, params: *mut Param) -> c_int;
    fn mpv_render_context_update(ctx: *mut c_void) -> u64;
    fn mpv_render_context_set_update_callback(
        ctx: *mut c_void,
        callback: Option<unsafe extern "C" fn(*mut c_void)>,
        data: *mut c_void,
    );
    fn mpv_render_context_free(ctx: *mut c_void);
}
fn check(code: c_int) -> Result<(), String> {
    if code >= 0 {
        Ok(())
    } else {
        Err(format!(
            "libmpv: {}",
            unsafe { CStr::from_ptr(mpv_error_string(code)) }.to_string_lossy()
        ))
    }
}
pub struct Core {
    handle: *mut c_void,
    pub socket: PathBuf,
    dir: PathBuf,
}
// libmpv's client API is thread-safe. The worker retains the core while playing;
// GL resources are separately confined to the UI paint/exit thread.
unsafe impl Send for Core {}
unsafe impl Sync for Core {}
impl Core {
    fn create(vo: &str, settings: &crate::config::Settings) -> Result<Arc<Self>, String> {
        use std::os::unix::fs::DirBuilderExt;
        let dir = std::env::temp_dir().join(format!("aster-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|e| e.to_string())?;
        let handle = unsafe { mpv_create() };
        if handle.is_null() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err("Could not create libmpv.".into());
        }
        let core = Arc::new(Self {
            handle,
            socket: dir.join("mpv.sock"),
            dir,
        });
        let script = core.dir.join("aster.lua");
        std::fs::write(&script, include_str!("../assets/player/aster.lua"))
            .map_err(|e| e.to_string())?;
        for (name, value) in [
            ("vo", vo),
            ("idle", "yes"),
            ("keep-open", "no"),
            ("terminal", "no"),
            ("osc", "no"),
            ("input-default-bindings", "no"),
            ("config", "no"),
            ("load-scripts", "no"),
            (
                "hwdec",
                if settings.hardware_decode {
                    "auto-safe"
                } else {
                    "no"
                },
            ),
            ("volume", &settings.volume.to_string()),
            (
                "input-ipc-server",
                core.socket.to_str().ok_or("Invalid socket path")?,
            ),
            ("scripts", script.to_str().ok_or("Invalid script path")?),
            ("script-opts", "aster-embedded=yes"),
        ] {
            let name = CString::new(name).unwrap();
            let value = CString::new(value).map_err(|_| "Invalid player option")?;
            check(unsafe { mpv_set_option_string(handle, name.as_ptr(), value.as_ptr()) })?;
        }
        check(unsafe { mpv_initialize(handle) })?;
        Ok(core)
    }
    #[cfg(test)]
    pub fn headless(settings: &crate::config::Settings) -> Result<Arc<Self>, String> {
        Self::create("null", settings)
    }
}
impl Drop for Core {
    fn drop(&mut self) {
        unsafe { mpv_terminate_destroy(self.handle) };
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
struct Renderer {
    context: *mut c_void,
    wakeup: Arc<RenderSignal>,
    _core: Arc<Core>,
}
// The mutex is for callback ownership, not permission to use GL from workers.
// draw and close are called only by eframe on its current GL context.
unsafe impl Send for Renderer {}
unsafe impl Sync for Renderer {}
struct RenderSignal {
    pending: std::sync::atomic::AtomicBool,
    active: std::sync::atomic::AtomicBool,
    stop: std::sync::atomic::AtomicBool,
    thread: std::sync::OnceLock<std::thread::Thread>,
    calls: std::sync::atomic::AtomicU64,
}
// The libmpv callback only signals a separate notifier. It never enters egui,
// libmpv or the renderer lock, avoiding cross-library lock inversion.
unsafe extern "C" fn wake(data: *mut c_void) {
    if !data.is_null() {
        let signal = unsafe { &*(data as *const RenderSignal) };
        signal
            .pending
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(thread) = signal.thread.get() {
            thread.unpark();
        }
    }
}
unsafe extern "C" fn proc_address(data: *mut c_void, name: *const c_char) -> *mut c_void {
    let loader = unsafe { &*(data as *const &dyn Fn(&CStr) -> *const c_void) };
    loader(unsafe { CStr::from_ptr(name) }).cast_mut()
}
pub struct Embedded {
    pub core: Arc<Core>,
    renderer: Arc<Mutex<Renderer>>,
    signal: Arc<RenderSignal>,
}
impl Embedded {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        settings: &crate::config::Settings,
    ) -> Result<Self, String> {
        let loader = cc
            .get_proc_address
            .ok_or("OpenGL is required for embedded playback.")?;
        let core = Core::create("libmpv", settings)?;
        let mut context = std::ptr::null_mut();
        // create resolves entry points synchronously; the borrowed loader and
        // init parameters are valid for that entire call.
        let mut init = GlInit {
            get_proc: proc_address,
            ctx: (&loader as *const &dyn Fn(&CStr) -> *const c_void)
                .cast_mut()
                .cast(),
        };
        let mut params = vec![
            Param {
                kind: 1,
                data: c"opengl".as_ptr().cast_mut().cast(),
            },
            Param {
                kind: 2,
                data: (&mut init as *mut GlInit).cast(),
            },
            Param {
                kind: 0,
                data: std::ptr::null_mut(),
            },
        ];
        // Share the native display so libmpv can import hardware frames into
        // this GL context instead of downloading and re-uploading each frame.
        use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
        if let Ok(display) = cc.display_handle() {
            let native = match display.as_raw() {
                RawDisplayHandle::Wayland(handle) => Some((9, handle.display.as_ptr())),
                RawDisplayHandle::Xlib(handle) => handle.display.map(|p| (8, p.as_ptr())),
                _ => None,
            };
            if let Some((kind, data)) = native {
                params.insert(params.len() - 1, Param { kind, data });
            }
        }
        check(unsafe {
            mpv_render_context_create(&mut context, core.handle, params.as_mut_ptr())
        })?;
        let signal = Arc::new(RenderSignal {
            pending: std::sync::atomic::AtomicBool::new(true),
            active: std::sync::atomic::AtomicBool::new(false),
            stop: std::sync::atomic::AtomicBool::new(false),
            thread: std::sync::OnceLock::new(),
            calls: std::sync::atomic::AtomicU64::new(0),
        });
        let renderer = Renderer {
            context,
            wakeup: signal.clone(),
            _core: core.clone(),
        };
        unsafe {
            mpv_render_context_set_update_callback(
                context,
                Some(wake),
                Arc::as_ptr(&signal).cast_mut().cast(),
            );
        }
        let notifier = signal.clone();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            let _ = notifier.thread.set(std::thread::current());
            while !notifier.stop.load(std::sync::atomic::Ordering::Acquire) {
                if notifier.active.load(std::sync::atomic::Ordering::Acquire)
                    && notifier
                        .pending
                        .swap(false, std::sync::atomic::Ordering::AcqRel)
                {
                    // A nonzero request avoids egui's additional immediate
                    // follow-up pass: one media frame needs one UI paint.
                    ctx.request_repaint_after(std::time::Duration::from_millis(1));
                }
                std::thread::park();
            }
        });
        Ok(Self {
            core,
            renderer: Arc::new(Mutex::new(renderer)),
            signal,
        })
    }
    pub fn paint(&self, ui: &egui::Ui, rect: egui::Rect) {
        let renderer = self.renderer.clone();
        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                let renderer = renderer.lock().unwrap();
                if renderer.context.is_null() {
                    return;
                }
                let gl = painter.gl();
                unsafe {
                    let framebuffer = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
                    let mut target = Fbo {
                        fbo: framebuffer,
                        w: info.screen_size_px[0] as i32,
                        h: info.screen_size_px[1] as i32,
                        format: 0,
                    };
                    let mut flip = 1_i32;
                    let mut block = 0_i32;
                    let mut params = [
                        Param {
                            kind: 3,
                            data: (&mut target as *mut Fbo).cast(),
                        },
                        Param {
                            kind: 4,
                            data: (&mut flip as *mut i32).cast(),
                        },
                        Param {
                            kind: 12,
                            data: (&mut block as *mut i32).cast(),
                        },
                        Param {
                            kind: 0,
                            data: std::ptr::null_mut(),
                        },
                    ];
                    renderer
                        .wakeup
                        .calls
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    mpv_render_context_update(renderer.context);
                    mpv_render_context_render(renderer.context, params.as_mut_ptr());
                    // egui restores its other GL state after the callback.
                    gl.bind_framebuffer(glow::FRAMEBUFFER, painter.intermediate_fbo());
                }
            })),
        });
    }
    pub fn set_active(&self, active: bool) {
        if self
            .signal
            .active
            .swap(active, std::sync::atomic::Ordering::AcqRel)
            != active
            && let Some(thread) = self.signal.thread.get()
        {
            thread.unpark();
        }
    }
    pub fn render_calls(&self) -> u64 {
        self.signal.calls.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub fn close(&mut self) {
        self.signal
            .stop
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(thread) = self.signal.thread.get() {
            thread.unpark();
        }
        let mut renderer = self.renderer.lock().unwrap();
        if !renderer.context.is_null() {
            unsafe {
                mpv_render_context_set_update_callback(
                    renderer.context,
                    None,
                    std::ptr::null_mut(),
                );
                mpv_render_context_free(renderer.context);
            }
            renderer.context = std::ptr::null_mut();
        }
    }
}
