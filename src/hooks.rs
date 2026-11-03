use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, ensure, Result};
use penrose::{
    core::{
        conn::{Conn, Query},
        hooks::ManageHook,
        State,
    },
    extensions::hooks::manage::{DefaultTiled, FloatingRelative},
    manage_hooks,
    pure::geometry::{Rect, RelativeRect},
    x::{property::WmNormalHints, Atom, Prop, XConn},
    WinId,
};

#[cfg(not(target_os = "macos"))]
use crate::BAR_HEIGHT_PX;
use crate::WmConn;

#[cfg(not(target_os = "macos"))]
mod x11 {
    use penrose::{
        core::conn::Query,
        x::{Atom, Prop, XConn},
        x11rb::RustConn,
        WinId,
    };
    use x11rb::protocol::xproto::{ConnectionExt, Gravity};
    use x11rb_protocol::protocol::xproto::AtomEnum;

    use super::WmNormalHintsExt;

    pub struct ConstrainedSizeHints;

    impl<X: XConn> Query<X> for ConstrainedSizeHints {
        fn run(&self, id: WinId, x: &mut X) -> penrose::Result<bool> {
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

    pub struct SubstringProperty(pub &'static str, pub &'static str);

    impl SubstringProperty {
        pub fn builder(prop: &'static str) -> impl Fn(&'static str) -> Self {
            move |s: &'static str| SubstringProperty(prop, s)
        }
    }

    impl<X: XConn> Query<X> for SubstringProperty {
        fn run(&self, id: WinId, x: &mut X) -> penrose::Result<bool> {
            match x.get_prop(id, self.0)? {
                Some(Prop::UTF8String(strs)) => {
                    Ok(strs.first().is_some_and(|value| value.contains(self.1)))
                }
                _ => Ok(false),
            }
        }
    }

    pub struct StaticSizeHints;

    impl StaticSizeHints {
        fn gravity_is_static(id: WinId, x: &mut RustConn) -> penrose::Result<bool> {
            let atom = Atom::WmNormalHints.as_ref();
            let atom_xid = x.intern_atom(atom)?;

            let r = x
                .connection()
                .get_property(false, *id, atom_xid, AtomEnum::WM_SIZE_HINTS, 0, 1024)?
                .reply()?;

            let prop_type = match r.type_ {
                0 => return Ok(false),
                id => x.atom_name(WinId::from(id))?,
            };

            let raw = match prop_type.as_ref() {
                "WM_SIZE_HINTS" => r
                    .value32()
                    .ok_or_else(|| penrose::Error::InvalidPropertyData {
                        id,
                        prop: atom.to_owned(),
                        ty: prop_type.clone(),
                    })?
                    .collect::<Vec<_>>(),
                _ => {
                    return Err(penrose::Error::InvalidHints {
                        reason: format!("expected WM_SIZE_HINTS but got {prop_type}"),
                    })
                }
            };

            if raw.len() != 18 {
                return Err(penrose::Error::InvalidHints {
                    reason: format!("raw bytes should be [u32; 18] but got [u32; {}]", raw.len()),
                });
            }

            let gravity = raw
                .get(17)
                .copied()
                .map(Gravity::from)
                .ok_or_else(|| penrose::Error::Custom("missing window gravity hint".to_owned()))?;
            tracing::trace!(?gravity, "gravity");

            Ok(gravity == Gravity::STATIC)
        }
    }

    impl Query<RustConn> for StaticSizeHints {
        fn run(&self, id: WinId, x: &mut RustConn) -> penrose::Result<bool> {
            tracing::trace!(%id, "checking if gravity is static");
            if !Self::gravity_is_static(id, x)? {
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
}

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
    X: Conn,
    Q: Query<X>,
    C: Fn(&'static str) -> Q + Send + Sync,
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
    X: Conn,
    Q: Query<X>,
    C: Fn(&'static str) -> Q + Send + Sync,
{
    fn run(&self, id: WinId, conn: &mut X) -> penrose::Result<bool> {
        self.strs.iter().try_fold(false, |acc, title| {
            Ok(acc || (self.constr)(title).run(id, conn)?)
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
        i32::try_from(value.get("x")?.as_u64()?).ok()?,
        i32::try_from(value.get("y")?.as_u64()?).ok()?,
        u32::try_from(value.get("w")?.as_u64()?).ok()?,
        u32::try_from(value.get("h")?.as_u64()?).ok()?,
    ))
}

#[cfg(not(target_os = "macos"))]
pub fn manage_hook<'a>() -> Box<dyn ManageHook<WmConn> + 'a> {
    use penrose::{
        extensions::hooks::manage::FloatingFixed,
        x::query::{ClassName, StringProperty, Title},
    };
    use x11::{ConstrainedSizeHints, StaticSizeHints, SubstringProperty};

    let top_right_corner = RelativeRect::new(0.75, 0.0, 0.90, 0.10);
    let zoom_titles = OneOfQuery::new(ZOOM_TILE_TITLES, Title);
    let _zoom_substrings: OneOfQuery<_, _, WmConn> = OneOfQuery::new(
        &["Zoom", "Meeting", "Settings", "Chat"],
        SubstringProperty::builder(Atom::NetWmName.as_ref()),
    );
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

#[cfg(target_os = "macos")]
mod macosx {
    use penrose::{core::conn::Query, Result, WinId};
    use penrosx::{AXUIElementAttributes, OsxConn};

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct AppName(pub &'static str);

    impl Query<OsxConn> for AppName {
        fn run(&self, id: WinId, conn: &mut OsxConn) -> Result<bool> {
            let Ok(app) = conn.app_for_window(id) else {
                return Ok(false);
            };
            Ok(app.axui_elem().title().is_ok_and(|title| title == self.0))
        }
    }

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct Title(pub &'static str);

    impl Query<OsxConn> for Title {
        fn run(&self, id: WinId, conn: &mut OsxConn) -> Result<bool> {
            let Ok(win) = conn.window(id) else {
                return Ok(false);
            };
            Ok(win.axui_elem().title().is_ok_and(|title| title == self.0))
        }
    }

    /// Matches transient popovers, hover tooltips, menus, sheets, and non-standard windows on macOS.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct IsTransientPopover;

    impl Query<OsxConn> for IsTransientPopover {
        fn run(&self, id: WinId, conn: &mut OsxConn) -> Result<bool> {
            let Ok(win) = conn.window(id) else {
                return Ok(true);
            };
            let ax = win.axui_elem();

            // Must have role "AXWindow" and subrole "AXStandardWindow" to be tiled
            let is_ax_window = ax.role().is_ok_and(|role| role == "AXWindow");
            let is_standard_subrole = ax
                .subrole()
                .is_ok_and(|subrole| subrole == "AXStandardWindow");

            // Standard macOS windows have a close button; hover cards/tooltips/popovers do not
            let has_close_button = ax.close_button().is_ok();

            Ok(!(is_ax_window && is_standard_subrole && has_close_button))
        }
    }
}

#[cfg(target_os = "macos")]
pub fn manage_hook<'a>() -> Box<dyn ManageHook<WmConn> + 'a> {
    use macosx::{AppName, IsTransientPopover, Title};
    use penrose::extensions::hooks::manage::FloatingCentered;

    let top_right_corner = RelativeRect::new(0.75, 0.0, 0.90, 0.10);
    let zoom_titles = OneOfQuery::new(ZOOM_TILE_TITLES, Title);
    let manage_hook = manage_hooks! {
        IsTransientPopover => IgnoreWindow,
        AppName("zoom.us").and(zoom_titles.clone()) => DefaultTiled,
        AppName("zoom.us").and(zoom_titles.not()) => FloatingRelative(top_right_corner),
        AppName("Obsidian").and(
            OneOfQuery::new(&["Obsidian Help"], Title))
                 => FloatingCentered::new(0.25, 0.25),
    };
    manage_hook
}

pub struct IsDock;

impl<X: XConn> Query<X> for IsDock {
    fn run(&self, id: WinId, x: &mut X) -> penrose::Result<bool> {
        use penrose::x::query::StringProperty;

        let window_type = Atom::NetWmWindowType.as_ref();

        Ok(
            StringProperty(window_type, Atom::NetWindowTypeDock.as_ref()).run(id, x)?
                || StringProperty(window_type, Atom::NetWindowTypeDesktop.as_ref()).run(id, x)?,
        )
    }
}

pub struct IgnoreWindow;

impl<C: Conn> ManageHook<C> for IgnoreWindow {
    fn call(&mut self, client: WinId, state: &mut State<C>, _: &mut C) -> penrose::Result<()> {
        let mut prev_focus_by_tag = HashMap::new();
        for ws in state.client_set.workspaces() {
            if let Some(stack) = ws.stack() {
                if let Some(&other) = stack.iter().find(|&&id| id != client) {
                    prev_focus_by_tag.insert(ws.tag().to_owned(), other);
                }
            }
        }
        state.client_set.remove_client(&client);
        for (tag, prev) in prev_focus_by_tag {
            if let Some(ws) = state.client_set.workspace_mut(&tag) {
                if let Some(stack) = ws.stack_mut() {
                    stack.focus_element(&prev);
                }
            }
        }
        Ok(())
    }
}

/// Float clients at the size and position suggested by `WmNormalHints` on the currently focused
/// screen. Otherwise, use the relative position and size of the client.
pub struct FloatingSuggestedCentered {
    width: f64,
    height: f64,
}

impl FloatingSuggestedCentered {
    /// Create a centered floating-window hook with relative dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error unless both dimensions are in `(0.0, 1.0]`.
    pub fn new(width: f64, height: f64) -> Result<Self> {
        ensure!(width > 0.0 && width <= 1.0);
        ensure!(height > 0.0 && height <= 1.0);
        Ok(Self { width, height })
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
    fn call(&mut self, client: WinId, state: &mut State<X>, x: &mut X) -> penrose::Result<()> {
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
        let r = centered_in_screen(r, r_screen.geometry())
            .map_err(|error| penrose::Error::Custom(format!("{error:#}")))?;
        tracing::trace!(%client, ?r, "client: applying size hints");

        state.client_set.float(client, r)
    }
}

pub struct NotificationUserSuggested {
    width: f64,
    height: f64,
}

impl NotificationUserSuggested {
    /// Create a notification hook with relative dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error unless both dimensions are in `(0.0, 1.0]`.
    pub fn new(width: f64, height: f64) -> Result<Self> {
        ensure!(width > 0.0 && width <= 1.0);
        ensure!(height > 0.0 && height <= 1.0);
        Ok(Self { width, height })
    }
}

fn centered_in_screen(rect: Rect, screen: Rect) -> Result<Rect> {
    rect.centered_in(&screen)
        .ok_or_else(|| anyhow!("window rectangle does not fit within the current screen"))
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
    fn call(&mut self, client: WinId, state: &mut State<X>, x: &mut X) -> penrose::Result<()> {
        const PADDING: f64 = 0.025;

        let p = Atom::WmNormalHints.as_ref();
        let Ok(Some(Prop::WmNormalHints(hints))) = x.get_prop(client, p) else {
            return Ok(());
        };

        let spec = hints.user_specified();

        let r_screen = state.client_set.current_screen();
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
        let r = Rect::new(top_left.x - r.w.cast_signed(), top_left.y, r.w, r.h);
        tracing::info!(?hints, ?r, ?top_left, "rect: from top_left");

        state.client_set.float(client, r)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_rect_from_json_value() {
        let value = json!({ "x": 12, "y": 34, "w": 56, "h": 78 });

        assert_eq!(get_rect_from_value(&value), Some(Rect::new(12, 34, 56, 78)));
    }

    #[test]
    fn parses_rect_at_integer_boundaries() {
        let value = json!({
            "x": 2_147_483_647_u64,
            "y": 2_147_483_647_u64,
            "w": 4_294_967_295_u64,
            "h": 4_294_967_295_u64,
        });

        assert_eq!(
            get_rect_from_value(&value),
            Some(Rect::new(i32::MAX, i32::MAX, u32::MAX, u32::MAX))
        );
    }

    #[test]
    fn rejects_rect_values_that_exceed_integer_boundaries() {
        let x_overflow = json!({ "x": 2_147_483_648_u64, "y": 0, "w": 1, "h": 1 });
        let width_overflow = json!({ "x": 0, "y": 0, "w": 4_294_967_296_u64, "h": 1 });

        assert_eq!(get_rect_from_value(&x_overflow), None);
        assert_eq!(get_rect_from_value(&width_overflow), None);
    }

    #[test]
    fn rejects_rect_with_missing_or_non_numeric_fields() {
        let missing_height = json!({ "x": 0, "y": 0, "w": 1 });
        let non_numeric_width = json!({ "x": 0, "y": 0, "w": "wide", "h": 1 });

        assert_eq!(get_rect_from_value(&missing_height), None);
        assert_eq!(get_rect_from_value(&non_numeric_width), None);
    }

    #[test]
    fn floating_hook_constructors_accept_valid_dimensions() {
        assert!(FloatingSuggestedCentered::new(0.5, 1.0).is_ok());
        assert!(NotificationUserSuggested::new(0.5, 1.0).is_ok());
    }

    #[test]
    fn floating_hook_constructors_reject_invalid_dimensions() {
        for (width, height) in [
            (0.0, 0.5),
            (-0.1, 0.5),
            (1.1, 0.5),
            (0.5, 0.0),
            (0.5, -0.1),
            (0.5, 1.1),
            (f64::NAN, 0.5),
            (0.5, f64::NAN),
        ] {
            assert!(FloatingSuggestedCentered::new(width, height).is_err());
            assert!(NotificationUserSuggested::new(width, height).is_err());
        }
    }

    #[test]
    fn centering_reports_rectangles_larger_than_the_screen() {
        let rect = Rect::new(0, 0, 200, 200);
        let screen = Rect::new(0, 0, 100, 100);

        assert!(centered_in_screen(rect, screen).is_err());
    }
}
