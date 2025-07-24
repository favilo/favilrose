use std::collections::HashMap;

use penrose::{
    builtin::{
        actions::{
            exit, floating::sink_focused, key_handler, modify_with, send_layout_message, spawn,
        },
        layout::messages::{ExpandMain, IncMain, ShrinkMain},
    },
    core::{bindings::KeyEventHandler, State},
    map, util,
    x::XConnExt,
    x11rb::RustConn,
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
        "M-S-comma" => send_layout_message(|| IncMain(1)),
        "M-S-period" => send_layout_message(|| IncMain(-1)),
        "M-S-Up" => send_layout_message(|| IncMain(1)),
        "M-S-Down" => send_layout_message(|| IncMain(-1)),
        "M-S-Right" => send_layout_message(|| ExpandMain),
        "M-S-Left" => send_layout_message(|| ShrinkMain),
        "M-S-z" => key_handler(|_, _| {
            util::spawn("i3lock")?;
            util::spawn("systemctl suspend-then-hibernate")?;
            Ok(())
        }),
        "M-r" => spawn("dmenu_run"),
        "M-Return" => spawn("kitty"),

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

        "M-F10" => spawn("flameshot gui"),
    };

    for tag in &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"] {
        let focus_tag: Box<dyn KeyEventHandler<RustConn>> = Box::new(
            move |state: &mut State<RustConn>, x: &RustConn| -> penrose::Result<()> {
                // let current_mouse = state
                //     .extension::<MouseHandler>()?
                //     .borrow()
                //     .current_mouse_position();
                let client_set = &mut state.client_set;
                // let screen = client_set
                //     .screens()
                //     .find(|s| s.geometry().contains_point(current_mouse));
                let screen = None::<&penrose::pure::Screen<RustConn>>;
                match screen {
                    // TODO: Lets add some methods to the penrose crate
                    Some(_screen) => todo!("We need to implement this in the penrose crate"),
                    None => {
                        client_set.pull_tag_to_screen(tag);
                    }
                };

                x.refresh(state)
            },
        );
        raw_bindings.extend([
            (format!("M-{tag}"), focus_tag),
            (
                format!("M-S-{tag}"),
                modify_with(move |client_set| client_set.move_focused_to_tag(tag)),
            ),
        ]);
    }

    raw_bindings
}
