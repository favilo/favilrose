use std::collections::HashMap;

use penrose::{
    builtin::{
        actions::{
            exit,
            floating::{float_focused, sink_focused},
            modify_with, send_layout_message, spawn,
        },
        layout::messages::{ExpandMain, IncMain, ShrinkMain},
    },
    core::{
        bindings::{
            KeyEventHandler, ModifierKey, MouseBindings, MouseButton, MouseEvent,
            MouseEventHandler, MouseEventKind, MouseState,
        },
        ClientSet, State,
    },
    map,
    pure::Stack,
    util::spawn_with_args,
    x::{XConn, XConnExt},
    x11rb::RustConn,
    Xid,
};

pub fn raw_key_bindings() -> HashMap<String, Box<dyn KeyEventHandler<RustConn>>> {
    let mut raw_bindings = map! {
        map_keys: |k: &str| k.to_string();

        "M-j" => modify_with(|cs| cs.focus_down()),
        "M-k" => modify_with(|cs| cs.focus_up()),
        "M-S-j" => modify_with(|cs| cs.swap_down()),
        "M-S-k" => modify_with(|cs| cs.swap_up()),
        "M-S-c" => modify_with(|cs| cs.kill_focused()),
        "M-t" => sink_focused(),

        "M-Tab" => modify_with(|cs| cs.toggle_tag()),
        "M-bracketright" => modify_with(|cs| cs.next_screen()),
        "M-bracketleft" => modify_with(|cs| cs.previous_screen()),
        "M-grave" => modify_with(|cs| cs.next_layout()),
        "M-S-grave" => modify_with(|cs| cs.previous_layout()),
        "M-S-Up" => send_layout_message(|| IncMain(1)),
        "M-S-Down" => send_layout_message(|| IncMain(-1)),
        "M-S-Right" => send_layout_message(|| ExpandMain),
        "M-S-Left" => send_layout_message(|| ShrinkMain),
        "M-S-z" => spawn("i3lock"),
        "M-r" => spawn("dmenu_run"),
        "M-Return" => spawn("alacritty"),

        // Restart the WM because we want to run inside a wrapper script
        "M-q" => exit(),
        // Kill the WM
        "M-S-q" => spawn("pkill -fi penrose"),

        // Volume control
        "XF86AudioRaiseVolume" => spawn("pactl set-sink-volume @DEFAULT_SINK@ +5%"),
        "XF86AudioLowerVolume" => spawn("pactl set-sink-volume @DEFAULT_SINK@ -5%"),
        "XF86AudioMute" => spawn("pactl set-sink-mute @DEFAULT_SINK@ toggle"),

        // Media control
        "XF86AudioPlay" => spawn("playerctl play-pause"),
        "XF86AudioNext" => spawn("playerctl next"),
        "XF86AudioPrev" => spawn("playerctl previous"),

        // Brightness control
        "XF86MonBrightnessUp" => spawn("light -A 5"),
        "XF86MonBrightnessDown" => spawn( "light -U 5"),
    };

    for tag in &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"] {
        raw_bindings.extend([
            (
                format!("M-{tag}"),
                modify_with(move |client_set| client_set.pull_tag_to_screen(tag)),
            ),
            (
                format!("M-S-{tag}"),
                modify_with(move |client_set| client_set.move_focused_to_tag(tag)),
            ),
        ]);
    }

    raw_bindings
}

pub fn mouse_bindings<X>() -> MouseBindings<X>
where
    X: XConn,
{
    let mut map: MouseBindings<X> = HashMap::new();
    map.insert(
        (
            MouseEventKind::Press,
            MouseState {
                button: MouseButton::Left,
                modifiers: vec![ModifierKey::Meta],
            },
        ),
        Box::new(
            move |_e: &MouseEvent, s: &mut State<X>, x: &X| -> penrose::Result<()> {
                let cs = &mut s.client_set;
                let client = cs.current_stack();
                let Some(client) = client else {
                    return Ok(());
                };
                let client = client.focused().clone();
                let rect = x.client_geometry(client)?;
                cs.float(client, rect)?;
                x.refresh(s)
            },
        ),
    );
    map
}

fn mouse_modify_with<F, X>(f: F) -> Box<dyn MouseEventHandler<X>>
where
    F: Fn(&mut ClientSet) + Clone + 'static,
    X: XConn,
{
    Box::new(move |_: &MouseEvent, s: &mut State<X>, x: &X| x.modify_and_refresh(s, f.clone()))
}
