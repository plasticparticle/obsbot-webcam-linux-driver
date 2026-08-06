#!/usr/bin/env bash
set -euo pipefail

if lsmod | grep -q '^v4l2loopback '; then
  if grep -Rqs '^OBSBOT Virtual Camera$' /sys/class/video4linux/*/name 2>/dev/null; then
    echo "OBSBOT Virtual Camera is already available."
    exit 0
  fi
  echo "v4l2loopback is loaded without an OBSBOT Virtual Camera." >&2
  echo "Close applications using loopback devices, then run:" >&2
  echo "  sudo modprobe -r v4l2loopback" >&2
  echo "  bash scripts/setup-virtual-camera.sh" >&2
  exit 1
fi

sudo modprobe v4l2loopback \
  devices=1 \
  video_nr=42 \
  card_label="OBSBOT Virtual Camera" \
  exclusive_caps=1

echo "Created OBSBOT Virtual Camera at /dev/video42"
