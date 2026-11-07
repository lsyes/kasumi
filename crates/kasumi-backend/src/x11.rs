//! Native X11 helpers used by overlay windows.
//!
//! Winit deliberately does not expose the Shape extension. For an override-
//! redirect IME popup the transparent shadow margin must not eat pointer
//! events, so we apply an input shape ourselves. The shape is set through a
//! second XCB connection by window id; it belongs to the server-side window and
//! therefore affects every client.

use anyhow::{Context as _, Result};
use kasumi_core::Rect;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;
use x11rb::connection::Connection as _;
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{ChangeGCAux, ConnectionExt as _, CreateGCAux, Rectangle};

pub fn set_input_region(window: &Window, rects: &[Rect]) -> Result<()> {
    let raw = window
        .window_handle()
        .context("window handle unavailable")?
        .as_raw();
    let window_id = match raw {
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return Ok(()),
    };
    let size = window.inner_size();
    if size.width == 0 || size.height == 0 {
        return Ok(());
    }
    let (connection, _) = x11rb::connect(None).context("connecting to the X11 display")?;
    connection
        .shape_query_version()
        .context("the X server lacks the Shape extension")?
        .reply()
        .context("Shape version request failed")?;

    let pixmap = connection.generate_id().context("allocating a pixmap id")?;
    connection
        .create_pixmap(1, pixmap, window_id, size.width as u16, size.height as u16)
        .context("creating the input-shape pixmap")?;
    let gc = connection.generate_id().context("allocating a GC id")?;
    connection
        .create_gc(gc, pixmap, &CreateGCAux::new().foreground(0))
        .context("creating the input-shape GC")?;
    // Zero everywhere first: outside the explicit rectangles is click-through.
    connection
        .poly_fill_rectangle(
            pixmap,
            gc,
            &[Rectangle {
                x: 0,
                y: 0,
                width: size.width as u16,
                height: size.height as u16,
            }],
        )
        .context("clearing the input-shape pixmap")?;
    if !rects.is_empty() {
        connection
            .change_gc(gc, &ChangeGCAux::new().foreground(1))
            .context("setting the input-shape color")?;
        let pixels: Vec<Rectangle> = rects
            .iter()
            .filter(|rect| !rect.is_empty())
            .map(|rect| Rectangle {
                x: rect.left().round().clamp(-32768.0, 32767.0) as i16,
                y: rect.top().round().clamp(-32768.0, 32767.0) as i16,
                width: rect.width().round().clamp(0.0, 65535.0) as u16,
                height: rect.height().round().clamp(0.0, 65535.0) as u16,
            })
            .collect();
        connection
            .poly_fill_rectangle(pixmap, gc, &pixels)
            .context("painting the input-shape rectangles")?;
    }
    connection
        .shape_mask(SO::SET, SK::INPUT, window_id, 0, 0, pixmap)
        .context("applying the X11 input shape")?;
    connection.free_gc(gc).ok();
    connection.free_pixmap(pixmap).ok();
    connection.flush().context("flushing the input shape")?;
    Ok(())
}
