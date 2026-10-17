//! The window: a `winit` event loop, a `wgpu` surface, and the same frames the
//! headless path draws.
//!
//! The shell has no time-dependent state yet, so the fixed update is a tick that
//! asks for a redraw; input is handled where it arrives. What the milestone
//! needs from this module is the shape: one `FrameRenderer`, a surface that is
//! reconfigured on resize, a click mapped through the scene's rectangle, and a
//! clean exit (Escape, the close button) rather than a panic when there is no
//! display.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ra2_render::ui::UiRenderer;
use ra2_render::Renderer as MeshRenderer;
use ra2_shell::Shell;
use ra2sim::r#match::MatchState;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::frame::FrameRenderer;
use crate::SHELL_SIZE;

/// The game's update cadence (`ra2.cfg`'s `UpdateInterval` is 50 ms).
const UPDATE: Duration = Duration::from_millis(50);

/// Open the shell in a window. Returns a clear message, not a panic, when the
/// machine has no display: the headless path is `--offscreen`.
pub fn run(game: &Path, size: [u32; 2]) -> Result<(), String> {
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
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("the window loop failed: {e}"))?;
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
pub fn run_hq(options: crate::hq_mode::Options) -> Result<(), String> {
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
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("the window loop failed: {e}"))?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub fn run_match(options: crate::match_mode::Options) -> Result<(), String> {
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
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("the window loop failed: {e}"))?;
    match app.error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Open the workshop in a window: the bot lab, its live build model, the 3D
/// placeholder preview and the editable panel.
pub fn run_build(options: crate::build_mode::Options) -> Result<(), String> {
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
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("the window loop failed: {e}"))?;
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
    /// `Some` when `--match` is playing; the shell stays booted so quitting the
    /// match returns to its main menu.
    session: Option<crate::match_mode::Session>,
    /// `Some` when `--build` is open: the workshop model and its preview.
    builder: Option<crate::build_mode::Builder>,
    /// `Some` when `--hq` is open: the shipped HQ and its event flow.
    hq: Option<crate::hq_mode::Session>,
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
        let shell = Shell::boot(game, SHELL_SIZE)?;
        let frames = FrameRenderer::open(game)?;
        println!(
            "booted {} from {} ({}x{}); window target {}x{}",
            shell.screen_id(),
            game.display(),
            shell.size()[0],
            shell.size()[1],
            requested[0],
            requested[1]
        );
        Ok(App {
            requested,
            shell,
            frames,
            session,
            builder: None,
            hq: None,
            modifiers: ModifiersState::empty(),
            window: None,
            gpu: None,
            cursor: None,
            hover: None,
            next_update: Instant::now(),
            error: None,
        })
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
            SHELL_SIZE,
            [position.x as f32, position.y as f32],
        )
    }

    fn set_hover(&mut self, point: Option<[f32; 2]>) {
        self.hover = point;
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
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
        } else if let Some(builder) = &mut self.builder {
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
            .with_min_inner_size(LogicalSize::new(320.0, 240.0));
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
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.builder.is_some() && self.session.is_none() {
                    let ctrl = self.modifiers.control_key();
                    let mut action: Option<Result<(), String>> = None;
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
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
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        if self.hq.is_some() {
                            event_loop.exit();
                            return;
                        }
                        let mut quit = false;
                        if let Some(session) = &mut self.session {
                            if session.matched.state() == MatchState::Results {
                                quit = true;
                            } else {
                                session.toggle_pause();
                            }
                        } else {
                            event_loop.exit();
                        }
                        if quit {
                            ra2script::set_paused(false);
                            let report = self.session.as_ref().map(|s| s.report());
                            self.session = None;
                            if let (Some(builder), Some(report)) = (&mut self.builder, report) {
                                builder.set_report(report);
                            }
                            println!("match over; back on {}", self.shell.screen_id());
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyC) if self.session.is_some() => {
                        if let Some(session) = &mut self.session {
                            session.cycle_camera();
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
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
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let Some(cursor) = self.cursor else {
                    return;
                };
                let Some(point) = self.map_cursor(cursor) else {
                    // A click in a bar is no click at all.
                    return;
                };
                if let Some(hq) = &mut self.hq {
                    match hq.click(point) {
                        Ok(_) => {
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                        Err(error) => self.fail(event_loop, error),
                    }
                    return;
                }
                let mut quit = false;
                if let Some(session) = &mut self.session {
                    if session.matched.state() == MatchState::Results {
                        if session.results_button_at(point).is_some() {
                            quit = true;
                        }
                    } else {
                        match session.pause_button_at(point).as_deref() {
                            Some("resume button") => {
                                session.toggle_pause();
                            }
                            Some("quit button") | Some("end match button") => quit = true,
                            _ => {}
                        }
                    }
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
                    if let Some(builder) = &mut self.builder {
                        if let Err(error) = builder.click(point[0], point[1]) {
                            self.fail(event_loop, error);
                            return;
                        }
                        if builder.take_test_request() {
                            test = Some(builder.test_options(target_size));
                        }
                    }
                    if let Some(result) = test {
                        match result.and_then(|options| crate::match_mode::Session::new(&options)) {
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
                match self.shell.click(point[0], point[1]) {
                    Ok(step) => {
                        println!("{}", crate::describe(&step));
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    Err(error) => self.fail(event_loop, error),
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
            if let Some(session) = &mut self.session {
                session.advance(UPDATE.as_secs_f32());
            }
            if let Some(hq) = &mut self.hq {
                hq.advance(UPDATE.as_secs_f32());
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_update));
    }
}
