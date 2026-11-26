#!/usr/bin/env bash
# Create the "OBSBOT Virtual Camera" that other apps (browsers, Teams, Zoom…)
# can pick as a webcam. Needs the v4l2loopback kernel module and sudo.
#
#   scripts/setup-virtual-camera.sh            create it until the next reboot
#   scripts/setup-virtual-camera.sh --persist  create it now and at every boot
#   scripts/setup-virtual-camera.sh --remove   remove it and the boot setup
set -euo pipefail

label="OBSBOT Virtual Camera"
video_nr=42
load_conf=/etc/modules-load.d/obsbot-virtual-camera.conf
options_conf=/etc/modprobe.d/obsbot-virtual-camera.conf

mode=once
case "${1:-}" in
  "") ;;
  --persist) mode=persist ;;
  --remove) mode=remove ;;
  -h|--help) sed -n '2,7p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "unknown option: $1" >&2; exit 2 ;;
esac

loaded() { lsmod | grep -q '^v4l2loopback '; }
present() { grep -qsx "$label" /sys/class/video4linux/*/name 2>/dev/null; }

if [ "$mode" = remove ]; then
  if [ -e "$load_conf" ] || [ -e "$options_conf" ]; then
    sudo rm -f "$load_conf" "$options_conf"
    echo "Removed the boot setup for $label."
  fi
  # Only unload the module when the device it provides is ours.
  if loaded && present; then
    if sudo modprobe -r v4l2loopback; then
      echo "Removed $label."
    else
      echo "$label is still in use; it disappears after the next reboot." >&2
    fi
  fi
  exit 0
fi

if ! modinfo v4l2loopback >/dev/null 2>&1; then
  echo "The v4l2loopback kernel module is not installed." >&2
  echo "Install it for your distribution, then run this script again:" >&2
  echo "  Arch:          sudo pacman -S v4l2loopback-dkms" >&2
  echo "                 (plus the headers package for your kernel, e.g. linux-headers," >&2
  echo "                  if it is not installed yet — check with: uname -r)" >&2
  echo "  Debian/Ubuntu: sudo apt install v4l2loopback-dkms" >&2
  echo "  Fedora:        sudo dnf install v4l2loopback (RPM Fusion)" >&2
  exit 1
fi

if [ "$mode" = persist ]; then
  echo "v4l2loopback" | sudo tee "$load_conf" >/dev/null
  echo "options v4l2loopback devices=1 video_nr=$video_nr card_label=\"$label\" exclusive_caps=1" |
    sudo tee "$options_conf" >/dev/null
  echo "$label will be created at every boot."
fi

if loaded; then
  if present; then
    echo "$label is already available."
    exit 0
  fi
  echo "v4l2loopback is loaded without an $label." >&2
  echo "Close applications using loopback devices, then run:" >&2
  echo "  sudo modprobe -r v4l2loopback" >&2
  echo "  bash scripts/setup-virtual-camera.sh" >&2
  exit 1
fi

sudo modprobe v4l2loopback \
  devices=1 \
  video_nr="$video_nr" \
  card_label="$label" \
  exclusive_caps=1

echo "Created $label at /dev/video$video_nr"
