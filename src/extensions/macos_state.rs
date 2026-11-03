use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use penrose::{
    core::{
        conn::Conn,
        hooks::{ManageHook, StateHook},
        State, WinId,
    },
    pure::{
        geometry::{Rect, RelativeRect, RelativeTo},
        Stack, StackSet,
    },
};
use serde::{Deserialize, Serialize};

use crate::WmConn;

const STATE_FILE: &str = "macos-state.json";
const FALLBACK_HIDDEN_TAG: &str = "9";

/// Metadata identifying a client window across WM restarts and system reboots.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WindowSnapshot {
    pub win_id: u32,
    pub owner: String,
    pub title: Option<String>,
    pub tag: String,
    pub floating: Option<RelativeRect>,
}

/// Persisted state for a single workspace.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceSnapshot {
    pub layout_name: String,
    pub stack_order: Vec<u32>,
    pub focused_win_id: Option<u32>,
}

/// Complete persisted window manager state for macOS.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PersistedState {
    pub focused_screen_index: usize,
    pub screen_tags: BTreeMap<usize, String>,
    pub workspaces: BTreeMap<String, WorkspaceSnapshot>,
    pub windows: Vec<WindowSnapshot>,
}

/// Return the path to `~/.local/state/favilrose/macos-state.json`.
///
/// # Errors
///
/// Returns an error when `HOME` is not set.
pub fn state_file_path() -> Result<PathBuf> {
    let state_home = match env::var_os("XDG_STATE_HOME") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?)
            .join(".local/state"),
    };

    Ok(state_home.join("favilrose").join(STATE_FILE))
}

/// Runtime state extension that coordinates restoring and persisting macOS workspace state.
pub struct MacosSessionState {
    path: PathBuf,
    restoring: Option<RestoringState>,
    matched_tags: HashMap<WinId, String>,
    matched_floating: HashMap<WinId, RelativeRect>,
    initial_refresh_pending: bool,
    known_pids: HashSet<i32>,
}

#[derive(Clone, Debug)]
struct RestoringState {
    persisted: PersistedState,
    claimed_indices: HashSet<usize>,
    new_to_old_win_id: HashMap<WinId, u32>,
}

impl MacosSessionState {
    /// Load any existing persisted state from `path` and prepare for startup restoration.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let persisted = load_persisted_state(&path)
            .inspect_err(|error| {
                tracing::debug!(?error, path = %path.display(), "no prior macOS state loaded");
            })
            .ok();

        Self {
            path,
            restoring: persisted.map(|persisted| RestoringState {
                persisted,
                claimed_indices: HashSet::new(),
                new_to_old_win_id: HashMap::new(),
            }),
            matched_tags: HashMap::new(),
            matched_floating: HashMap::new(),
            initial_refresh_pending: true,
            known_pids: HashSet::new(),
        }
    }
}

fn load_persisted_state(path: &Path) -> Result<PersistedState> {
    let contents = fs::read_to_string(path).context("read macos state file")?;
    serde_json::from_str(&contents).context("parse macos state file")
}

fn save_persisted_state(path: &Path, state: &PersistedState) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("state file path has no parent"))?;
    fs::create_dir_all(parent).context("create state directory")?;

    let tmp_path = path.with_extension("json.tmp");
    let serialized = serde_json::to_string_pretty(state).context("serialize macos state")?;
    fs::write(&tmp_path, serialized).context("write temporary macos state file")?;
    fs::rename(&tmp_path, path).context("replace macos state file")?;

    Ok(())
}

/// Match an incoming window against the persisted window list, claiming at most one entry.
fn claim_matching_window(
    restoring: &mut RestoringState,
    win_id: u32,
    owner: &str,
    title: Option<&str>,
) -> Option<WindowSnapshot> {
    let windows = &restoring.persisted.windows;

    // 1. Exact WinId + owner match (same login session / WM restart)
    if let Some((idx, snapshot)) = windows.iter().enumerate().find(|(idx, w)| {
        !restoring.claimed_indices.contains(idx) && w.win_id == win_id && w.owner == owner
    }) {
        restoring.claimed_indices.insert(idx);
        return Some(snapshot.clone());
    }

    // 2. Exact owner + title match (across reboot/relaunch)
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        if let Some((idx, snapshot)) = windows.iter().enumerate().find(|(idx, w)| {
            !restoring.claimed_indices.contains(idx)
                && w.owner == owner
                && w.title.as_deref() == Some(title)
        }) {
            restoring.claimed_indices.insert(idx);
            return Some(snapshot.clone());
        }
    }

    // 3. Fallback: owner-only match (across reboot when window title changed)
    if let Some((idx, snapshot)) = windows
        .iter()
        .enumerate()
        .find(|(idx, w)| !restoring.claimed_indices.contains(idx) && w.owner == owner)
    {
        restoring.claimed_indices.insert(idx);
        return Some(snapshot.clone());
    }

    None
}

fn is_at_hide_corner(bounds: Rect, screens: &[Rect]) -> bool {
    screens.iter().any(|screen| {
        let hide_x = screen.x + screen.w.cast_signed() - 1;
        let hide_y = screen.y + screen.h.cast_signed() - 1;
        (bounds.x - hide_x).abs() <= 2 && (bounds.y - hide_y).abs() <= 2
    })
}

fn apply_workspace_and_screen_state(
    client_set: &mut StackSet<WinId>,
    restoring: &RestoringState,
    matched_floating: &HashMap<WinId, RelativeRect>,
) -> bool {
    let persisted = &restoring.persisted;
    let mut changed = false;

    // Restore per-workspace layout and stack order/focus
    for (tag, ws_snap) in &persisted.workspaces {
        let Some(ws) = client_set.workspace_mut(tag) else {
            continue;
        };

        if !ws_snap.layout_name.is_empty() && ws.layout_name() != ws_snap.layout_name {
            ws.set_layout_by_name(&ws_snap.layout_name);
            changed = true;
        }

        if ws_snap.stack_order.is_empty() {
            continue;
        }

        let current_clients: Vec<WinId> = ws.clients().copied().collect();
        if current_clients.len() > 1 {
            let mut ordered: Vec<WinId> = ws_snap
                .stack_order
                .iter()
                .filter_map(|&saved_id| {
                    current_clients.iter().copied().find(|client| {
                        restoring
                            .new_to_old_win_id
                            .get(client)
                            .copied()
                            .unwrap_or(**client)
                            == saved_id
                    })
                })
                .collect();

            for client in &current_clients {
                if !ordered.contains(client) {
                    ordered.push(*client);
                }
            }

            let focused = ws_snap
                .focused_win_id
                .and_then(|saved_focus| {
                    ordered.iter().copied().find(|client| {
                        restoring
                            .new_to_old_win_id
                            .get(client)
                            .copied()
                            .unwrap_or(**client)
                            == saved_focus
                    })
                })
                .or_else(|| ws.focus().copied())
                .or_else(|| ordered.first().copied());

            if let (Some(focused), Some(mut new_stack)) = (focused, Stack::try_from_iter(ordered)) {
                new_stack.focus_element(&focused);
                if let Some(stack_mut) = ws.stack_mut() {
                    if *stack_mut != new_stack {
                        *stack_mut = new_stack;
                        changed = true;
                    }
                }
            }
        }
    }

    // Restore active workspace tags on each screen
    for (&screen_index, tag) in &persisted.screen_tags {
        if client_set.contains_tag(tag) && client_set.tag_for_screen(screen_index) != Some(tag) {
            client_set.focus_screen(screen_index);
            client_set.pull_tag_to_screen(tag);
            changed = true;
        }
    }
    if client_set.current_screen().index() != persisted.focused_screen_index {
        client_set.focus_screen(persisted.focused_screen_index);
        changed = true;
    }

    // Restore floating rects for visible clients
    for (&client, &rel_rect) in matched_floating {
        if let Some(screen) = client_set.screen_for_client(&client) {
            let rect = rel_rect.applied_to(&screen.geometry());
            if client_set.float(client, rect).is_ok() {
                changed = true;
            }
        }
    }

    changed
}

/// Event hook that filters noisy macOS events and stabilizes focus across workspace/popup transitions.
pub struct MacosSessionEventHook;

impl penrose::core::hooks::EventHook<WmConn> for MacosSessionEventHook {
    fn call(
        &mut self,
        event: &penrosx::Event,
        state: &mut State<WmConn>,
        conn: &mut WmConn,
    ) -> penrose::Result<bool> {
        use penrosx::Event;

        match *event {
            Event::AppDeactivated { .. }
            | Event::WindowMoved { .. }
            | Event::WindowResized { .. } => Ok(false),

            // When a new app launches, enable AXManualAccessibility for its PID once.
            Event::AppLaunched { .. } => {
                if let Ok(session_rc) = state.extension::<MacosSessionState>() {
                    let mut session = session_rc.borrow_mut();
                    enable_manual_accessibility_for_apps(conn, &mut session.known_pids);
                }
                Ok(true)
            }

            // When macOS fires AppActivated or FocusedWindowChanged:
            // 1. If the app has a non-standard popup/menu/sheet focused, or if the focused window
            //    is already the current client, skip OsxConn's handler so it doesn't synthesize
            //    a mouse click on the main window (which immediately closes right-click menus)
            //    or jump to another workspace during unlock/sleep wake.
            // 2. Only allow AppActivated/FocusedWindowChanged to switch focus if the target
            //    window is on a currently visible screen workspace (never auto-jump to a hidden
            //    workspace just because a background app woke up after unlock!).
            Event::AppActivated { pid } | Event::FocusedWindowChanged { pid } => {
                if let Ok(session_rc) = state.extension::<MacosSessionState>() {
                    let mut session = session_rc.borrow_mut();
                    if !session.known_pids.contains(&pid) {
                        enable_manual_accessibility_for_apps(conn, &mut session.known_pids);
                    }
                }

                // Check if the current focused client already belongs to `pid`:
                // If the user is already in this app (e.g. opening a right-click menu or hover card),
                // do NOT run OsxConn::handle_event, which would re-sync all windows and call
                // make_key_window (synthesizing a mouse click that dismisses context menus!).
                if let Some(&focused_id) = state.client_set.current_client() {
                    if conn.client_pid(focused_id) == Some(pid.cast_unsigned()) {
                        return Ok(false);
                    }
                }

                // Check if `pid` has a managed client on a currently visible screen workspace
                let visible_candidates: Vec<WinId> = state
                    .client_set
                    .on_screen_workspace_clients()
                    .copied()
                    .collect();
                let visible_client = visible_candidates
                    .into_iter()
                    .find(|&id| conn.client_pid(id) == Some(pid.cast_unsigned()));

                let Some(target_id) = visible_client else {
                    // The activated app has no window on the active workspace (e.g. background app
                    // waking up after unlock or on another virtual workspace) -> ignore so we don't
                    // flicker between workspaces!
                    return Ok(false);
                };

                // Update internal Penrose focus to `target_id` without calling `make_key_window`
                // (since macOS already activated the app/window that generated this event!).
                state.client_set.focus_client(&target_id);
                Ok(false)
            }

            // When WindowCreated fires (e.g. a Chrome hover card, right-click menu, or a real new window):
            // Check if there are actually any new standard (non-transient) windows for `pid` before
            // letting OsxConn::handle_event run `manage_new_windows` + `refresh` (which calls
            // `make_key_window` and closes right-click menus!).
            Event::WindowCreated { pid } => {
                let old_ids: HashSet<WinId> = state.client_set.clients().copied().collect();
                conn.update_known_apps_and_windows();
                let window_ids: Vec<WinId> = conn.windows().keys().copied().collect();
                let has_new_standard_window = window_ids.into_iter().any(|id| {
                    conn.client_pid(id) == Some(pid.cast_unsigned())
                        && !old_ids.contains(&id)
                        && !is_transient_window(id, conn)
                });

                Ok(has_new_standard_window)
            }

            // When WindowDestroyed or UiElementDestroyed fires for a transient popover/menu that
            // we never managed in `state.client_set`, skip OsxConn::handle_event so it doesn't
            // trigger `unmanage` -> `refresh` -> `make_key_window`!
            Event::WindowDestroyed { id } | Event::UiElementDestroyed { id } => {
                Ok(state.client_set.contains(&id))
            }

            _ => Ok(true),
        }
    }
}

/// Startup hook that ensures workspace `"9"` is available, enables Electron accessibility, and initializes `MacosSessionState`.
pub struct MacosSessionStartupHook;

impl StateHook<WmConn> for MacosSessionStartupHook {
    fn call(&mut self, state: &mut State<WmConn>, conn: &mut WmConn) -> penrose::Result<()> {
        if !state.client_set.contains_tag(FALLBACK_HIDDEN_TAG) {
            let layouts = state.config.default_layouts.clone();
            let _ = state
                .client_set
                .add_workspace(FALLBACK_HIDDEN_TAG, layouts);
        }

        if state.extension::<MacosSessionState>().is_err() {
            let path = state_file_path()
                .map_err(|error| penrose::Error::Custom(format!("{error:#}")))?;
            state.add_extension(MacosSessionState::new(path));
        }

        if let Ok(session_rc) = state.extension::<MacosSessionState>() {
            let mut session = session_rc.borrow_mut();
            enable_manual_accessibility_for_apps(conn, &mut session.known_pids);
        }

        Ok(())
    }
}

/// Manage hook that restores each window to its saved workspace tag or moves unmatched windows to `"9"`.
pub struct MacosSessionManageHook;

impl ManageHook<WmConn> for MacosSessionManageHook {
    fn call(
        &mut self,
        client: WinId,
        state: &mut State<WmConn>,
        conn: &mut WmConn,
    ) -> penrose::Result<()> {
        let Ok(session_rc) = state.extension::<MacosSessionState>() else {
            return Ok(());
        };
        let mut session = session_rc.borrow_mut();

        // Only run restoration logic during initial startup (or if window is at the hide corner)
        let screens = conn.screen_details().unwrap_or_default();
        let bounds = conn.client_geometry(client).unwrap_or_default();
        let hidden_corner = is_at_hide_corner(bounds, &screens);

        if is_transient_window(client, conn) {
            return Ok(());
        }

        if !session.initial_refresh_pending && !hidden_corner {
            return Ok(());
        }

        let (owner, title) = window_identity(client, conn);
        let matched = session.restoring.as_mut().and_then(|restoring| {
            let snapshot = claim_matching_window(restoring, *client, &owner, title.as_deref())?;
            restoring.new_to_old_win_id.insert(client, snapshot.win_id);
            Some(snapshot)
        });

        let target_tag = match matched {
            Some(snapshot) => {
                if let Some(floating) = snapshot.floating {
                    session.matched_floating.insert(client, floating);
                }
                snapshot.tag
            }
            None => FALLBACK_HIDDEN_TAG.to_owned(),
        };

        if state.client_set.contains_tag(&target_tag) {
            session.matched_tags.insert(client, target_tag.clone());
            state.client_set.move_client_to_tag(&client, &target_tag);
        }

        Ok(())
    }
}

/// Refresh hook that finalizes initial restoration on the first refresh and persists state thereafter.
pub struct MacosSessionRefreshHook;

impl StateHook<WmConn> for MacosSessionRefreshHook {
    fn call(&mut self, state: &mut State<WmConn>, conn: &mut WmConn) -> penrose::Result<()> {
        let Ok(session_rc) = state.extension::<MacosSessionState>() else {
            return Ok(());
        };

        let restored = {
            let mut session = session_rc.borrow_mut();
            if session.initial_refresh_pending {
                session.initial_refresh_pending = false;
                let matched_floating = std::mem::take(&mut session.matched_floating);
                session
                    .restoring
                    .take()
                    .map(|restoring| (restoring, matched_floating))
            } else {
                None
            }
        };

        if let Some((restoring, matched_floating)) = restored {
            use penrose::core::conn::ConnExt;

            if apply_workspace_and_screen_state(
                &mut state.client_set,
                &restoring,
                &matched_floating,
            ) {
                conn.refresh(state)?;
                return Ok(());
            }
        }

        let snapshot = snapshot_current_state(&state.client_set, conn);
        let path = session_rc.borrow().path.clone();
        if let Err(error) = save_persisted_state(&path, &snapshot) {
            tracing::warn!(?error, path = %path.display(), "failed to persist macOS window state");
        }

        Ok(())
    }
}

fn enable_manual_accessibility_for_apps(conn: &mut WmConn, known_pids: &mut HashSet<i32>) {
    use core_foundation::{
        base::{CFTypeRef, TCFType},
        boolean::CFBoolean,
        string::{CFString, CFStringRef},
    };
    use std::ffi::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementSetAttributeValue(
            element: *const c_void,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> i32;
    }

    conn.update_known_apps_and_windows();
    let attr = CFString::from_static_string("AXManualAccessibility");
    let val = CFBoolean::true_value();
    let mut added_any = false;

    for (&pid, app) in conn.apps() {
        if known_pids.insert(pid) {
            let elem = app.axui_elem();
            unsafe {
                let _ = AXUIElementSetAttributeValue(
                    elem.as_concrete_TypeRef().cast(),
                    attr.as_concrete_TypeRef(),
                    val.as_CFTypeRef(),
                );
            }
            added_any = true;
        }
    }

    if added_any {
        conn.update_known_apps_and_windows();
    }
}

fn is_transient_window(client: WinId, conn: &mut WmConn) -> bool {
    use penrosx::AXUIElementAttributes;

    let Ok(win) = conn.window(client) else {
        return true;
    };
    let ax = win.axui_elem();
    let is_ax_window = ax.role().is_ok_and(|role| role == "AXWindow");
    let is_standard_subrole = ax
        .subrole()
        .is_ok_and(|subrole| subrole == "AXStandardWindow");
    let has_close_button = ax.close_button().is_ok();

    !(is_ax_window && is_standard_subrole && has_close_button)
}

fn window_identity(client: WinId, conn: &mut WmConn) -> (String, Option<String>) {
    use penrosx::AXUIElementAttributes;

    if let Ok(win) = conn.window(client) {
        let title = win
            .window_name()
            .map(str::to_owned)
            .or_else(|| win.axui_elem().title().ok().map(|s| s.to_string()))
            .filter(|t| !t.is_empty());
        (win.owner().to_owned(), title)
    } else {
        (String::new(), conn.client_title(client).ok())
    }
}

fn snapshot_current_state(client_set: &StackSet<WinId>, conn: &mut WmConn) -> PersistedState {
    let focused_screen_index = client_set.current_screen().index();
    let screen_tags = client_set
        .screens()
        .map(|screen| (screen.index(), screen.workspace.tag().to_owned()))
        .collect();

    let mut workspaces = BTreeMap::new();
    let mut windows = Vec::new();

    for ws in client_set.workspaces() {
        let tag = ws.tag().to_owned();
        let stack_order: Vec<u32> = ws.clients().map(|id| **id).collect();
        let focused_win_id = ws.focus().map(|id| **id);

        workspaces.insert(
            tag.clone(),
            WorkspaceSnapshot {
                layout_name: ws.layout_name(),
                stack_order,
                focused_win_id,
            },
        );

        for &client in ws.clients() {
            if is_transient_window(client, conn) {
                continue;
            }
            let (owner, title) = window_identity(client, conn);
            let floating = client_set
                .screen_for_client(&client)
                .filter(|_| client_set.is_floating(&client))
                .and_then(|screen| {
                    conn.client_geometry(client)
                        .ok()
                        .map(|r| r.relative_to(&screen.geometry()))
                });

            windows.push(WindowSnapshot {
                win_id: *client,
                owner,
                title,
                tag: tag.clone(),
                floating,
            });
        }
    }

    PersistedState {
        focused_screen_index,
        screen_tags,
        workspaces,
        windows,
    }
}

#[cfg(test)]
mod tests {
    use penrose::core::layout::LayoutStack;

    use super::*;

    fn sample_persisted_state() -> PersistedState {
        PersistedState {
            focused_screen_index: 0,
            screen_tags: BTreeMap::from([(0, "2".to_owned())]),
            workspaces: BTreeMap::from([
                (
                    "1".to_owned(),
                    WorkspaceSnapshot {
                        layout_name: String::new(),
                        stack_order: vec![10],
                        focused_win_id: Some(10),
                    },
                ),
                (
                    "2".to_owned(),
                    WorkspaceSnapshot {
                        layout_name: String::new(),
                        stack_order: vec![20, 21],
                        focused_win_id: Some(21),
                    },
                ),
            ]),
            windows: vec![
                WindowSnapshot {
                    win_id: 10,
                    owner: "Safari".to_owned(),
                    title: Some("GitHub".to_owned()),
                    tag: "1".to_owned(),
                    floating: None,
                },
                WindowSnapshot {
                    win_id: 20,
                    owner: "iTerm2".to_owned(),
                    title: Some("zsh".to_owned()),
                    tag: "2".to_owned(),
                    floating: None,
                },
                WindowSnapshot {
                    win_id: 21,
                    owner: "iTerm2".to_owned(),
                    title: Some("nvim".to_owned()),
                    tag: "2".to_owned(),
                    floating: None,
                },
            ],
        }
    }

    #[test]
    fn matches_exact_win_id_first() {
        let mut restoring = RestoringState {
            persisted: sample_persisted_state(),
            claimed_indices: HashSet::new(),
            new_to_old_win_id: HashMap::new(),
        };

        let matched = claim_matching_window(&mut restoring, 20, "iTerm2", Some("different title"))
            .expect("match by win_id");
        assert_eq!(matched.tag, "2");
        assert_eq!(matched.win_id, 20);
    }

    #[test]
    fn matches_by_owner_and_title_across_reboot_when_win_ids_change() {
        let mut restoring = RestoringState {
            persisted: sample_persisted_state(),
            claimed_indices: HashSet::new(),
            new_to_old_win_id: HashMap::new(),
        };

        let matched = claim_matching_window(&mut restoring, 999, "iTerm2", Some("nvim"))
            .expect("match by owner + title");
        assert_eq!(matched.win_id, 21);
        assert_eq!(matched.tag, "2");

        // Next iTerm2 window with a different title falls back to remaining unclaimed iTerm2 entry
        let second = claim_matching_window(&mut restoring, 1000, "iTerm2", Some("bash"))
            .expect("match by owner fallback");
        assert_eq!(second.win_id, 20);
        assert_eq!(second.tag, "2");

        // Third iTerm2 window has no remaining saved entries
        assert!(claim_matching_window(&mut restoring, 1001, "iTerm2", Some("fish")).is_none());
    }

    #[test]
    fn detects_hide_corner_coordinates() {
        let screens = vec![Rect::new(0, 33, 1512, 859)];
        assert!(is_at_hide_corner(Rect::new(1511, 891, 800, 600), &screens));
        assert!(!is_at_hide_corner(Rect::new(100, 100, 800, 600), &screens));
    }

    #[test]
    fn restores_screen_tag_and_workspace_stack_order() {
        let mut client_set = StackSet::try_new(
            LayoutStack::default(),
            ["1", "2", "9"],
            [Rect::new(0, 0, 1512, 859)],
        )
        .expect("create stack set");

        client_set.insert(WinId(10));
        client_set.insert(WinId(21));
        client_set.insert(WinId(20));
        client_set.move_client_to_tag(&WinId(21), "2");
        client_set.move_client_to_tag(&WinId(20), "2");

        let restoring = RestoringState {
            persisted: sample_persisted_state(),
            claimed_indices: HashSet::new(),
            new_to_old_win_id: HashMap::new(),
        };
        assert!(apply_workspace_and_screen_state(
            &mut client_set,
            &restoring,
            &HashMap::new()
        ));

        assert_eq!(client_set.current_tag(), "2");
        assert_eq!(client_set.current_client(), Some(&WinId(21)));
        let ws2_clients: Vec<u32> = client_set
            .workspace("2")
            .expect("workspace 2")
            .clients()
            .map(|id| **id)
            .collect();
        assert_eq!(ws2_clients, vec![20, 21]);
    }
}
