pub mod bar;
pub mod hooks;

pub const STARTUP_SCRIPT: &str = "/usr/local/scripts/penrose-startup.sh";

const FONT: &str = "ProFontIIx Nerd Font";
const BLACK: u32 = 0x282828ff;
const WHITE: u32 = 0xebdbb2ff;
const GREY: u32 = 0x3c3836ff;
const BLUE: u32 = 0x458588ff;

pub const BAR_HEIGHT_PX: u32 = 28;
const POINT_SIZE: u8 = 10;

const MAX_ACTIVE_WINDOW_CHARS: usize = 50;

pub fn raw_key_bindings() -> HashMap<String, Box<dyn KeyEventHandler<RustConn>>> {
    let mut raw_bindings = map! {
        map_keys: |k: &str| k.to_string();

        "M-j" => modify_with(|cs| cs.focus_down()),
        "M-k" => modify_with(|cs| cs.focus_up()),
        "M-S-j" => modify_with(|cs| cs.swap_down()),
        "M-S-k" => modify_with(|cs| cs.swap_up()),
        "M-S-c" => modify_with(|cs| cs.kill_focused()),

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
    };

    for tag in &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"] {
        raw_bindings.extend([
            (
                format!("M-{tag}"),
                modify_with(move |client_set| client_set.focus_tag(tag)),
            ),
            (
                format!("M-S-{tag}"),
                modify_with(move |client_set| client_set.move_focused_to_tag(tag)),
            ),
        ]);
    }

    (raw_bindings)
}