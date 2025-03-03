use std::sync::Arc;

use penrose::{
    core::{hooks::ManageHook, State},
    extensions::hooks::manage::{DefaultTiled, FloatingFixed, FloatingRelative},
    manage_hooks,
    pure::geometry::{Rect, RelativeRect},
    x::{
        property::WmNormalHints,
        query::{ClassName, StringProperty, Title},
        Atom, Prop, Query, XConn,
    },
    x11rb::RustConn,
};
use x11rb::protocol::xproto::{ConnectionExt, Gravity};
use x11rb_protocol::protocol::xproto::AtomEnum;

use crate::BAR_HEIGHT_PX;

#[derive(Debug, PartialEq, Eq)]
struct OneOfQuery<C, Q, X> {
    strs: Arc<[&'static str]>,
    constr: Arc<C>,
    _phantom: std::marker::PhantomData<(Q, X)>,
}

impl<C, Q, X> Clone for OneOfQuery<C, Q, X> {
    fn clone(&self) -> Self {
        Self {
            strs: Arc::clone(&self.strs),
            constr: Arc::clone(&self.constr),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<C, Q, X> OneOfQuery<C, Q, X>
where
    X: XConn,
    Q: Query<X>,
    C: Fn(&'static str) -> Q,
{
    pub fn new(strs: &[&'static str], constr: C) -> Self {
        Self {
            strs: strs.into(),
            constr: constr.into(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<C, Q, X> Query<X> for OneOfQuery<C, Q, X>
where
    X: XConn,
    Q: Query<X>,
    C: Fn(&'static str) -> Q,
{
    fn run(&self, id: penrose::Xid, x: &X) -> penrose::Result<bool> {
        self.strs.iter().try_fold(false, |acc, title| {
            Ok(acc || (self.constr)(title).run(id, x)?)
        })
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
    fn user_specified(&self) -> Option<Rect>;

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

    fn user_specified(&self) -> Option<Rect> {
        let json = serde_json::to_value(self).ok()?;
        get_rect_from_value(json.get("user_specified")?)
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

struct StaticSizeHints;

impl StaticSizeHints {
    fn gravity_is_static(&self, id: penrose::Xid, x: &RustConn) -> penrose::Result<bool> {
        let atom = Atom::WmNormalHints.as_ref();
        let atom_xid = x.intern_atom(atom)?;

        let r = x
            .connection()
            .get_property(false, *id, atom_xid, AtomEnum::WM_SIZE_HINTS, 0, 1024)?
            .reply()?;

        let prop_type = match r.type_ {
            0 => return Ok(false),
            id => x.atom_name(penrose::Xid::from(id))?,
        };

        let raw = match prop_type.as_ref() {
            "WM_SIZE_HINTS" => r
                .value32()
                .ok_or_else(|| penrose::Error::InvalidPropertyData {
                    id,
                    prop: atom.to_owned(),
                    ty: prop_type.to_owned(),
                })?
                .collect::<Vec<_>>(),
            _ => {
                return Err(penrose::Error::InvalidHints {
                    reason: format!("expected WM_SIZE_HINTS but got {}", prop_type),
                })
            }
        };

        if raw.len() != 18 {
            return Err(penrose::Error::InvalidHints {
                reason: format!("raw bytes should be [u32; 18] but got [u32; {}]", raw.len()),
            });
        }

        let gravity = Gravity::from(raw[17]);
        tracing::trace!(?gravity, "gravity");

        Ok(gravity == Gravity::STATIC)
    }
}

impl Query<RustConn> for StaticSizeHints {
    fn run(&self, id: penrose::Xid, x: &RustConn) -> penrose::Result<bool> {
        tracing::trace!(%id, "checking if gravity is static");
        if !self.gravity_is_static(id, x)? {
            return Ok(false);
        }

        let p = Atom::WmNormalHints.as_ref();
        let Ok(Some(Prop::WmNormalHints(hints))) = x.get_prop(id, p) else {
            return Ok(false);
        };

        tracing::trace!(?hints, "gravity is static");

        let Some(spec) = hints.user_specified() else {
            return Ok(false);
        };
        tracing::trace!(?spec, "user_specified");

        Ok(spec.w > 0 && spec.h > 0)
    }
}

pub fn manage_hook<'a>() -> Box<dyn ManageHook<RustConn> + 'a> {
    let top_right_corner = RelativeRect::new(0.75, 0.0, 0.90, 0.10);
    let zoom_titles = OneOfQuery::new(ZOOM_TILE_TITLES, Title);
    let manage_hook = manage_hooks! {
        // Need this to handle the stupid zoom audio notifications
        // _NET_WM_NAME == "zoom", WM_NAME == "", and gravity is static
        StaticSizeHints
            .and(StringProperty(Atom::NetWmName.as_ref(), "zoom"))
            .and(StringProperty(Atom::WmName.as_ref(), ""))
                => NotificationUserSuggested::default(),
        ClassName("zoom").and(zoom_titles.clone()) => DefaultTiled,
        ClassName("zoom").and(zoom_titles.not()) => FloatingRelative(top_right_corner),
        ClassName("obsidian").and(
            OneOfQuery::new(&["Obsidian Help"], Title))
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

pub struct NotificationUserSuggested {
    width: f64,
    height: f64,
}

impl NotificationUserSuggested {
    pub fn new(width: f64, height: f64) -> Self {
        assert!(width > 0.0 && width <= 1.0);
        assert!(height > 0.0 && height <= 1.0);
        Self { width, height }
    }
}

impl Default for NotificationUserSuggested {
    fn default() -> Self {
        Self {
            width: 0.15,
            height: 0.05,
        }
    }
}

impl<X: XConn> ManageHook<X> for NotificationUserSuggested {
    fn call(&mut self, client: penrose::Xid, state: &mut State<X>, x: &X) -> penrose::Result<()> {
        let p = Atom::WmNormalHints.as_ref();
        let Ok(Some(Prop::WmNormalHints(hints))) = x.get_prop(client, p) else {
            return Ok(());
        };

        let spec = hints.user_specified();

        let r_screen = state.client_set.current_screen();
        const PADDING: f64 = 0.025;
        let r = spec.unwrap_or(
            RelativeRect::new(
                1.0 - PADDING,
                PADDING - self.height,
                self.width,
                self.height,
            )
            .applied_to(&r_screen.geometry()),
        );
        tracing::info!(?hints, ?r, "rect: from base");
        let top_left = RelativeRect::new(1.0 - PADDING, PADDING, PADDING, PADDING)
            .applied_to(&r_screen.geometry());
        let r = Rect::new(top_left.x - r.w, top_left.y, r.w, r.h);
        tracing::info!(?hints, ?r, ?top_left, "rect: from top_left");

        state.client_set.float(client, r)
    }
}
