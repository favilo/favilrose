use penrose::{
    core::{
        hooks::{ManageHook, StateHook},
        State,
    },
    extensions::hooks::manage::{FloatingFixed, SetWorkspace},
    manage_hooks,
    pure::geometry::Rect,
    x::{
        query::{ClassName, StringProperty},
        Atom, Query, XConn,
    },
};

use crate::BAR_HEIGHT_PX;

pub fn manage_hook<'a, X: XConn + 'static>() -> Box<dyn ManageHook<X> + 'a> {
    manage_hooks! {
        // ClassName("discord") => SetWorkspace("3"),
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
