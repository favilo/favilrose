#!/usr/bin/env bash
eval $(/usr/bin/gnome-keyring-daemon --start --components=pkcs11,secrets,ssh)
export SSH_AUTH_SOCK
eval $(ssh-agent)

while true; do
  favilo-penrose
  export RESTARTED=true
done
