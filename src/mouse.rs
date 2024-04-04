use std::collections::HashMap;

use penrose::{
    core::{
        bindings::{
            ModifierKey, MouseBindings, MouseButton, MouseEvent, MouseEventHandler, MouseEventKind,
            MouseState,
        },
        ClientSet, State,
    },
    pure::geometry::{Point, Rect},
    util::spawn_with_args,
    x::{XConn, XConnExt},
    Error, Xid,
};

#[derive(Debug, Default)]
pub struct MouseHandler {
    data: Option<ClickData>,
}

#[derive(Debug, Default)]
pub struct ClickData {
    start_point: Point,
    start_rect: Rect,
    xid: Xid,
}

impl MouseHandler {
    pub fn new() -> Self {
        Self { data: None }
    }

    pub fn start_left_drag<X: XConn>() -> Box<dyn MouseEventHandler<X>> {
        Box::new(
            move |e: &MouseEvent, s: &mut State<X>, x: &X| -> penrose::Result<()> {
                tracing::info!("Starting drag");
                let cs = &mut s.client_set;
                let stack = cs.current_stack();
                let Some(stack) = stack else {
                    return Ok(());
                };
                let xid = *stack.focused();
                let client_rect = x.client_geometry(xid)?;
                cs.float(xid, client_rect)?;

                let handler = s.extension::<Self>()?;
                let mut handler = handler.borrow_mut();
                if handler.data.is_none() {
                    handler.data = Some(ClickData {
                        start_point: e.rpt,
                        start_rect: client_rect,
                        xid,
                    });
                } else {
                    return Err(Error::Custom("already dragging".to_string()));
                }
                x.refresh(s)
            },
        )
    }

    pub fn stop_left_drag<X: XConn>() -> Box<dyn MouseEventHandler<X>> {
        Box::new(
            move |_e: &MouseEvent, s: &mut State<X>, _x: &X| -> penrose::Result<()> {
                tracing::info!("Stopping drag");
                let handler = s.extension::<Self>()?;
                let mut handler = handler.borrow_mut();
                if handler.data.is_none() {
                    return Err(Error::Custom("no drag in progress".to_string()));
                };
                tracing::info!(
                    "Done dragging window {} ",
                    handler.data.as_ref().unwrap().xid
                );
                handler.data = None;
                Ok(())
            },
        )
    }

    pub fn left_drag<X: XConn>() -> Box<dyn MouseEventHandler<X>> {
        Box::new(
            move |e: &MouseEvent, s: &mut State<X>, x: &X| -> penrose::Result<()> {
                tracing::info!("Dragging window");
                let handler = &s.extension::<Self>()?;
                let handler = handler.borrow();
                let Some(ref data) = handler.data else {
                    return Err(Error::Custom("no drag in progress".to_string()));
                };

                let (dx, dy) = (
                    e.rpt.x as i32 - data.start_point.x as i32,
                    e.rpt.y as i32 - data.start_point.y as i32,
                );

                tracing::info!("Dragging window {} by {},{}", data.xid, dx, dy);

                let mut new_rect = data.start_rect.clone();
                new_rect.reposition(dx, dy);

                let cs = &mut s.client_set;
                cs.float(data.xid, new_rect)?;

                x.refresh(s)
            },
        )
    }
}

pub fn mouse_bindings<X>() -> MouseBindings<X>
where
    X: XConn,
{
    let mut map: MouseBindings<X> = HashMap::new();
    // Float window with meta + left click
    map.insert(
        (
            MouseEventKind::Press,
            MouseState {
                button: MouseButton::Left,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        MouseHandler::start_left_drag(),
    );
    // Drag window with meta held + left button
    map.insert(
        (
            MouseEventKind::Motion,
            MouseState {
                button: MouseButton::Left,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        MouseHandler::left_drag(),
    );
    map.insert(
        (
            MouseEventKind::Release,
            MouseState {
                button: MouseButton::Left,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        MouseHandler::stop_left_drag(),
    );
    map
}

#[allow(unused)]
fn mouse_modify_with<F, X>(f: F) -> Box<dyn MouseEventHandler<X>>
where
    F: Fn(&mut ClientSet) + Clone + 'static,
    X: XConn,
{
    Box::new(move |_: &MouseEvent, s: &mut State<X>, x: &X| x.modify_and_refresh(s, f.clone()))
}
