//! Monitor capture via `xcap`.

use crate::geom::PxPoint;
use crate::render::MonitorShot;
use crate::CaptureError;
use xcap::Monitor;

/// Captures every monitor in parallel.
///
/// `xcap::Monitor` is not `Send`, so each thread re-resolves its monitor by id.
pub fn grab_all() -> Result<Vec<MonitorShot>, CaptureError> {
    let ids = Monitor::all()
        .map_err(grab_err)?
        .iter()
        .map(|m| m.id().map_err(grab_err))
        .collect::<Result<Vec<_>, _>>()?;
    std::thread::scope(|s| {
        let handles: Vec<_> = ids
            .into_iter()
            .map(|id| s.spawn(move || grab_one(id)))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(CaptureError::Grab("capture thread panicked".into())))
            })
            .collect()
    })
}

fn grab_one(id: u32) -> Result<MonitorShot, CaptureError> {
    let monitors = Monitor::all().map_err(grab_err)?;
    let m = monitors
        .iter()
        .find(|m| m.id().is_ok_and(|i| i == id))
        .ok_or_else(|| CaptureError::Grab(format!("monitor {id} disappeared")))?;
    Ok(MonitorShot {
        id,
        origin: PxPoint {
            x: m.x().map_err(grab_err)?,
            y: m.y().map_err(grab_err)?,
        },
        scale: m.scale_factor().map_err(grab_err)?,
        image: m.capture_image().map_err(grab_err)?,
    })
}

fn grab_err(e: xcap::XCapError) -> CaptureError {
    CaptureError::Grab(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs a real display"]
    fn grab_all_returns_at_least_one_monitor() {
        let shots = grab_all().expect("grab");
        for s in &shots {
            println!(
                "monitor {} origin=({},{}) scale={} size={}x{}",
                s.id,
                s.origin.x,
                s.origin.y,
                s.scale,
                s.image.width(),
                s.image.height()
            );
        }
        assert!(!shots.is_empty());
    }
}
