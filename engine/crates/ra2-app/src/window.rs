//! The window: a `winit` event loop, a `wgpu` surface, and the same frames the
//! headless path draws.
//!
//! The shell has no time-dependent state yet, so the fixed update is a tick that
//! asks for a redraw; input is handled where it arrives. What the milestone
//! needs from this module is the shape: one `FrameRenderer`, a surface that is
//! reconfigured on resize, a click mapped through the scene's rectangle, and a
//! clean exit (the main menu's `Quit`, the close button) rather than a panic
//! when there is no display.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ra2_render::ui::UiRenderer;
use ra2_render::Renderer as MeshRenderer;
use ra2_shell::Shell;
use ra2sim::r#match::MatchState;
use ra2sim::DriveInput;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::audio::AudioSystem;
use crate::frame::FrameRenderer;
use crate::options::Options;
use crate::SHELL_SIZE;

/// The game's update cadence (`ra2.cfg`'s `UpdateInterval` is 50 ms).
const UPDATE: Duration = Duration::from_millis(50);

/// Open the shell in a window. Returns a clear message, not a panic, when the
/// machine has no display: the headless path is `--offscreen`.
pub fn run(game: &Path, size: [u32; 2], audio_report: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(
            "no display (DISPLAY and WAYLAND_DISPLAY are both unset); the windowed client \
             needs one — use --offscreen PNG for the headless path"
                .into(),
        );
    }

    let event_loop = EventLoop::new().map_err(|e| {
        format!("cannot open a window: {e}; use --offscreen PNG for the headless path")
    })?;
    let mut app = App::new(game, size)?;
    app.audio_report = audio_report;
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("the window loop failed: {e}"))?;
    print!("{}", app.audio.report(audio_report));
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Open the match in a window: the same composition the offscreen runner uses,
/// one frame per loop. `Esc` pauses, `C` cycles watch/static/orbit, and the
/// pause dialog's resume/quit buttons are hit-tested through the layout.
/// Open the Team HQ in a window. The same session the headless run uses: the
/// shipped `teamhq.gib` and its tabs, `event_bracket.gib` when an event is
/// entered, and M8's match presentation for the event's match.
pub fn run_hq(options: crate::hq_mode::Options, audio_report: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(
            "no display (DISPLAY and WAYLAND_DISPLAY are both unset); the windowed client \
             needs one — use --hq --out DIR for the headless path"
                .into(),
        );
    }
    let event_loop = EventLoop::new()
        .map_err(|e| format!("cannot open a window: {e}; use --hq --out DIR for the HQ"))?;
    let mut app = App::new_hq(options)?;
    app.audio_report = audio_report;
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("the window loop failed: {e}"))?;
    print!("{}", app.audio.report(audio_report));
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub fn run_match(options: crate::match_mode::Options, audio_report: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(
            "no display (DISPLAY and WAYLAND_DISPLAY are both unset); the windowed client \
             needs one — use --match --out DIR for the headless path"
                .into(),
        );
    }
    let event_loop = EventLoop::new()
        .map_err(|e| format!("cannot open a window: {e}; use --match --out DIR for the match"))?;
    let mut app = App::new_match(options)?;
    app.audio_report = audio_report;
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("the window loop failed: {e}"))?;
    print!("{}", app.audio.report(audio_report));
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Open the workshop in a window: the bot lab, its live build model, the 3D
/// placeholder preview and the editable panel.
pub fn run_build(options: crate::build_mode::Options, audio_report: bool) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(
            "no display (DISPLAY and WAYLAND_DISPLAY are both unset); the windowed client \
             needs one — use --build --out DIR for the headless path"
                .into(),
        );
    }
    let event_loop = EventLoop::new()
        .map_err(|e| format!("cannot open a window: {e}; use --build --out DIR"))?;
    let builder = crate::build_mode::Builder::open(&options.game, options.start.as_deref())?;
    let size = options.size;
    let mut app = App::with_builder(&options.game, size, builder)?;
    app.audio_report = audio_report;
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("the window loop failed: {e}"))?;
    print!("{}", app.audio.report(audio_report));
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct Gpu {
    /// Held so the surface's instance outlives it.
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    ui: UiRenderer,
    /// The 3D pipeline pair, on the same device as the surface.
    mesh: MeshRenderer,
    /// The 3D pass's depth buffer, recreated on resize.
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

impl Gpu {
    fn new(event_loop: &ActiveEventLoop, window: Arc<Window>) -> Result<Gpu, String> {
        // A display-handle instance is what a surface on Wayland/GLES needs;
        // the offscreen path's `Renderer::new()` deliberately stays without one.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("cannot create a window surface: {e}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no GPU adapter can draw this window: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ra2-app"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("cannot open the GPU device: {e}"))?;

        let size = window_size(&window);
        let mut config = surface
            .get_default_config(&adapter, size[0], size[1])
            .ok_or_else(|| "the window has no usable surface format".to_string())?;
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        config.present_mode = wgpu::PresentMode::Fifo;
        // Prefer a non-sRGB swapchain: the shader's values are the bytes the
        // offscreen PNG holds (the UI pipeline is deliberately not sRGB), and a
        // conversion on write would make the window a different picture.
        if let Some(format) = surface
            .get_capabilities(&adapter)
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
        {
            config.format = format;
        }
        surface.configure(&device, &config);
        let ui = UiRenderer::new_for(&device, config.format);
        let mesh =
            MeshRenderer::from_device(&device, &queue).map_err(|e| format!("3D renderer: {e}"))?;
        let (depth, depth_view) = depth_target(&device, size);
        Ok(Gpu {
            _instance: instance,
            surface,
            device,
            queue,
            config,
            ui,
            mesh,
            _depth: depth,
            depth_view,
        })
    }

    fn resize(&mut self, size: [u32; 2]) {
        if size[0] == 0 || size[1] == 0 {
            return;
        }
        self.config.width = size[0];
        self.config.height = size[1];
        self.surface.configure(&self.device, &self.config);
        let (depth, depth_view) = depth_target(&self.device, size);
        self._depth = depth;
        self.depth_view = depth_view;
    }

    fn configure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }
}

fn depth_target(device: &wgpu::Device, size: [u32; 2]) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("match-depth"),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn window_size(window: &Window) -> [u32; 2] {
    let size = window.inner_size();
    [size.width.max(1), size.height.max(1)]
}

struct App {
    requested: [u32; 2],
    shell: Shell,
    frames: FrameRenderer,
    /// The client's music, click and match-sound policy.
    audio: AudioSystem,
    /// The options screen's staged `ra2.cfg`.
    options: Options,
    /// Print every play call at exit.
    audio_report: bool,
    /// `FullScreen` from the startup file; the window opens borderless when set.
    full_screen: bool,
    /// `Some` when `--match` is playing; the shell stays booted so quitting the
    /// match returns to its main menu.
    session: Option<crate::match_mode::Session>,
    /// `Some` when `--build` is open: the workshop model and its preview.
    builder: Option<crate::build_mode::Builder>,
    /// `Some` when `--hq` is open: the shipped HQ and its event flow. The
    /// shell's own Team HQ hands its event bracket to one of these as well
    /// (`hq_from_shell`), and takes the player back when the event flow returns
    /// to the HQ.
    hq: Option<crate::hq_mode::Session>,
    /// The event flow was started from the shell's bracket, not `--hq`.
    hq_from_shell: bool,
    /// The workshop was opened from the shell's Team HQ, not `--build`.
    lab_from_shell: bool,
    /// The inventory robot the player last clicked in the shell's Team HQ.
    inventory_pick: Option<usize>,
    /// The modifiers currently held, for Ctrl+Z / Ctrl+Y.
    modifiers: ModifiersState,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    cursor: Option<PhysicalPosition<f64>>,
    /// The hover hook: the shell has no hover state yet, so the mapped point is
    /// kept here for the cursor pass (M7.8) and changes nothing.
    #[allow(dead_code)]
    hover: Option<[f32; 2]>,
    next_update: Instant,
    error: Option<String>,
}

impl App {
    fn new(game: &Path, requested: [u32; 2]) -> Result<App, String> {
        App::with_session(game, requested, None)
    }

    fn new_match(options: crate::match_mode::Options) -> Result<App, String> {
        let session = crate::match_mode::Session::new(&options)?;
        App::with_session(&options.game, options.size, Some(session))
    }

    fn with_builder(
        game: &Path,
        requested: [u32; 2],
        builder: crate::build_mode::Builder,
    ) -> Result<App, String> {
        let mut app = App::with_session(game, requested, None)?;
        app.builder = Some(builder);
        Ok(app)
    }

    fn new_hq(options: crate::hq_mode::Options) -> Result<App, String> {
        let mut hq = crate::hq_mode::Session::open(&options.game)?;
        hq.set_event_seconds(options.seconds);
        hq.set_report_damage(options.report_damage);
        let mut app = App::with_session(&options.game, options.size, None)?;
        app.hq = Some(hq);
        Ok(app)
    }

    fn with_session(
        game: &Path,
        requested: [u32; 2],
        session: Option<crate::match_mode::Session>,
    ) -> Result<App, String> {
        let mut shell = Shell::boot(game, [requested[0] as f32, requested[1] as f32])?;
        if let Some(view) = crate::hq_mode::shell_view(game) {
            shell.set_hq_view(view);
        }
        if let Some(hook) = crate::hq_mode::shell_enter_hook(game) {
            shell.set_enter_hook(hook);
        }
        shell.set_team_hook(crate::hq_mode::shell_team_hook(game));
        shell.set_animate(true);
        let frames = FrameRenderer::open(game)?;
        let cfg = crate::options::read_cfg(game);
        let audio = AudioSystem::new(game, &cfg).with_music_gap(true);
        let full_screen = cfg.full_screen();
        let options = Options::open_game(game);
        println!(
            "booted {} from {} ({}x{}); window target {}x{}{}",
            shell.screen_id(),
            game.display(),
            shell.size()[0],
            shell.size()[1],
            requested[0],
            requested[1],
            if full_screen { ", fullscreen" } else { "" }
        );
        Ok(App {
            requested,
            shell,
            frames,
            audio,
            options,
            audio_report: false,
            full_screen,
            session,
            builder: None,
            hq: None,
            hq_from_shell: false,
            lab_from_shell: false,
            inventory_pick: None,
            modifiers: ModifiersState::empty(),
            window: None,
            gpu: None,
            cursor: None,
            hover: None,
            next_update: Instant::now(),
            error: None,
        })
    }

    /// The match on screen: an exhibition/test match, or the Team HQ's event
    /// match.
    fn live_match(&self) -> Option<&crate::match_mode::Session> {
        self.session
            .as_ref()
            .or_else(|| self.hq.as_ref().and_then(|hq| hq.match_session()))
    }

    fn live_match_mut(&mut self) -> Option<&mut crate::match_mode::Session> {
        match &mut self.session {
            Some(session) => Some(session),
            None => self.hq.as_mut().and_then(|hq| hq.match_session_mut()),
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl Into<String>) {
        self.error = Some(error.into());
        event_loop.exit();
    }

    /// A window pixel in the shell's coordinates, or `None` in the bars.
    fn map_cursor(&self, position: PhysicalPosition<f64>) -> Option<[f32; 2]> {
        let window = self.window.as_ref()?;
        crate::viewport::to_shell(
            window_size(window),
            self.input_space(),
            [position.x as f32, position.y as f32],
        )
    }

    /// The coordinate space clicks are mapped into: the shell's native screen
    /// size, or the fixed design size for the tools that still lay out at it.
    fn input_space(&self) -> [f32; 2] {
        if let (true, Some(window)) = (self.session.is_some() || self.hq.is_some(), &self.window) {
            // The match and the Team HQ's event screens lay their windows out
            // natively on the window.
            let [w, h] = window_size(window);
            [w as f32, h as f32]
        } else if let Some(builder) = &self.builder {
            builder.size()
        } else {
            self.shell.size()
        }
    }

    /// Track the window: the shell lays out natively at the new size.
    fn relayout(&mut self, size: [u32; 2]) {
        if self.session.is_none() && self.builder.is_none() && self.hq.is_none() {
            self.shell.set_size([size[0] as f32, size[1] as f32]);
        }
    }

    /// The pause dialog's `options button` (`FUN_00476700`): the dialog
    /// closes and `options win` (`common/options.gib`) is shown modally over
    /// the paused match. The window lives on the shell's window stack, laid
    /// out natively on the window, and edits the same staged `ra2.cfg` as the
    /// main menu's.
    fn open_match_options(&mut self) -> Result<(), String> {
        if let Some(window) = &self.window {
            let [w, h] = window_size(window);
            self.shell.set_size([w as f32, h as f32]);
        }
        self.shell.open_modal(crate::options::SCREEN)?;
        if let Err(error) = self.options.reload() {
            eprintln!("options: {error}; keeping the staged values");
        }
        if let Some(document) = self.shell.document_mut() {
            self.options.sync(document);
        }
        if let Some(session) = self.live_match_mut() {
            session.open_options();
        }
        Ok(())
    }

    /// A click while the options window is over the match: the options
    /// controls, then the shell's own window handling (tabs, OK, Cancel).
    /// When the window closes the match resumes.
    fn match_options_click(&mut self, position: PhysicalPosition<f64>) -> Result<(), String> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let Some(point) = crate::viewport::to_shell(
            window_size(window),
            self.shell.size(),
            [position.x as f32, position.y as f32],
        ) else {
            return Ok(());
        };
        self.options_hit(point)?;
        match self.shell.click(point[0], point[1]) {
            Ok(step) => {
                println!("{}", crate::describe(&step));
                self.audio.shell_step(&step);
                self.apply_options_step(&step)?;
            }
            Err(error) => eprintln!("click {:.0},{:.0}: {error}", point[0], point[1]),
        }
        if Options::is_screen(&self.shell.screen_id()) {
            if let Some(document) = self.shell.document_mut() {
                self.options.sync(document);
            }
        } else if let Some(session) = self.live_match_mut() {
            session.close_options();
        }
        Ok(())
    }

    /// The options window's own controls (radios, sliders, checkboxes) under
    /// a click, applied to the staged `ra2.cfg`.
    fn options_hit(&mut self, point: [f32; 2]) -> Result<(), String> {
        if !Options::is_screen(&self.shell.screen_id()) {
            return Ok(());
        }
        if let Some(hit) = crate::options::hit(&self.shell, point) {
            if let Some(change) = self.options.click(&hit) {
                println!("options: {change}");
                self.options.save()?;
                self.audio.apply_cfg(self.options.cfg());
            }
        }
        Ok(())
    }

    /// The options window applies on OK and restores on Cancel.
    fn apply_options_step(&mut self, step: &ra2_shell::Step) -> Result<(), String> {
        match step.note.as_deref() {
            Some(crate::options::OPEN_NOTE) => {
                // Re-read the file each time the window opens; another
                // program may have edited it.
                if let Err(error) = self.options.reload() {
                    eprintln!("options: {error}; keeping the staged values");
                }
            }
            Some(crate::options::OK_NOTE) => {
                self.options.save()?;
                self.audio.apply_cfg(self.options.cfg());
            }
            Some(crate::options::CANCEL_NOTE) => {
                self.options.cancel()?;
                self.audio.apply_cfg(self.options.cfg());
            }
            _ => {}
        }
        Ok(())
    }

    fn set_hover(&mut self, point: Option<[f32; 2]>) {
        self.hover = point;
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            let size = window_size(window);
            self.relayout(size);
        }
        let (Some(window), Some(gpu)) = (&self.window, &mut self.gpu) else {
            return;
        };
        let size = window_size(window);
        if size != [gpu.config.width, gpu.config.height] {
            gpu.resize(size);
        }

        let current = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                gpu.configure();
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return;
            }
            _ => {
                self.error = Some("the surface rejected the frame".into());
                event_loop.exit();
                return;
            }
        };
        let view = current
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let format = gpu.config.format;
        if let Some(hq) = &mut self.hq {
            hq.set_layout(size);
            let screen = hq.audio_screen();
            self.audio.on_screen_opt(screen);
            let (scene, three_d) = match hq.scene() {
                Ok(scene) => scene,
                Err(error) => {
                    self.error = Some(format!("could not build the HQ frame: {error}"));
                    event_loop.exit();
                    return;
                }
            };
            if three_d {
                if let Err(error) =
                    hq.render_3d(&gpu.mesh, &view, &gpu.depth_view, format, size[0], size[1])
                {
                    self.error = Some(format!("could not draw the event match: {error}"));
                    event_loop.exit();
                    return;
                }
                if let Err(error) = gpu.ui.render_over_to(
                    &gpu.device,
                    &gpu.queue,
                    &scene,
                    &view,
                    format,
                    size[0],
                    size[1],
                ) {
                    self.error = Some(format!("could not draw the HQ match screen: {error}"));
                    event_loop.exit();
                    return;
                }
                if hq.match_session().is_some_and(|m| m.options_open()) {
                    let windows = self.frames.windows_scene(&self.shell);
                    if let Err(error) = gpu.ui.render_over_to(
                        &gpu.device,
                        &gpu.queue,
                        &windows,
                        &view,
                        format,
                        size[0],
                        size[1],
                    ) {
                        self.error = Some(format!("could not draw the options window: {error}"));
                        event_loop.exit();
                        return;
                    }
                }
            } else if let Err(error) = gpu.ui.render_to(
                &gpu.device,
                &gpu.queue,
                &scene,
                &view,
                format,
                size[0],
                size[1],
            ) {
                self.error = Some(format!("could not draw the HQ: {error}"));
                event_loop.exit();
                return;
            }
            gpu.queue.present(current);
            return;
        }
        if let Some(session) = &mut self.session {
            self.audio.stop_music("the match is on screen");
            session.set_layout(size);
            let scene = match session.scene() {
                Ok(scene) => scene,
                Err(error) => {
                    self.error = Some(format!("could not build the match frame: {error}"));
                    event_loop.exit();
                    return;
                }
            };
            if let Err(error) =
                session.render_3d(&gpu.mesh, &view, &gpu.depth_view, format, size[0], size[1])
            {
                self.error = Some(format!("could not draw the match: {error}"));
                event_loop.exit();
                return;
            }
            if let Err(error) = gpu.ui.render_over_to(
                &gpu.device,
                &gpu.queue,
                &scene,
                &view,
                format,
                size[0],
                size[1],
            ) {
                self.error = Some(format!("could not draw the HUD: {error}"));
                event_loop.exit();
                return;
            }
            if session.options_open() {
                let windows = self.frames.windows_scene(&self.shell);
                if let Err(error) = gpu.ui.render_over_to(
                    &gpu.device,
                    &gpu.queue,
                    &windows,
                    &view,
                    format,
                    size[0],
                    size[1],
                ) {
                    self.error = Some(format!("could not draw the options window: {error}"));
                    event_loop.exit();
                    return;
                }
            }
        } else if let Some(builder) = &mut self.builder {
            self.audio.on_screen("botlab/botlabwin.gib");
            if let Err(error) = builder.set_size([size[0] as f32, size[1] as f32]) {
                eprintln!("workshop: {error}");
            }
            let scene = match builder.scene(size) {
                Ok(scene) => scene,
                Err(error) => {
                    self.error = Some(format!("could not build the workshop frame: {error}"));
                    event_loop.exit();
                    return;
                }
            };
            if let Err(error) = gpu.ui.render_to(
                &gpu.device,
                &gpu.queue,
                &scene,
                &view,
                format,
                size[0],
                size[1],
            ) {
                self.error = Some(format!("could not draw the workshop: {error}"));
                event_loop.exit();
                return;
            }
        } else {
            let screen = self.shell.screen_id();
            self.audio.on_screen(&screen);
            if Options::is_screen(&screen) {
                if let Some(document) = self.shell.document_mut() {
                    self.options.sync(document);
                }
            }
            let frame = self.frames.frame(&mut self.shell, size);
            if let Err(error) = gpu.ui.render_to(
                &gpu.device,
                &gpu.queue,
                &frame.scene,
                &view,
                format,
                size[0],
                size[1],
            ) {
                self.error = Some(format!("could not draw the shell: {error}"));
                event_loop.exit();
                return;
            }
        }
        gpu.queue.present(current);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Open Robot Arena")
            .with_inner_size(LogicalSize::new(
                self.requested[0] as f64,
                self.requested[1] as f64,
            ))
            .with_min_inner_size(LogicalSize::new(320.0, 240.0))
            .with_fullscreen(self.full_screen.then_some(Fullscreen::Borderless(None)));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.fail(
                    event_loop,
                    format!("cannot create the shell window: {error}"),
                );
                return;
            }
        };
        let gpu = match Gpu::new(event_loop, window.clone()) {
            Ok(gpu) => gpu,
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        };
        self.window = Some(window);
        self.gpu = Some(gpu);
        self.next_update = Instant::now() + UPDATE;
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // The robot's keys change the player's input on press *and*
            // release: the held scan codes go to the robot's own wiring, which
            // decides what each does. The match still steps at its own fixed
            // cadence; this only changes the value sampled at the next tick.
            WindowEvent::KeyboardInput { event, .. }
                if self.live_match().is_some_and(|session| {
                    session.player_controlled
                        && !session.chat_open()
                        && (event.state == ElementState::Released
                            || !(session.reminder_open() || session.options_open()))
                }) && is_drive_key(&event.physical_key) =>
            {
                let pressed = event.state == ElementState::Pressed;
                if let Some(session) = self.live_match_mut() {
                    let mut input = session.drive();
                    apply_drive_key(&event.physical_key, pressed, &mut input);
                    session.set_drive(input);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.builder.is_some() && self.session.is_none() {
                    let ctrl = self.modifiers.control_key();
                    let mut action: Option<Result<(), String>> = None;
                    match event.physical_key {
                        // The lab is a shell window: Esc does nothing there
                        // (see the shell's Esc below).
                        PhysicalKey::Code(KeyCode::Escape) => {}
                        PhysicalKey::Code(KeyCode::Backspace) => {
                            if let Some(builder) = &mut self.builder {
                                builder.backspace();
                            }
                        }
                        PhysicalKey::Code(KeyCode::Enter) => {
                            if let Some(builder) = &mut self.builder {
                                action = Some(builder.enter());
                            }
                        }
                        PhysicalKey::Code(KeyCode::KeyZ) if ctrl => {
                            if let Some(builder) = &mut self.builder {
                                builder.undo();
                            }
                        }
                        PhysicalKey::Code(KeyCode::KeyY) if ctrl => {
                            if let Some(builder) = &mut self.builder {
                                builder.redo();
                            }
                        }
                        _ if !ctrl => {
                            if let Some(text) = &event.text {
                                if let Some(builder) = &mut self.builder {
                                    for c in text.chars() {
                                        builder.type_char(c);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    if let Some(Err(error)) = action {
                        self.fail(event_loop, error);
                        return;
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }
                let ctrl = self.modifiers.control_key();
                // The chat entry owns the keyboard while it is open
                // (`0x475ee0` opens it on `\`; the text field consumes the
                // rest). Enter/Escape close it; nothing is sent anywhere.
                if let Some(session) = self.live_match_mut() {
                    if session.chat_open() {
                        match event.physical_key {
                            PhysicalKey::Code(KeyCode::Escape | KeyCode::Enter) => {
                                session.chat_submit();
                            }
                            PhysicalKey::Code(KeyCode::Backspace) => session.chat_backspace(),
                            _ if !ctrl => {
                                if let Some(text) = &event.text {
                                    for c in text.chars() {
                                        session.chat_type(c);
                                    }
                                }
                            }
                            _ => {}
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                        return;
                    }
                }
                let in_match = self.live_match().is_some();
                if !in_match && self.hq.is_none() && self.shell.text_focus().is_some() {
                    // A focused shell text box (New Team's name and motto)
                    // takes the typing.
                    let typed = match event.physical_key {
                        PhysicalKey::Code(KeyCode::Backspace) => self.shell.backspace(),
                        PhysicalKey::Code(KeyCode::Escape) => false,
                        _ if !ctrl => event
                            .text
                            .as_ref()
                            .map(|text| {
                                text.chars()
                                    .fold(false, |any, c| self.shell.type_char(c) | any)
                            })
                            .unwrap_or(false),
                        _ => false,
                    };
                    if typed {
                        self.request_redraw();
                        return;
                    }
                }
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Backslash) if in_match => {
                        if let Some(session) = self.live_match_mut() {
                            session.toggle_chat();
                        }
                        self.request_redraw();
                    }
                    PhysicalKey::Code(KeyCode::Escape) => {
                        if let Some(session) = self.live_match_mut() {
                            // `0x475ee0`: Esc skips the intro, opens/closes the
                            // pause popup, and is ignored once the match is over.
                            session.escape();
                        } else if crate::flow::shell_escape(&mut self.shell) {
                            event_loop.exit();
                        }
                        // On the shell's screens and windows Esc does
                        // nothing, as in the original: the base window's key
                        // handler (`0x5ee6b0`) only forwards a key to the
                        // focused control, and the only handlers that test
                        // for Esc (`0x1b`) are the match window's
                        // (`0x475ee0`), the in-game popup's (`0x476a1b`) and
                        // the controls reminder's (`0x482a10`). The game is
                        // left through `Quit` or the window's close button.
                        self.request_redraw();
                    }
                    PhysicalKey::Code(KeyCode::F1) if in_match => {
                        if let Some(session) = self.live_match_mut() {
                            session.controls_key();
                        }
                        self.request_redraw();
                    }
                    _ if self
                        .live_match()
                        .is_some_and(|m| m.reminder_open() || m.options_open()) =>
                    {
                        // A modal window over the match owns the keys.
                    }
                    PhysicalKey::Code(code @ (KeyCode::F2 | KeyCode::F3 | KeyCode::F4))
                        if in_match =>
                    {
                        if let Some(session) = self.live_match_mut() {
                            session.camera_key(match code {
                                KeyCode::F2 => 2,
                                KeyCode::F3 => 3,
                                _ => 4,
                            });
                        }
                        self.request_redraw();
                    }
                    PhysicalKey::Code(KeyCode::Space) if in_match => {
                        if let Some(session) = self.live_match_mut() {
                            session.skip_intro();
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyC) if in_match => {
                        if let Some(session) = self.live_match_mut() {
                            session.cycle_camera();
                        }
                        self.request_redraw();
                    }
                    _ => {}
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some(position);
                let point = self.map_cursor(position);
                self.set_hover(point);
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.set_hover(None);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(session) = self.live_match_mut() {
                    let notches = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(position) => position.y as f32 / 40.0,
                    };
                    session.orbit_wheel(notches);
                    return;
                }
                if self.hq.is_some() || self.builder.is_some() {
                    return;
                }
                let Some(cursor) = self.cursor else {
                    return;
                };
                let Some(point) = self.map_cursor(cursor) else {
                    return;
                };
                // A wheel up is a smaller offset; winit's positive `y` is up.
                let rows = match delta {
                    MouseScrollDelta::LineDelta(_, y) => {
                        if y > 0.0 {
                            -1
                        } else if y < 0.0 {
                            1
                        } else {
                            0
                        }
                    }
                    MouseScrollDelta::PixelDelta(position) => {
                        if position.y > 0.0 {
                            -1
                        } else if position.y < 0.0 {
                            1
                        } else {
                            0
                        }
                    }
                };
                if rows != 0 && self.shell.scroll_at(point[0], point[1], rows) {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let Some(cursor) = self.cursor else {
                    return;
                };
                if self.live_match().is_some_and(|m| m.options_open()) {
                    if let Err(error) = self.match_options_click(cursor) {
                        self.fail(event_loop, error);
                        return;
                    }
                    self.request_redraw();
                    return;
                }
                let Some(point) = self.map_cursor(cursor) else {
                    // A click in a bar is no click at all.
                    return;
                };
                if let Some(hq) = &mut self.hq {
                    // The event match's own HUD and pause dialog come first; the
                    // results' Continue is the event flow's.
                    let mut abandon = false;
                    let mut open_options = false;
                    if let Some(session) = hq.match_session_mut() {
                        if session.matched.state() != MatchState::Results {
                            if session.skip_button_at(point) {
                                session.skip_intro();
                            } else {
                                match session.pause_button_at(point).as_deref() {
                                    Some("resume button") => session.toggle_pause(),
                                    Some("quit button") => {
                                        event_loop.exit();
                                        return;
                                    }
                                    Some("end match button") => abandon = true,
                                    Some("options button") => open_options = true,
                                    _ => {}
                                }
                            }
                            if open_options {
                                if let Err(error) = self.open_match_options() {
                                    eprintln!("options: {error}");
                                }
                                self.request_redraw();
                                return;
                            }
                            if !abandon {
                                self.request_redraw();
                                return;
                            }
                        }
                    }
                    let result = if abandon {
                        hq.abandon_match().map(|_| true)
                    } else {
                        hq.click(point)
                    };
                    if let Err(error) = result {
                        self.fail(event_loop, error);
                        return;
                    }
                    if self.hq_from_shell && hq.on_hq_screen() {
                        // The event flow is back on the HQ: the shell's own Team
                        // HQ takes over again, refreshed from the saved career.
                        self.hq = None;
                        self.hq_from_shell = false;
                        if let Err(error) = crate::flow::return_to_shell_hq(&mut self.shell) {
                            self.fail(event_loop, error);
                            return;
                        }
                        println!("event flow left; back on {}", self.shell.screen_id());
                    }
                    self.request_redraw();
                    return;
                }
                let mut quit = false;
                let mut open_options = false;
                if let Some(session) = &mut self.session {
                    if session.skip_button_at(point) {
                        session.skip_intro();
                    } else if session.matched.state() == MatchState::Results {
                        if session.results_button_at(point).is_some() {
                            quit = true;
                        }
                    } else {
                        match session.pause_button_at(point).as_deref() {
                            Some("resume button") => {
                                session.toggle_pause();
                            }
                            // result 8: the game exits (`0x4122d0`).
                            Some("quit button") => {
                                event_loop.exit();
                                return;
                            }
                            // result 4: back to the screen the match came from.
                            Some("end match button") => quit = true,
                            Some("options button") => open_options = true,
                            _ => {}
                        }
                    }
                }
                if open_options {
                    if let Err(error) = self.open_match_options() {
                        eprintln!("options: {error}");
                    }
                    self.request_redraw();
                    return;
                }
                if quit {
                    ra2script::set_paused(false);
                    let report = self.session.as_ref().map(|s| s.report());
                    self.session = None;
                    if let (Some(builder), Some(report)) = (&mut self.builder, report) {
                        builder.set_report(report);
                    }
                    println!("match quit; back on {}", self.shell.screen_id());
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }
                if self.session.is_some() {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }
                if self.builder.is_some() {
                    let target_size = self
                        .window
                        .as_ref()
                        .map(|w| window_size(w))
                        .unwrap_or([SHELL_SIZE[0] as u32, SHELL_SIZE[1] as u32]);
                    let mut test = None;
                    let mut left_lab = None;
                    if let Some(builder) = &mut self.builder {
                        if let Err(error) = builder.click(point[0], point[1]) {
                            self.fail(event_loop, error);
                            return;
                        }
                        if builder.take_test_request() {
                            test = Some(builder.test_options(target_size));
                        }
                        if self.lab_from_shell && builder.shell().lab().is_none() {
                            // `Back to HQ` / `Main Menu`: the workshop's shell
                            // left the lab; the same control takes the app's
                            // shell there.
                            left_lab = Some(crate::flow::lab_exit_control(builder.shell()));
                        }
                    }
                    if let Some(control) = left_lab {
                        let mut builder = self.builder.take().expect("the workshop is open");
                        self.lab_from_shell = false;
                        match crate::flow::close_workshop(&mut builder, &mut self.shell) {
                            Ok(Some(path)) => println!("workshop: saved {}", path.display()),
                            Ok(None) => {}
                            Err(error) => eprintln!("workshop: {error}"),
                        }
                        if let Some(control) = control {
                            match self.shell.click_named(&control) {
                                Ok(step) => println!("{}", crate::describe(&step)),
                                Err(error) => eprintln!("workshop: {error}"),
                            }
                        }
                        let screen = self.shell.screen_id();
                        self.audio.on_screen(&screen);
                        self.request_redraw();
                        return;
                    }
                    if let Some(result) = test {
                        // `Test Robot`: the player drives the robot just built.
                        match result
                            .and_then(|options| crate::match_mode::Session::new_player(&options))
                        {
                            Ok(session) => {
                                println!(
                                    "test robot: {} vs {}",
                                    session.report().robots[0].bot_name,
                                    session.report().robots[1].bot_name
                                );
                                self.session = Some(session);
                            }
                            Err(error) => {
                                self.fail(event_loop, error);
                                return;
                            }
                        }
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }
                if let Err(error) = self.options_hit(point) {
                    self.fail(event_loop, error);
                    return;
                }
                if let Some(index) = crate::flow::inventory_cell_at(&self.shell, point) {
                    self.inventory_pick = Some(index);
                }
                let screen_before = self.shell.base_screen_id();
                match self.shell.click(point[0], point[1]) {
                    Ok(step) => {
                        println!("{}", crate::describe(&step));
                        if crate::flow::quits(&screen_before, &step) {
                            // `Quit` (`0x486c30`): the game exits.
                            event_loop.exit();
                            return;
                        }
                        let on_bracket = screen_before
                            .eq_ignore_ascii_case(crate::flow::BRACKET_SCREEN)
                            && self.shell.window_depth() == 0;
                        if on_bracket && step.hit_widget() == Some("play button") {
                            // `Play Next Round!`: the event's match, with the
                            // player driving the entered robot.
                            match crate::flow::start_event_match(
                                self.shell.game(),
                                self.shell.entered_event(),
                                crate::flow::event_match_seconds(&self.shell),
                            ) {
                                Ok(hq) => {
                                    self.hq = Some(hq);
                                    self.hq_from_shell = true;
                                }
                                Err(error) => {
                                    self.fail(event_loop, error);
                                    return;
                                }
                            }
                            self.request_redraw();
                            return;
                        }
                        if on_bracket && step.hit_widget() == Some("forfeit button") {
                            if let Err(error) = crate::flow::forfeit_event(
                                self.shell.game(),
                                self.shell.entered_event(),
                            ) {
                                eprintln!("forfeit: {error}");
                            }
                            if let Some(view) = crate::hq_mode::shell_view(self.shell.game()) {
                                self.shell.set_hq_view(view);
                            }
                        }
                        let locked = self.shell.lab().is_some()
                            && self.builder.is_none()
                            && self.inventory_pick.is_some_and(|index| {
                                crate::flow::robot_in_event(self.shell.game(), index)
                                    || self.shell.entered_event().map(|(_, robot)| robot)
                                        == Some(index)
                            });
                        if locked {
                            // `FUN_004fee90`: the entered robot is not editable;
                            // the HQ stays, with the exe's message.
                            let _ = self.shell.click_named("back button");
                            if let Err(error) = self
                                .shell
                                .show_message("Robot Arena 2", crate::flow::ROBOT_IN_EVENT)
                            {
                                eprintln!("workshop: {error}");
                            }
                        }
                        if self.shell.lab().is_some() && self.builder.is_none() {
                            // The HQ's `workshop button`: the editable workshop on
                            // the robot the player selected.
                            match crate::flow::open_workshop(self.shell.game(), self.inventory_pick)
                            {
                                Ok(builder) => {
                                    println!(
                                        "workshop: {}",
                                        builder
                                            .team_target()
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|| "a new robot".to_string())
                                    );
                                    self.builder = Some(builder);
                                    self.lab_from_shell = true;
                                }
                                Err(error) => {
                                    self.fail(event_loop, error);
                                    return;
                                }
                            }
                        }
                        self.audio.shell_step(&step);
                        if let Err(error) = self.apply_options_step(&step) {
                            self.fail(event_loop, error);
                            return;
                        }
                        // The exhibition screen's own `start game button` note
                        // says "start the game"; it is not a shell link. Build
                        // the match from the choices the screen has selected.
                        if step.hit_widget() == Some(crate::play_mode::START_BUTTON) {
                            match crate::play_mode::session_from_shell(
                                self.shell.game(),
                                &self.shell,
                                None,
                                None,
                            ) {
                                Ok((session, _)) => {
                                    ra2script::set_paused(false);
                                    self.session = Some(session);
                                }
                                Err(error) => {
                                    self.fail(event_loop, error);
                                    return;
                                }
                            }
                        }
                        let screen = self.shell.screen_id();
                        if Options::is_screen(&screen) {
                            if let Some(document) = self.shell.document_mut() {
                                self.options.sync(document);
                            }
                        }
                        self.audio.on_screen(&screen);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    // A click the shell cannot follow (a screen the data does
                    // not ship) is reported, not fatal: the player stays on
                    // the screen they were on.
                    Err(error) => eprintln!("click {:.0},{:.0}: {error}", point[0], point[1]),
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize([size.width, size.height]);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.render(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_update {
            // One fixed update. The shell has no time-dependent drawing yet, so
            // its tick only asks for the next frame; the match advances at its
            // own `dt` through the accumulator.
            self.next_update = now + UPDATE;
            self.shell.advance(UPDATE.as_secs_f32());
            self.audio.tick_music(UPDATE.as_secs_f32());
            if let Some(session) = &mut self.session {
                session.advance(UPDATE.as_secs_f32());
            }
            if let Some(hq) = &mut self.hq {
                hq.advance(UPDATE.as_secs_f32());
            }
            if let Some(builder) = &mut self.builder {
                for sound in builder.take_sounds() {
                    self.audio.ui(sound);
                }
            }
            // One tick of the script sounds, heard from whichever match is
            // live. The HQ and the build mode own their own match sessions.
            let listener = if let Some(session) = &self.session {
                Some(session.audio_listener())
            } else {
                self.hq.as_ref().and_then(|hq| hq.audio_listener())
            };
            if let Some(listener) = listener {
                self.audio.drain_match(listener);
                let cues = if let Some(session) = &mut self.session {
                    session.take_cues()
                } else if let Some(hq) = &mut self.hq {
                    hq.take_cues()
                } else {
                    Vec::new()
                };
                self.audio.play_cues(cues, listener);
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_update));
    }
}

/// The DirectInput scan code of a key a robot's controls can be bound to.
///
/// A `.bot` stores scan codes (`W` is 17, Space 57), and the robot's own wiring
/// decides what each one does; the arrow keys are ours, standing in for
/// `W`/`A`/`S`/`D`. `C` stays the camera key.
fn dik_of(key: &PhysicalKey) -> Option<i32> {
    use KeyCode::*;
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    Some(match code {
        Digit1 => 2,
        Digit2 => 3,
        Digit3 => 4,
        Digit4 => 5,
        Digit5 => 6,
        Digit6 => 7,
        Digit7 => 8,
        Digit8 => 9,
        Digit9 => 10,
        Digit0 => 11,
        KeyQ => 16,
        KeyW | ArrowUp => 17,
        KeyE => 18,
        KeyR => 19,
        KeyT => 20,
        KeyY => 21,
        KeyU => 22,
        KeyI => 23,
        KeyO => 24,
        KeyP => 25,
        KeyA | ArrowLeft => 30,
        KeyS | ArrowDown => 31,
        KeyD | ArrowRight => 32,
        KeyF => 33,
        KeyG => 34,
        KeyH => 35,
        KeyJ => 36,
        KeyK => 37,
        KeyL => 38,
        KeyZ => 44,
        KeyX => 45,
        KeyV => 47,
        KeyB => 48,
        KeyN => 49,
        KeyM => 50,
        Space => 57,
        Numpad7 => 71,
        Numpad8 => 72,
        Numpad9 => 73,
        Numpad4 => 75,
        Numpad5 => 76,
        Numpad6 => 77,
        Numpad1 => 79,
        Numpad2 => 80,
        Numpad3 => 81,
        Numpad0 => 82,
        _ => return None,
    })
}

/// Whether a key is one a robot's controls can use.
fn is_drive_key(key: &PhysicalKey) -> bool {
    dik_of(key).is_some()
}

/// Apply a key edge to the keys held, by scan code. The robot's wiring turns
/// them into controls (`ra2sim::controls`).
fn apply_drive_key(key: &PhysicalKey, pressed: bool, input: &mut DriveInput) {
    if let Some(code) = dik_of(key) {
        input.keys.set(code, pressed);
    }
}
