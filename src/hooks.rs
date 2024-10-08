use penrose::{
    core::{
        hooks::{ManageHook, StateHook},
        State,
    },
    extensions::hooks::manage::{DefaultTiled, FloatingCentered, FloatingFixed, FloatingRelative},
    manage_hooks,
    pure::geometry::{Rect, RelativeRect},
    x::{
        query::{ClassName, StringProperty, Title},
        Atom, Query, XConn,
    },
};

use crate::BAR_HEIGHT_PX;

struct Titles(Vec<&'static str>);

impl<X> Query<X> for Titles
where
    X: XConn,
{
    fn run(&self, id: penrose::Xid, x: &X) -> penrose::Result<bool> {
        self.0
            .iter()
            .try_fold(false, |acc, title| Ok(acc || Title(title).run(id, x)?))
    }
}

const ZOOM_TILE_TITLES: &[&str] = &[
    "Zoom - Free Account",               // main window
    "Zoom Workplace - Free account",     // main window
    "Zoom - Licensed Account",           // main window
    "Zoom Workplace - Licensed account", // main window
    "Zoom",                              // meeting window on creation
    "Zoom Workplace",                    // meeting window on creation
    "Zoom Meeting",                      // meeting window shortly after creation
    "Settings",                          // settings window
    "Metting Chat",                      // chat window
    "",                                  // main window before renamed to "Zoom Workplace"
];

pub fn manage_hook<'a, X: XConn + 'static>() -> Box<dyn ManageHook<X> + 'a> {
    let top_right_corner = RelativeRect::new(0.05, 0.0, 0.15, 0.10);
    let manage_hook = manage_hooks! {
        ClassName("zoom").and(
            Titles(ZOOM_TILE_TITLES.to_vec()))
        => DefaultTiled,
        ClassName("zoom").and(
            Titles(ZOOM_TILE_TITLES.to_vec()).not()
        ) => FloatingRelative(top_right_corner),
        ClassName("obsidian").and(
            Titles(vec!["Obsidian Help"]))
         => FloatingCentered::new(0.25, 0.5),
        IsDock => FloatingFixed(Rect::new(0, 0, 100, BAR_HEIGHT_PX)).then(IgnoreWindow),
        ClassName("stalonetray") => FloatingFixed(Rect::new(0, 0, 100, BAR_HEIGHT_PX)),
    };
    manage_hook
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
    fn call(&mut self, client: penrose::Xid, state: &mut State<X>, _: &X) -> penrose::Result<()> {
        state.client_set.remove_client(&client);
        Ok(())
    }
}

pub fn refresh_hooks<'a, X: XConn + 'a>() -> Box<dyn StateHook<X> + 'a> {
    Box::new(DockMoveHook)
}

struct DockMoveHook;

impl<X: XConn> StateHook<X> for DockMoveHook {
    fn call(&mut self, _state: &mut State<X>, _: &X) -> penrose::Result<()> {
        todo!()
    }
}
