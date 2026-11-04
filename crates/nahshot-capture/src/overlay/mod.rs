//! Native full-screen capture overlay spanning every monitor.
//!
//! A dedicated thread runs a winit event loop for the app's lifetime. Each capture
//! grabs all monitors, shows one borderless topmost window per monitor and shares a
//! single [`session::Session`] between them, so a selection can cross monitors. All
//! coordinates are physical pixels of the virtual desktop.

mod paint;
mod session;
pub mod toolbar;
mod window;

use crate::font::Font;
use crate::geom::PxRect;
use crate::grab::grab_all;
use crate::render::compose;
use chrono::{Local, NaiveDateTime};
use image::RgbaImage;
use log::{error, info, warn};
use session::{Key, Monitor, Outcome, Session};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::time::Instant;
use window::{Context, MonitorWindow};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::WindowId;

/// What the user chose to do with the capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Copy,
    SaveAs,
    Close,
}

/// The outcome of one capture.
#[derive(Debug)]
pub struct CaptureResult {
    pub action: Action,
    /// The selection with annotations applied; `None` only for `Close` without a selection.
    pub image: Option<RgbaImage>,
    pub captured_at: NaiveDateTime,
}

/// Requests sent to the overlay thread.
#[derive(Debug)]
pub(crate) enum UserEvent {
    Capture,
}

/// Cloneable, `Send` handle to the overlay thread.
#[derive(Clone)]
pub struct OverlayHandle {
    proxy: EventLoopProxy<UserEvent>,
}

impl OverlayHandle {
    /// Freezes the screen and opens the overlay (ignored if it is already open).
    pub fn trigger(&self) {
        if self.proxy.send_event(UserEvent::Capture).is_err() {
            error!("overlay thread is gone; capture request dropped");
        }
    }
}

/// Starts the overlay thread, which lives for the rest of the process.
///
/// `on_result` runs on that thread after each capture closes.
///
/// # Panics
/// If the thread or its event loop cannot be created.
pub fn spawn_overlay(on_result: impl Fn(CaptureResult) + Send + 'static) -> OverlayHandle {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("nahshot-overlay".into())
        .spawn(move || {
            // DPI awareness (per-monitor v2) is set by winit here, before any grab.
            let event_loop = EventLoop::<UserEvent>::with_user_event()
                .with_any_thread(true)
                .with_dpi_aware(true)
                .build()
                .expect("overlay event loop");
            tx.send(event_loop.create_proxy())
                .expect("spawn_overlay is waiting");
            let mut app = Overlay::new(Box::new(on_result));
            if let Err(e) = event_loop.run_app(&mut app) {
                error!("overlay event loop stopped: {e}");
            }
        })
        .expect("spawn overlay thread");
    OverlayHandle {
        proxy: rx.recv().expect("overlay thread failed to start"),
    }
}

/// The event-loop side: windows, context and the open session.
struct Overlay {
    on_result: Box<dyn Fn(CaptureResult)>,
    font: Option<Rc<Font>>,
    context: Option<Context>,
    windows: Vec<MonitorWindow>,
    session: Option<Session>,
    modifiers: ModifiersState,
    ime: bool,
}

impl Overlay {
    fn new(on_result: Box<dyn Fn(CaptureResult)>) -> Self {
        let font = Font::system()
            .inspect_err(|e| warn!("no system font, text tool disabled: {e}"))
            .ok()
            .map(Rc::new);
        Overlay {
            on_result,
            font,
            context: None,
            windows: Vec::new(),
            session: None,
            modifiers: ModifiersState::default(),
            ime: false,
        }
    }

    fn capture(&mut self, event_loop: &ActiveEventLoop) {
        if self.session.is_some() {
            return;
        }
        let started = Instant::now();
        let captured_at = Local::now().naive_local();
        let shots = match grab_all() {
            Ok(s) if !s.is_empty() => s,
            Ok(_) => return error!("capture failed: no monitors"),
            Err(e) => return error!("capture failed: {e}"),
        };
        let desktop = compose(&shots);
        let monitors: Vec<Monitor> = shots
            .iter()
            .map(|s| Monitor {
                area: PxRect {
                    x: s.origin.x,
                    y: s.origin.y,
                    w: s.image.width(),
                    h: s.image.height(),
                },
                scale: s.scale,
            })
            .collect();
        let ids: Vec<u32> = shots.iter().map(|s| s.id).collect();
        drop(shots);

        self.windows.retain(|w| ids.contains(&w.monitor));
        for (&id, m) in ids.iter().zip(&monitors) {
            match self.windows.iter_mut().find(|w| w.monitor == id) {
                Some(w) => w.place(m.area),
                None => match MonitorWindow::create(event_loop, id, m.area) {
                    Ok(w) => self.windows.push(w),
                    Err(e) => error!("overlay window for monitor {id}: {e}"),
                },
            }
        }
        let context = match &self.context {
            Some(c) => c,
            None => match Context::new(event_loop.owned_display_handle()) {
                Ok(c) => self.context.insert(c),
                Err(e) => return error!("softbuffer context: {e}"),
            },
        };
        let session = Session::new(desktop, monitors, self.font.clone(), captured_at);
        let mut shown = 0;
        for w in &mut self.windows {
            if let Err(e) = w.prepare(context, session.desktop()) {
                error!("overlay surface: {e}");
                continue;
            }
            w.show();
            match w.present(&session.scene()) {
                Ok(()) => shown += 1,
                Err(e) => error!("overlay present: {e}"),
            }
        }
        if shown == 0 {
            self.windows.iter_mut().for_each(MonitorWindow::release);
            return error!("capture failed: no overlay window could be shown");
        }
        info!("overlay_visible_ms={}", started.elapsed().as_millis());
        if let Some(w) = self.windows.first() {
            w.focus();
        }
        self.session = Some(session);
    }

    fn apply(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Nothing => {}
            Outcome::Redraw => self.invalidate(),
            Outcome::Finish(action) => self.finish(action),
        }
    }

    /// Repaints only the windows touched by what changed.
    fn invalidate(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if let Some(damage) = session.take_damage() {
            for w in self
                .windows
                .iter()
                .filter(|w| w.area.intersect(&damage).is_some())
            {
                w.request_redraw();
            }
        }
        let editing = session.editing();
        if editing != self.ime {
            self.ime = editing;
            self.windows
                .iter()
                .for_each(|w| w.window().set_ime_allowed(editing));
        }
    }

    fn finish(&mut self, action: Action) {
        let Some(session) = self.session.take() else {
            return;
        };
        for w in &mut self.windows {
            w.release();
        }
        if self.ime {
            self.ime = false;
            self.windows
                .iter()
                .for_each(|w| w.window().set_ime_allowed(false));
        }
        deliver(&*self.on_result, session.finish(action));
    }
}

/// Hands `result` to the caller's callback. A panic there is logged instead of
/// unwinding, since it would kill the overlay thread and with it every future capture.
fn deliver(on_result: &dyn Fn(CaptureResult), result: CaptureResult) {
    if let Err(panic) = catch_unwind(AssertUnwindSafe(|| on_result(result))) {
        let msg = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".into());
        error!("capture result handler panicked: {msg}");
    }
}

/// Maps a key press to an overlay command.
fn decode_key(event: &KeyEvent, modifiers: ModifiersState) -> Option<Key> {
    if event.state != ElementState::Pressed {
        return None;
    }
    match event.logical_key {
        WinitKey::Named(NamedKey::Escape) => return Some(Key::Escape),
        WinitKey::Named(NamedKey::Enter) => return Some(Key::Enter),
        WinitKey::Named(NamedKey::Backspace) => return Some(Key::Backspace),
        _ => {}
    }
    // AltGr arrives as Ctrl+Alt on Windows: treat it as typing, not a shortcut.
    if modifiers.control_key() && !modifiers.alt_key() {
        let WinitKey::Character(c) = event.key_without_modifiers() else {
            return None;
        };
        return match c.to_lowercase().as_str() {
            "c" => Some(Key::Copy),
            "s" => Some(Key::SaveAs),
            "z" => Some(Key::Undo),
            _ => None,
        };
    }
    let text = event.text.as_ref()?;
    (!text.is_empty() && !text.chars().any(char::is_control)).then(|| Key::Text(text.to_string()))
}

impl ApplicationHandler<UserEvent> for Overlay {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Capture => self.capture(event_loop),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(idx) = self.windows.iter().position(|w| w.id() == id) else {
            return;
        };
        if let WindowEvent::ScaleFactorChanged {
            mut inner_size_writer,
            ..
        } = event
        {
            // Keep covering the monitor exactly, in physical pixels, whatever the DPI.
            let area = self.windows[idx].area;
            let _ = inner_size_writer.request_inner_size(PhysicalSize::new(area.w, area.h));
            return;
        }
        if let WindowEvent::ModifiersChanged(m) = event {
            self.modifiers = m.state();
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let outcome = match event {
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.windows[idx].present(&session.scene()) {
                    error!("overlay present: {e}");
                }
                Outcome::Nothing
            }
            WindowEvent::CursorMoved { position, .. } => {
                let w = &self.windows[idx];
                let outcome = session.moved(w.to_desktop(position));
                w.window().set_cursor(session.cursor_icon());
                outcome
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => session.pressed(),
                ElementState::Released => session.released(),
            },
            WindowEvent::KeyboardInput { event, .. } => {
                decode_key(&event, self.modifiers).map_or(Outcome::Nothing, |k| session.key(k))
            }
            WindowEvent::Ime(Ime::Commit(text)) if session.editing() => {
                session.key(Key::Text(text))
            }
            WindowEvent::CloseRequested => Outcome::Finish(Action::Close),
            _ => Outcome::Nothing,
        };
        self.apply(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn result() -> CaptureResult {
        CaptureResult {
            action: Action::Close,
            image: None,
            captured_at: NaiveDateTime::default(),
        }
    }

    #[test]
    fn panicking_result_handler_does_not_unwind_into_the_overlay() {
        let calls = Cell::new(0);
        let handler = |_: CaptureResult| {
            calls.set(calls.get() + 1);
            panic!("caller bug");
        };
        deliver(&handler, result());
        deliver(&handler, result());
        assert_eq!(calls.get(), 2);
    }
}
