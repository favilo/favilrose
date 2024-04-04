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
    x::{XConn, XConnExt},
    Error, Xid,
};

#[derive(Debug, Default)]
pub struct MouseHandler {
    data: Option<ClickData>,
}

#[derive(Debug)]
pub struct ClickData {
    start_point: Point,
    start_rect: Rect,
    xid: Xid,
    button: MouseButton,
}

impl Default for ClickData {
    fn default() -> Self {
        Self {
            start_point: Point::default(),
            start_rect: Rect::default(),
            xid: Xid::default(),
            button: MouseButton::Left,
        }
    }
}

impl MouseHandler {
    pub fn new() -> Self {
        Self { data: None }
    }

    pub fn start_drag<X: XConn>() -> Box<dyn MouseEventHandler<X>> {
        Box::new(
            move |e: &MouseEvent, s: &mut State<X>, x: &X| -> penrose::Result<()> {
                let cs = &mut s.client_set;
                let stack = cs.current_stack();
                let Some(stack) = stack else {
                    return Ok(());
                };
                let xid = *stack.focused();
                let client_rect = x.client_geometry(xid)?;
                // Keep the internal representation of the client in sync with the X server
                cs.float(xid, client_rect)?;

                let handler = s.extension::<Self>()?;
                let mut handler = handler.borrow_mut();
                if handler.data.is_none() {
                    handler.data = Some(ClickData {
                        start_point: e.rpt,
                        start_rect: client_rect,
                        xid,
                        button: e.state.button,
                    });
                } else {
                    return Err(Error::Custom("already dragging".to_string()));
                }
                // Don't call `x.refresh()` because it re-focuses the mouse
                Ok(())
            },
        )
    }

    pub fn stop_drag<X: XConn>() -> Box<dyn MouseEventHandler<X>> {
        Box::new(
            move |e: &MouseEvent, s: &mut State<X>, _x: &X| -> penrose::Result<()> {
                let handler = s.extension::<Self>()?;
                let mut handler = handler.borrow_mut();
                if handler.data.is_none() {
                    return Err(Error::Custom("no drag in progress".to_string()));
                };
                assert!(handler.data.as_ref().unwrap().button == e.state.button);
                handler.data = None;
                Ok(())
            },
        )
    }

    pub fn drag<X: XConn>() -> Box<dyn MouseEventHandler<X>> {
        Box::new(
            move |e: &MouseEvent, s: &mut State<X>, x: &X| -> penrose::Result<()> {
                let handler = &s.extension::<Self>()?;
                let handler = handler.borrow();
                let Some(ref data) = handler.data else {
                    return Err(Error::Custom("no drag in progress".to_string()));
                };

                let (dx, dy) = (
                    e.rpt.x as i32 - data.start_point.x as i32,
                    e.rpt.y as i32 - data.start_point.y as i32,
                );
                let mut new_rect = data.start_rect.clone();
                match data.button {
                    MouseButton::Left => {
                        new_rect.reposition(dx, dy);
                    }
                    MouseButton::Right => {
                        new_rect.resize(dx, dy);
                    }
                    MouseButton::Middle | MouseButton::ScrollUp | MouseButton::ScrollDown => {
                        // Don't handle these yet
                        return Ok(());
                    }
                };

                // Keep the internal representation of the client in sync with the X server
                let cs = &mut s.client_set;
                cs.float(data.xid, new_rect)?;

                x.position_client(data.xid, new_rect)?;
                // Don't call `x.refresh()` because it re-focuses the mouse

                Ok(())
            },
        )
    }
}

pub fn mouse_bindings<X>() -> MouseBindings<X>
where
    X: XConn,
{
    let mut map: MouseBindings<X> = HashMap::new();
    map.insert(
        (
            MouseEventKind::Motion,
            MouseState {
                button: MouseButton::ScrollDown,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        MouseHandler::drag(),
    );
    map.insert(
        (
            MouseEventKind::Press,
            MouseState {
                button: MouseButton::Left,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        MouseHandler::start_drag(),
    );
    map.insert(
        (
            MouseEventKind::Release,
            MouseState {
                button: MouseButton::Left,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        MouseHandler::stop_drag(),
    );
    map.insert(
        (
            MouseEventKind::Press,
            MouseState {
                button: MouseButton::Right,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        MouseHandler::start_drag(),
    );
    map.insert(
        (
            MouseEventKind::Release,
            MouseState {
                button: MouseButton::Right,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        MouseHandler::stop_drag(),
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
