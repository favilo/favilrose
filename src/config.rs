use std::{
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    env,
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    sync::mpsc::{channel, Receiver, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use color_eyre::eyre::{eyre, Context, Report, Result};
use derive_more::{AsRef, Debug};
use notify::{
    event::ModifyKind, recommended_watcher, Event, EventKind, RecommendedWatcher, RecursiveMode,
    Watcher,
};
use penrose::{
    core::{
        bindings::{KeyBindings, KeyCode, KeyEventHandler, ModifierKey},
        hooks::{EventHook, StateHook},
        State,
    },
    util::spawn_for_output_with_args,
    x::{XConn, XEvent},
    x11rb::RustConn,
};
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};
use x11rb::{
    connection::Connection,
    protocol::xproto::{
        AtomEnum, ConnectionExt as XprotoConnectionExt, GrabMode, ModMask, PropMode,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use crate::bindings;
use crate::extensions::stack_set::Direction;

const CONFIG_FILE: &str = "config.toml";
const RELOAD_PROPERTY: &[u8] = b"_FAVILROSE_CONFIG_RELOAD";
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);
const NOTIFICATION_DEBOUNCE: Duration = Duration::from_secs(1);

#[derive(AsRef, Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Keybinding(String);

#[derive(AsRef, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Command(String);

#[derive(Clone, Copy, Debug, Deserialize, EnumIter, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Program {
    Lock,
    Suspend,
    Launcher,
    Terminal,
    Quit,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    BrightnessUp,
    BrightnessDown,
    Screenshot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    FocusDown,
    FocusUp,
    SwapDown,
    SwapUp,
    KillFocused,
    SinkFocused,
    ToggleTag,
    NextScreen,
    PreviousScreen,
    NextLayout,
    PreviousLayout,
    IncreaseMain,
    DecreaseMain,
    ExpandMain,
    ShrinkMain,
    LockAndSuspend,
    Launcher,
    Terminal,
    Restart,
    Quit,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    BrightnessUp,
    BrightnessDown,
    Screenshot,
    FocusTag(u8),
    MoveFocusTo(u8),
    FocusDirection(Direction),
    MoveDirection(Direction),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PenroseConfig {
    #[serde(default)]
    pub keybindings: BTreeMap<Keybinding, Action>,
    #[serde(default)]
    programs: BTreeMap<Program, Command>,
}

impl Default for PenroseConfig {
    fn default() -> Self {
        Self {
            keybindings: BTreeMap::from([
                binding("M-h", Action::FocusDirection(Direction::Left)),
                binding("M-j", Action::FocusDirection(Direction::Down)),
                binding("M-k", Action::FocusDirection(Direction::Up)),
                binding("M-l", Action::FocusDirection(Direction::Right)),
                binding("M-S-h", Action::MoveDirection(Direction::Left)),
                binding("M-S-j", Action::MoveDirection(Direction::Down)),
                binding("M-S-k", Action::MoveDirection(Direction::Up)),
                binding("M-S-l", Action::MoveDirection(Direction::Right)),
                binding("M-S-c", Action::KillFocused),
                binding("M-t", Action::SinkFocused),
                binding("M-Tab", Action::ToggleTag),
                binding("M-bracketright", Action::NextScreen),
                binding("M-bracketleft", Action::PreviousScreen),
                binding("M-grave", Action::NextLayout),
                binding("M-S-grave", Action::PreviousLayout),
                binding("M-S-comma", Action::IncreaseMain),
                binding("M-S-period", Action::DecreaseMain),
                binding("M-S-Up", Action::IncreaseMain),
                binding("M-S-Down", Action::DecreaseMain),
                binding("M-S-Right", Action::ExpandMain),
                binding("M-S-Left", Action::ShrinkMain),
                binding("M-S-z", Action::LockAndSuspend),
                binding("M-r", Action::Launcher),
                binding("M-Return", Action::Terminal),
                binding("M-q", Action::Restart),
                binding("M-S-q", Action::Quit),
                binding("XF86AudioRaiseVolume", Action::VolumeUp),
                binding("XF86AudioLowerVolume", Action::VolumeDown),
                binding("XF86AudioMute", Action::VolumeMute),
                binding("XF86AudioPlay", Action::MediaPlayPause),
                binding("XF86AudioNext", Action::MediaNext),
                binding("XF86AudioPrev", Action::MediaPrevious),
                binding("XF86MonBrightnessUp", Action::BrightnessUp),
                binding("XF86MonBrightnessDown", Action::BrightnessDown),
                binding("M-F10", Action::Screenshot),
                binding("M-1", Action::FocusTag(1)),
                binding("M-S-1", Action::MoveFocusTo(1)),
                binding("M-2", Action::FocusTag(2)),
                binding("M-S-2", Action::MoveFocusTo(2)),
                binding("M-3", Action::FocusTag(3)),
                binding("M-S-3", Action::MoveFocusTo(3)),
                binding("M-4", Action::FocusTag(4)),
                binding("M-S-4", Action::MoveFocusTo(4)),
                binding("M-5", Action::FocusTag(5)),
                binding("M-S-5", Action::MoveFocusTo(5)),
                binding("M-6", Action::FocusTag(6)),
                binding("M-S-6", Action::MoveFocusTo(6)),
                binding("M-7", Action::FocusTag(7)),
                binding("M-S-7", Action::MoveFocusTo(7)),
                binding("M-8", Action::FocusTag(8)),
                binding("M-S-8", Action::MoveFocusTo(8)),
                binding("M-9", Action::FocusTag(9)),
                binding("M-S-9", Action::MoveFocusTo(9)),
                binding("M-0", Action::FocusTag(0)),
                binding("M-S-0", Action::MoveFocusTo(0)),
            ]),
            programs: BTreeMap::from([
                program(Program::Lock, "i3lock"),
                program(Program::Suspend, "systemctl suspend-then-hibernate"),
                program(Program::Launcher, "dmenu_run"),
                program(Program::Terminal, "wezterm"),
                program(Program::Quit, "pkill -fi penrose"),
                program(
                    Program::VolumeUp,
                    "pactl set-sink-volume @DEFAULT_SINK@ +5%",
                ),
                program(
                    Program::VolumeDown,
                    "pactl set-sink-volume @DEFAULT_SINK@ -5%",
                ),
                program(
                    Program::VolumeMute,
                    "pactl set-sink-mute @DEFAULT_SINK@ toggle",
                ),
                program(Program::MediaPlayPause, "playerctl play-pause"),
                program(Program::MediaNext, "playerctl next"),
                program(Program::MediaPrevious, "playerctl previous"),
                program(Program::BrightnessUp, "light -A 5"),
                program(Program::BrightnessDown, "light -U 5"),
                program(Program::Screenshot, "flameshot gui"),
            ]),
        }
    }
}

impl PenroseConfig {
    fn with_overrides(mut self, overrides: Self) -> Self {
        self.keybindings.extend(overrides.keybindings);
        self.programs.extend(overrides.programs);
        self
    }

    /// Return the configured command for `program`.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration omits `program`.
    pub fn command(&self, program: Program) -> Result<Command> {
        self.programs
            .get(&program)
            .cloned()
            .ok_or_else(|| eyre!("program is missing from configuration: {program:?}"))
    }

    fn validate(&self) -> Result<()> {
        if let Some(program) = Program::iter().find(|program| !self.programs.contains_key(program))
        {
            return Err(eyre!("program is missing from configuration: {program:?}"));
        }

        Ok(())
    }
}

fn binding(key: &str, action: Action) -> (Keybinding, Action) {
    (Keybinding(key.to_owned()), action)
}

fn program(program: Program, command: &str) -> (Program, Command) {
    (program, Command(command.to_owned()))
}

/// Return the user's favilrose configuration path.
///
/// # Errors
///
/// Returns an error when `HOME` is not set.
pub fn config_path() -> Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| eyre!("HOME is not set"))?;
    Ok(PathBuf::from(home)
        .join(".config/favilrose")
        .join(CONFIG_FILE))
}

/// Create a commented default configuration when `path` does not exist.
///
/// # Errors
///
/// Returns an error if defaults cannot be serialized or the file cannot be created or written.
pub fn ensure_config_file(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    let parent = path
        .parent()
        .ok_or_else(|| eyre!("config path has no parent"))?;
    fs::create_dir_all(parent).wrap_err("create configuration directory")?;
    let config = commented_default_config()?;

    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            use std::io::Write;

            file.write_all(config.as_bytes())
                .wrap_err("write default configuration")?;
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error).wrap_err("create configuration file"),
    }

    Ok(())
}

fn commented_default_config() -> Result<String> {
    let config = toml::to_string_pretty(&PenroseConfig::default())
        .wrap_err("serialize default configuration")?;
    let mut commented = String::with_capacity(config.len());

    for line in config.lines() {
        commented.push_str("# ");
        commented.push_str(line);
        commented.push('\n');
    }

    Ok(commented)
}

fn parse_config(contents: &str) -> Result<PenroseConfig> {
    let deserializer =
        toml::de::Deserializer::parse(contents).wrap_err("parse configuration file")?;
    let overrides = serde_path_to_error::deserialize(deserializer).map_err(|error| {
        let path = error.path().to_string();
        Report::new(error.into_inner()).wrap_err(format!("parse configuration file at {path}"))
    })?;
    let config = PenroseConfig::default().with_overrides(overrides);
    config.validate()?;

    Ok(config)
}

fn load_key_bindings(path: &Path) -> Result<KeyBindings<RustConn>> {
    let contents = fs::read_to_string(path).wrap_err("read configuration file")?;
    let config = parse_config(&contents)?;
    let configured_bindings = config
        .keybindings
        .keys()
        .map(|binding| binding.as_ref().as_str())
        .collect::<Vec<_>>();
    tracing::debug!("Keybindings: {configured_bindings:#?}");
    let raw_bindings = bindings::key_bindings(&config)?;

    let key_codes = keycodes_from_xmodmap()?;
    parse_keybindings(raw_bindings, &key_codes).wrap_err("parse keybindings with xmodmap")
}

fn keycodes_from_xmodmap() -> Result<HashMap<String, u8>> {
    let output = spawn_for_output_with_args("xmodmap", &["-pke"]).wrap_err("run xmodmap -pke")?;

    parse_xmodmap_keycodes(&output)
}

fn parse_xmodmap_keycodes(output: &str) -> Result<HashMap<String, u8>> {
    let mut key_codes = HashMap::new();

    for (line_number, line) in output.lines().enumerate() {
        let mut fields = line.split_whitespace();
        if fields.next() != Some("keycode") {
            return Err(eyre!(
                "unexpected xmodmap output on line {}: {line}",
                line_number + 1
            ));
        }

        let key_code = fields
            .next()
            .ok_or_else(|| eyre!("missing keycode on xmodmap output line {}", line_number + 1))?
            .parse::<u8>()
            .wrap_err_with(|| {
                format!("parse keycode on xmodmap output line {}", line_number + 1)
            })?;

        if fields.next() != Some("=") {
            return Err(eyre!(
                "missing '=' after keycode on xmodmap output line {}",
                line_number + 1
            ));
        }

        key_codes.extend(fields.map(|key_name| (key_name.to_owned(), key_code)));
    }

    Ok(key_codes)
}

fn parse_keybindings(
    bindings: HashMap<String, Box<dyn KeyEventHandler<RustConn>>>,
    key_codes: &HashMap<String, u8>,
) -> penrose::Result<KeyBindings<RustConn>> {
    bindings
        .into_iter()
        .map(|(binding, handler)| parse_keybinding(&binding, key_codes).map(|key| (key, handler)))
        .collect()
}

fn parse_keybinding(pattern: &str, key_codes: &HashMap<String, u8>) -> penrose::Result<KeyCode> {
    let (modifiers, key_name) = pattern.rsplit_once('-').unwrap_or(("", pattern));
    let code = key_codes
        .get(key_name)
        .ok_or_else(|| penrose::Error::UnknownKeyName {
            name: key_name.to_owned(),
        })?;
    let mask = modifiers
        .split('-')
        .filter(|modifier| !modifier.is_empty())
        .map(ModifierKey::try_from)
        .try_fold(0, |mask, modifier| {
            modifier.map(|modifier| mask | u16::from(modifier))
        })?;

    Ok(KeyCode { mask, code: *code })
}

pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    _debounce_worker: JoinHandle<()>,
    changes: Receiver<()>,
    key_bindings: KeyBindings<RustConn>,
    notifications: NotificationDebouncer,
    path: PathBuf,
}

impl ConfigWatcher {
    /// Watch `path` and load its initial keybindings.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration cannot be loaded or the file watcher cannot start.
    pub fn new(path: PathBuf) -> Result<Self> {
        let key_bindings = load_key_bindings(&path)?;
        let (sender, changes) = channel();
        let (write_sender, write_events) = channel();
        let debounce_worker = thread::spawn(move || debounce_writes(&write_events, &sender));
        let watched_path = path.clone();
        let mut watcher = recommended_watcher(move |event: notify::Result<Event>| match event {
            Ok(event) if is_config_write(&event, &watched_path) => {
                let _ = write_sender.send(());
            }
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "watch configuration file"),
        })
        .wrap_err("create configuration watcher")?;

        let parent = path
            .parent()
            .ok_or_else(|| eyre!("config path has no parent"))?;
        watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .wrap_err("watch configuration directory")?;

        Ok(Self {
            _watcher: watcher,
            _debounce_worker: debounce_worker,
            changes,
            key_bindings,
            notifications: NotificationDebouncer::default(),
            path,
        })
    }

    fn grab_keys(&self, x: &RustConn) -> penrose::Result<()> {
        let keys = self.key_bindings.keys().copied().collect::<Vec<_>>();
        let connection = x.connection();
        let root = *x.root();

        connection.ungrab_key(0, root, ModMask::ANY)?.check()?;

        for modifier in [0, u16::from(ModMask::M2)] {
            for key in &keys {
                connection
                    .grab_key(
                        false,
                        root,
                        (key.mask | modifier).into(),
                        key.code,
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    )?
                    .check()?;
            }
        }

        connection.flush()?;

        Ok(())
    }

    fn reload_if_requested(&mut self, x: &RustConn) {
        if self.changes.try_iter().next().is_none() {
            return;
        }

        match load_key_bindings(&self.path) {
            Ok(key_bindings) => {
                self.key_bindings = key_bindings;
                if let Err(error) = self.grab_keys(x).wrap_err("grab_keys failed") {
                    tracing::error!(?error, "grab reloaded keybindings");
                    self.notify(
                        "Keybindings not reloaded",
                        "Check config.toml and the favilo-penrose logs",
                    );
                } else {
                    self.notify(
                        "Keybindings reloaded",
                        "favilo-penrose is using your updated configuration",
                    );
                }
            }
            Err(error) => {
                tracing::error!(?error, path = %self.path.display(), "configuration reload failed");
                self.notify(
                    "Keybindings not reloaded",
                    "Check config.toml and the favilo-penrose logs",
                );
            }
        }
    }

    fn notify(&mut self, summary: &str, body: &str) {
        if self.notifications.should_send(Instant::now()) {
            send_notification(summary, body);
        }
    }

    fn handle_keypress(
        &mut self,
        key: penrose::core::bindings::KeyCode,
        state: &mut State<RustConn>,
        x: &RustConn,
    ) -> penrose::Result<()> {
        if let Some(handler) = self.key_bindings.get_mut(&key) {
            handler.call(state, x)?;
        }

        Ok(())
    }
}

fn is_config_write(event: &Event, path: &Path) -> bool {
    event.paths.iter().any(|changed| changed == path) && is_write_event(event.kind)
}

fn is_write_event(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(ModifyKind::Data(_))
    )
}

fn debounce_writes(write_events: &Receiver<()>, changes: &Sender<()>) {
    while write_events.recv().is_ok() {
        while write_events.recv_timeout(RELOAD_DEBOUNCE).is_ok() {}

        if changes.send(()).is_ok() {
            if let Err(error) = wake_window_manager() {
                tracing::error!(%error, "wake config reload handler");
            }
        }
    }
}

#[derive(Default)]
struct NotificationDebouncer {
    last_sent_at: Option<Instant>,
}

impl NotificationDebouncer {
    fn should_send(&mut self, now: Instant) -> bool {
        if self
            .last_sent_at
            .is_some_and(|last_sent_at| now.duration_since(last_sent_at) < NOTIFICATION_DEBOUNCE)
        {
            return false;
        }

        self.last_sent_at = Some(now);
        true
    }
}

pub struct ConfigKeybindingHook;

impl EventHook<RustConn> for ConfigKeybindingHook {
    fn call(
        &mut self,
        event: &XEvent,
        state: &mut State<RustConn>,
        x: &RustConn,
    ) -> penrose::Result<bool> {
        let watcher = state.extension::<ConfigWatcher>()?;
        let mut watcher = watcher.borrow_mut();
        watcher.reload_if_requested(x);

        if let XEvent::KeyPress(key) = event {
            watcher.handle_keypress(*key, state, x)?;
        }

        Ok(true)
    }
}

pub struct ConfigKeybindingStartup;

impl StateHook<RustConn> for ConfigKeybindingStartup {
    fn call(&mut self, state: &mut State<RustConn>, x: &RustConn) -> penrose::Result<()> {
        state.extension::<ConfigWatcher>()?.borrow().grab_keys(x)
    }
}

fn wake_window_manager() -> Result<()> {
    let (connection, _) = RustConnection::connect(None).wrap_err("connect to X server")?;
    let root = connection
        .setup()
        .roots
        .first()
        .ok_or_else(|| eyre!("X server did not report any screens"))?
        .root;
    let property = connection
        .intern_atom(false, RELOAD_PROPERTY)
        .wrap_err("intern reload property")?
        .reply()
        .wrap_err("resolve reload property")?
        .atom;

    connection
        .change_property8(PropMode::REPLACE, root, property, AtomEnum::STRING, &[1])
        .wrap_err("signal configuration reload")?;
    connection.flush().wrap_err("flush reload signal")?;

    Ok(())
}

fn send_notification(summary: &str, body: &str) {
    if let Err(error) = ProcessCommand::new("notify-send")
        .args(["--app-name", "favilo-penrose", summary, body])
        .spawn()
    {
        tracing::error!(%error, "send configuration notification");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_configuration_serializes_and_deserializes() {
        let config = PenroseConfig::default();
        let serialized = toml::to_string_pretty(&config).expect("serialize default configuration");
        let parsed = toml::from_str::<PenroseConfig>(&serialized)
            .expect("deserialize default configuration");

        assert!(serialized.contains("M-Return = \"terminal\""));
        assert!(serialized.contains("[keybindings.M-1]\nfocus_tag = 1"));
        assert!(serialized.contains("[keybindings.M-h]\nfocus_direction = \"left\""));
        parsed.validate().expect("validate default configuration");
        assert_eq!(
            parsed.keybindings.get(&Keybinding("M-1".to_owned())),
            Some(&Action::FocusTag(1))
        );
        assert_eq!(
            parsed.keybindings.get(&Keybinding("M-S-l".to_owned())),
            Some(&Action::MoveDirection(Direction::Right))
        );
        assert!(bindings::key_bindings(&parsed).is_ok());
    }

    #[test]
    fn commented_default_configuration_uses_all_defaults() {
        let config = parse_config(&commented_default_config().expect("serialize defaults"))
            .expect("parse commented defaults");

        assert_eq!(
            config.command(Program::Terminal).expect("terminal command"),
            Command("wezterm".to_owned())
        );
        assert_eq!(
            config.keybindings.get(&Keybinding("M-1".to_owned())),
            Some(&Action::FocusTag(1))
        );
    }

    #[test]
    fn explicit_configuration_overrides_defaults() {
        let config = parse_config("[programs]\nterminal = 'alacritty'")
            .expect("parse override configuration");

        assert_eq!(
            config.command(Program::Terminal).expect("terminal command"),
            Command("alacritty".to_owned())
        );
        assert_eq!(
            config.command(Program::Launcher).expect("launcher command"),
            Command("dmenu_run".to_owned())
        );
    }

    #[test]
    fn missing_program_command_returns_an_error() {
        let mut config = PenroseConfig::default();
        config.programs.remove(&Program::Terminal);

        let error = config
            .command(Program::Terminal)
            .expect_err("reject missing program command");

        assert!(error.to_string().contains("Terminal"));
    }

    #[test]
    fn configuration_errors_identify_the_failing_section() {
        let error = parse_config("[programs]\nterminal = 1").expect_err("reject invalid command");
        let rendered = format!("{error:?}");

        assert!(rendered.contains("parse configuration file at programs.?"));
        assert!(rendered.contains("invalid type: integer `1`, expected a string"));
    }

    #[test]
    fn xmodmap_keycodes_parse_bindings_without_waiting_for_a_child() {
        let key_codes = parse_xmodmap_keycodes(
            "keycode  36 = Return NoSymbol Return\nkeycode 172 = XF86AudioRaiseVolume NoSymbol XF86AudioRaiseVolume",
        )
        .expect("parse xmodmap output");

        assert_eq!(key_codes.get("Return"), Some(&36));
        assert_eq!(
            parse_keybinding("M-Return", &key_codes).expect("parse configured binding"),
            KeyCode {
                mask: u16::from(ModifierKey::Meta),
                code: 36,
            }
        );
    }

    #[test]
    fn only_write_events_request_a_reload() {
        use notify::event::{AccessKind, CreateKind, DataChange};

        assert!(!is_write_event(EventKind::Access(AccessKind::Read)));
        assert!(is_write_event(EventKind::Create(CreateKind::File)));
        assert!(is_write_event(EventKind::Modify(ModifyKind::Data(
            DataChange::Content
        ))));
    }

    #[test]
    fn notification_debouncer_suppresses_rapid_notifications() {
        let now = Instant::now();
        let mut debouncer = NotificationDebouncer::default();

        assert!(debouncer.should_send(now));
        assert!(!debouncer.should_send(now + Duration::from_millis(999)));
        assert!(debouncer.should_send(now + NOTIFICATION_DEBOUNCE));
    }
}
