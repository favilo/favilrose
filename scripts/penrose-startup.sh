#! /usr/bin/env bash
# ----------------------------------------
# Bootstrap the start of a penrose session
# >> This get's run on restart as well!
# ----------------------------------------

# Make sure we only run once
pid=$$
pgrep -fi penrose-startup.sh | grep -v "^$pid$" | xargs -I{} kill {}

xinput set-prop "11" "libinput Tapping Enabled" 1
xinput set-prop "11" "libinput Natural Scrolling Enabled" 1
xinput map-to-output "ELAN900C:00" eDP-1

xsetroot -cursor_name left_ptr -solid black

~/.config/polybar/launch.sh

# xset s 300 600
# xss-lock -- i3lock -n

pkill -fi xautolock; xautolock -secure -time 15 -locker "systemctl suspend" -notify 300 \
    -notifier "i3lock -n | xset dpms force off" &
    # -notifier "notify-send -t 5000 -i dialog-password -u low 'Security Advisory' '\nLocking Session in 15 seconds'" \
    # `-detectsleep` does the opposite of what I want. It avoids locking the screen when sleep was detected.
    # -detectsleep &

pkill -fi snixembed; snixembed &

# kitty -o allow_remote_control=yes --listen-on unix:/tmp/kitty_remote_control &
pkill -fi nm-applet; nm-applet &
# pkill -fi udiskie; udiskie -a -n -t &
pkill -fi volumeicon; volumeicon &
pkill -fi xfce4-notifyd
# pkill -fi dunst; dunst &
# pkill -fi wired; wired &
systemctl --user restart wired.service
pkill -fi blueman-applet; blueman-applet &
pkill -fi flameshot; flameshot &
# pkill -fi xfce4-power-man; xfce4-power-manager &  # for some reason, this ends up running as xcfe4-power-man
pkill -fi gnome-keyring-daemon; gnome-keyring-daemon --start --components=pkcs11,secrets,ssh &
pkill -fi talon-bin; talon &
