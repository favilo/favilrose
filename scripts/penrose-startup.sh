#! /usr/bin/env bash
# ----------------------------------------
# Bootstrap the start of a penrose session
# >> This get's run on restart as well!
# ----------------------------------------

# Make sure we only run once
pid=$$
pgrep -fi penrose-startup.sh | grep -v "^$pid$" | xargs -I{} kill {}

xinput --set-prop "10" "libinput Tapping Enabled" 1
xinput --set-prop "10" "libinput Natural Scrolling Enabled" 1

xsetroot -cursor_name left_ptr

~/.config/polybar/launch.sh

pkill -fi snixembed; snixembed &

pkill -fi nm-applet; nm-applet &
# pkill -fi udiskie; udiskie -a -n -t &
pkill -fi volumeicon; volumeicon &
pkill -fi dunst; dunst &
pkill -fi blueman-applet; blueman-applet &
pkill -fi xfce4-power-man; xfce4-power-manager &  # for some reason, this ends up running as xcfe4-power-man
pkill -fi xfce4-screensaver; xfce4-screensaver &
pkill -fi gnome-keyring-daemon; gnome-keyring-daemon --start --components=pkcs11,secrets,ssh &
pkill -fi talon-bin; talon &
