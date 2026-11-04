//! One borderless, topmost window per monitor and its software surface.

use super::paint::{self, Frame, Scene};
use crate::geom::{PxPoint, PxRect};
use crate::render::Desktop;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::error::OsError;
use winit::event_loop::{ActiveEventLoop, OwnedDisplayHandle};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::window::{Window, WindowId, WindowLevel};

type Surface = softbuffer::Surface<OwnedDisplayHandle, Rc<Window>>;
/// Softbuffer context shared by every overlay window.
pub(crate) type Context = softbuffer::Context<OwnedDisplayHandle>;

/// A monitor's overlay window. Big buffers exist only while a capture is open.
pub(crate) struct MonitorWindow {
    /// The `xcap` monitor id this window covers.
    pub monitor: u32,
    /// Monitor bounds in virtual-desktop physical pixels.
    pub area: PxRect,
    window: Rc<Window>,
    surface: Option<Surface>,
    background: Vec<u32>,
}

impl MonitorWindow {
    /// Creates the (hidden) window covering `area`.
    pub fn create(
        event_loop: &ActiveEventLoop,
        monitor: u32,
        area: PxRect,
    ) -> Result<Self, OsError> {
        let attrs = Window::default_attributes()
            .with_title("nahshot")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(PhysicalPosition::new(area.x, area.y))
            .with_inner_size(PhysicalSize::new(area.w, area.h))
            .with_skip_taskbar(true)
            .with_drag_and_drop(false)
            .with_undecorated_shadow(false);
        let window = Rc::new(event_loop.create_window(attrs)?);
        Ok(MonitorWindow {
            monitor,
            area,
            window,
            surface: None,
            background: Vec::new(),
        })
    }

    /// The winit id of the window.
    pub fn id(&self) -> WindowId {
        self.window.id()
    }

    /// The winit window, for cursor and IME control.
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Converts a window-relative pointer position to virtual-desktop pixels.
    pub fn to_desktop(&self, p: PhysicalPosition<f64>) -> PxPoint {
        PxPoint {
            x: self.area.x + p.x.floor() as i32,
            y: self.area.y + p.y.floor() as i32,
        }
    }

    /// Re-applies the monitor's physical position and size (monitors may have moved).
    pub fn place(&mut self, area: PxRect) {
        self.area = area;
        self.window
            .set_outer_position(PhysicalPosition::new(area.x, area.y));
        let _ = self
            .window
            .request_inner_size(PhysicalSize::new(area.w, area.h));
    }

    /// Builds the veiled background and the drawing surface for a new capture.
    pub fn prepare(
        &mut self,
        context: &Context,
        desktop: &Desktop,
    ) -> Result<(), softbuffer::SoftBufferError> {
        self.background = paint::veiled(desktop, self.area);
        let mut surface = Surface::new(context, self.window.clone())?;
        let (w, h) = (non_zero(self.area.w), non_zero(self.area.h));
        surface.resize(w, h)?;
        self.surface = Some(surface);
        Ok(())
    }

    /// Shows the window on top of everything.
    pub fn show(&self) {
        self.window.set_visible(true);
    }

    /// Gives the window keyboard focus.
    pub fn focus(&self) {
        self.window.focus_window();
    }

    /// Schedules a repaint.
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Paints this monitor's slice of `scene` and presents it.
    pub fn present(&mut self, scene: &Scene) -> Result<(), softbuffer::SoftBufferError> {
        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };
        let mut buffer = surface.buffer_mut()?;
        buffer.copy_from_slice(&self.background);
        scene.paint(&mut Frame {
            px: &mut buffer,
            area: self.area,
        });
        buffer.present()
    }

    /// Hides the window and frees its pixel buffers.
    pub fn release(&mut self) {
        self.window.set_visible(false);
        self.surface = None;
        self.background = Vec::new();
    }
}

fn non_zero(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap_or(NonZeroU32::MIN)
}
