<image width="100px" src="https://raw.githubusercontent.com/sminez/penrose/develop/icon.svg" align="left"></image>
favilrose - a tiling window manager
===================================

My personal Tiling Window Manager, built with [Penrose](https://github.com/sminez/penrose)

Trying to get as close to my old deprecated XMonad build as I can.

## Keybindings

On startup, favilrose creates `~/.config/favilrose/config.toml` with every
default commented out. Uncomment only the settings you want to override; all
other bindings and commands continue to come from `PenroseConfig::default()`.
Key names use Penrose's `xmodmap` syntax and map to actions; parameterized
actions keep their tag explicit:

```toml
[keybindings]
M-Return = "terminal"

[keybindings.M-1]
focus_tag = 1

[keybindings.M-S-1]
move_focus_to = 1

[keybindings.M-h]
focus_direction = "left"

[keybindings.M-S-l]
move_direction = "right"

[programs]
terminal = "kitty"
launcher = "dmenu_run"
```

Saving the file reloads bindings and program commands immediately and shows a
desktop notification. Invalid configuration leaves the previous working setup
active.

Directional bindings (`M-h/j/k/l`) consider only windows on the current screen.
They rank candidates by perpendicular overlap, gap, and midpoint alignment.
Their shifted variants swap the focused window with the selected window while
preserving focus.

Screenshot from the main penrose repo:
![screenshot](https://raw.githubusercontent.com/sminez/penrose/develop/screenshot.png)
