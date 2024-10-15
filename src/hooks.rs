use penrose::{
    core::{
        hooks::{ManageHook, StateHook},
        State,
    },
    extensions::hooks::manage::{DefaultTiled, FloatingFixed, FloatingRelative},
    manage_hooks,
    pure::geometry::{Rect, RelativeRect},
    x::{
        property::WmNormalHints,
        query::{ClassName, StringProperty, Title},
        Atom, Prop, Query, XConn,
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
    "Meeting",                           // meeting window while in meeting
    "Settings",                          // settings window
    "Meeting chat",                      // chat window while in meeting
    "Chat",                              // chat window shortly after creation
    "",                                  // main window before renamed to "Zoom Workplace"
];

// FIXME: horrible hack to get size hint Rects from WmNormalHints
trait WmNormalHintsExt {
    fn max(&self) -> Option<Rect>;
    #[allow(unused)]
    fn min(&self) -> Option<Rect>;
    fn base(&self) -> Option<Rect>;

    fn apply_to_elementwise(&self, mut r: Rect) -> Rect {
        if let Some(max) = self.max() {
            if r.w > max.w {
                r.w = max.w;
            }
            if r.h > max.h {
                r.h = max.h;
            }
        }

        if let Some(min) = self.min() {
            if r.w < min.w {
                r.w = min.w;
            }
            if r.h < min.h {
                r.h = min.h;
            }
        }

        r
    }
}

impl WmNormalHintsExt for WmNormalHints {
    fn max(&self) -> Option<Rect> {
        let json = serde_json::to_value(self).ok()?;
        get_rect_from_value(json.get("max")?)
    }

    fn min(&self) -> Option<Rect> {
        let json = serde_json::to_value(self).ok()?;
        get_rect_from_value(json.get("min")?)
    }

    fn base(&self) -> Option<Rect> {
        let json = serde_json::to_value(self).ok()?;
        get_rect_from_value(json.get("base")?)
    }
}

fn get_rect_from_value(value: &serde_json::Value) -> Option<Rect> {
    Some(Rect::new(
        value.get("x")?.as_u64()? as u32,
        value.get("y")?.as_u64()? as u32,
        value.get("w")?.as_u64()? as u32,
        value.get("h")?.as_u64()? as u32,
    ))
}

struct ConstrainedSizeHints;

impl<X: XConn> Query<X> for ConstrainedSizeHints {
    fn run(&self, id: penrose::Xid, x: &X) -> penrose::Result<bool> {
        let p = Atom::WmNormalHints.as_ref();
        let Ok(Some(Prop::WmNormalHints(hints))) = x.get_prop(id, p) else {
            return Ok(false);
        };

        let Some(min) = hints.min() else {
            return Ok(false);
        };
        let Some(max) = hints.max() else {
            return Ok(false);
        };

        Ok(min.w == max.w && min.h == max.h)
    }
}

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
         => FloatingSuggestedCentered::default(),
        // Windows that have max size == min size are not resizable, float them
        ConstrainedSizeHints => FloatingSuggestedCentered::default(),
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

/// Float clients at the size and position suggested by WmNormalHints on the currently focused
/// screen. Otherwise, use the relative position and size of the client.
pub struct FloatingSuggestedCentered {
    width: f64,
    height: f64,
}

impl FloatingSuggestedCentered {
    pub fn new(width: f64, height: f64) -> Self {
        assert!(width > 0.0 && width <= 1.0);
        assert!(height > 0.0 && height <= 1.0);
        Self { width, height }
    }
}

impl Default for FloatingSuggestedCentered {
    fn default() -> Self {
        Self {
            width: 0.25,
            height: 0.25,
        }
    }
}

impl<X: XConn> ManageHook<X> for FloatingSuggestedCentered {
    fn call(&mut self, client: penrose::Xid, state: &mut State<X>, x: &X) -> penrose::Result<()> {
        let p = Atom::WmNormalHints.as_ref();
        let Ok(Some(Prop::WmNormalHints(hints))) = x.get_prop(client, p) else {
            return Ok(());
        };

        let base = hints.base();

        let r_screen = state.client_set.current_screen();
        let r = base.unwrap_or(
            RelativeRect::new(0.0, 0.0, self.width, self.height).applied_to(&r_screen.geometry()),
        );
        tracing::trace!(?hints, ?r, "rect: from base");
        let r = hints.apply_to_elementwise(r);
        tracing::trace!(?hints, ?r, "rect: size hints applied");
        let r = r
            .centered_in(&r_screen.geometry())
            .expect("scaled to smaller than screen");
        tracing::trace!(%client, ?r, "client: applying size hints");

        state.client_set.float(client, r)
    }
}
