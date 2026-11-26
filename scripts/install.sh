#!/usr/bin/env bash
# Install OBSBOT Control Center for the current user, so it no longer needs
# this source checkout to run.
#
#   scripts/install.sh [--prefix DIR] [--skip-runtime] [--skip-virtual-camera]
#   scripts/install.sh --uninstall [--prefix DIR]
#
# --prefix               where bin/ and share/ go (default: ~/.local)
# --skip-runtime         do not create the Python environment or download
#                        models (for packagers, or when it is already set up)
# --skip-virtual-camera  do not set up the virtual camera other apps can pick
#                        (the only step that asks for sudo)
#
# Uninstalling also removes the virtual camera again.
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prefix="$HOME/.local"
skip_runtime=0
skip_virtual_camera=0
uninstall=0
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) prefix="$2"; shift 2 ;;
    --skip-runtime) skip_runtime=1; shift ;;
    --skip-virtual-camera) skip_virtual_camera=1; shift ;;
    --uninstall) uninstall=1; shift ;;
    -h|--help) sed -n '2,15p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
runtime_dir="$data_home/obsbot-control-center"
share_dir="$prefix/share/obsbot-control-center"
desktop_file="$data_home/applications/obsbot-control-center.desktop"
icon_file="$data_home/icons/hicolor/scalable/apps/obsbot-control-center.svg"

if [ "$uninstall" = 1 ]; then
  "$prefix/bin/obsbotctl" app quit >/dev/null 2>&1 || true
  bash "$repo_dir/scripts/setup-virtual-camera.sh" --remove ||
    echo "Could not remove the virtual camera; run scripts/setup-virtual-camera.sh --remove later." >&2
  rm -f "$prefix/bin/obsbot-control-center" "$prefix/bin/obsbotctl" "$desktop_file" "$icon_file"
  rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/obsbot-control-center.desktop"
  rm -rf "$share_dir" "$runtime_dir/venv" "$runtime_dir/models"
  echo "Removed. Your settings in ${XDG_CONFIG_HOME:-$HOME/.config}/obsbot-control-center were kept."
  exit 0
fi

echo "Building…"
(cd "$repo_dir" && cargo build --release)

echo "Installing programs to $prefix/bin"
install -Dm755 "$repo_dir/target/release/obsbot-control-center" "$prefix/bin/obsbot-control-center"
install -Dm755 "$repo_dir/target/release/obsbotctl" "$prefix/bin/obsbotctl"

echo "Installing effects to $share_dir"
rm -rf "$share_dir/effects"
install -d "$share_dir/effects/luts" "$share_dir/effects/backgrounds"
install -m644 "$repo_dir"/effects/*.py "$repo_dir/effects/requirements.txt" "$share_dir/effects/"
install -m644 "$repo_dir"/effects/luts/*.cube "$share_dir/effects/luts/"
install -m644 "$repo_dir"/effects/backgrounds/*.jpg "$share_dir/effects/backgrounds/"

if [ "$skip_runtime" = 0 ]; then
  echo "Setting up the effects runtime in $runtime_dir (this downloads about 300 MB)"
  python3 -m venv "$runtime_dir/venv"
  "$runtime_dir/venv/bin/python" -m pip install --quiet --upgrade pip
  "$runtime_dir/venv/bin/python" -m pip install --quiet -r "$repo_dir/effects/requirements.txt"
  mkdir -p "$runtime_dir/models"
  base="https://storage.googleapis.com/mediapipe-models"
  curl --fail --location --retry 3 --silent --show-error \
    --output "$runtime_dir/models/face_landmarker.task" \
    "$base/face_landmarker/face_landmarker/float16/1/face_landmarker.task"
  curl --fail --location --retry 3 --silent --show-error \
    --output "$runtime_dir/models/selfie_segmenter.tflite" \
    "$base/image_segmenter/selfie_segmenter/float16/1/selfie_segmenter.tflite"
  curl --fail --location --retry 3 --silent --show-error \
    --output "$runtime_dir/models/hand_landmarker.task" \
    "$base/hand_landmarker/hand_landmarker/float16/1/hand_landmarker.task"
fi

install -d "$(dirname "$desktop_file")" "$(dirname "$icon_file")"
cat > "$icon_file" <<'SVG'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <rect x="4" y="4" width="56" height="56" rx="14" fill="#7469ff"/>
  <circle cx="32" cy="32" r="16" fill="#ffffff"/>
  <circle cx="32" cy="32" r="8" fill="#7469ff"/>
  <circle cx="28" cy="28" r="2.5" fill="#ffffff"/>
</svg>
SVG
cat > "$desktop_file" <<DESKTOP
[Desktop Entry]
Type=Application
Name=OBSBOT Control Center
Comment=Camera control, effects and virtual camera
Exec="$prefix/bin/obsbot-control-center"
Icon=obsbot-control-center
Terminal=false
Categories=AudioVideo;Video;Utility;
DESKTOP

if [ "$skip_virtual_camera" = 0 ]; then
  echo "Setting up the virtual camera (asks for your password)"
  if ! bash "$repo_dir/scripts/setup-virtual-camera.sh" --persist; then
    echo
    echo "The virtual camera is not set up, so other apps cannot pick the effects" >&2
    echo "camera yet. Everything else works. To finish it later, run:" >&2
    echo "  bash scripts/setup-virtual-camera.sh --persist" >&2
  fi
fi

echo
echo "Done. Start it from your app launcher or run: $prefix/bin/obsbot-control-center"
case ":$PATH:" in
  *":$prefix/bin:"*) ;;
  *) echo "Note: $prefix/bin is not on your PATH, so type the full path for obsbotctl." ;;
esac
"$prefix/bin/obsbot-control-center" --paths || true
