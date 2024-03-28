use penrose::{
    core::{
        hooks::{ManageHook, StateHook},
        State,
    },
    extensions::hooks::manage::{DefaultTiled, FloatingFixed, SetWorkspace},
    manage_hooks,
    pure::geometry::Rect,
    x::{
        query::{ClassName, StringProperty, Title},
        Atom, Query, XConn,
    },
};

use crate::BAR_HEIGHT_PX;

const ZOOM_TILE_TITLES: [&str; 5] = [
    "Zoom - Free Account",     // main window
    "Zoom - Licensed Account", // main window
    "Zoom",                    // meeting window on creation
    "Zoom Meeting",            // meeting window shortly after creation
    "Settings",                // settings window
];

struct ZoomTiledQuery;

impl<X: XConn> Query<X> for ZoomTiledQuery {
    fn run(&self, id: penrose::Xid, x: &X) -> penrose::Result<bool> {
        let zoom_class = ClassName("zoom").run(id, x)?;
        if !zoom_class {
            return Ok(false);
        }

        for title in ZOOM_TILE_TITLES.iter() {
            if Title(title).run(id, x)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

struct ZoomFloatQuery;

impl<X: XConn> Query<X> for ZoomFloatQuery {
    fn run(&self, id: penrose::Xid, x: &X) -> penrose::Result<bool> {
        let zoom_class = ClassName("zoom").run(id, x)?;
        if !zoom_class {
            return Ok(false);
        }

        for title in ZOOM_TILE_TITLES.iter() {
            if Title(title).run(id, x)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

pub fn manage_hook<'a, X: XConn + 'static>() -> Box<dyn ManageHook<X> + 'a> {
    let top_right_corner = Rect::new(0, 0, 500, 100);
    manage_hooks! {
        ZoomTiledQuery => DefaultTiled,
        ZoomFloatQuery => FloatingFixed(top_right_corner),
        IsDock => FloatingFixed(Rect::new(0, 0, 100, BAR_HEIGHT_PX)).then(IgnoreWindow),
        ClassName("stalonetray") => FloatingFixed(Rect::new(0, 0, 100, BAR_HEIGHT_PX)),
    }
}

pub struct IsDock;

impl<X: XConn> Query<X> for IsDock {
    fn run(&self, id: penrose::Xid, x: &X) -> penrose::Result<bool> {
        let window_type = Atom::NetWmWindowType.as_ref();

        Ok(
            StringProperty(window_type, Atom::NetWindowTypeDock.as_ref()).run(id, x)?
                || StringProperty(window_type, Atom::NetWindowTypeDesktop.as_ref()).run(id, x)?,
        )
    }
}

pub struct IgnoreWindow;

impl<X: XConn> ManageHook<X> for IgnoreWindow {
    fn call(&mut self, client: penrose::Xid, state: &mut State<X>, x: &X) -> penrose::Result<()> {
        state.client_set.remove_client(&client);
        Ok(())
    }
}

pub fn refresh_hooks<'a, X: XConn + 'a>() -> Box<dyn StateHook<X> + 'a> {
    Box::new(DockMoveHook)
}

struct DockMoveHook;

impl<X: XConn> StateHook<X> for DockMoveHook {
    fn call(&mut self, state: &mut State<X>, x: &X) -> penrose::Result<()> {
        todo!()
    }
}
