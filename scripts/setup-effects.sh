#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
venv_dir="$repo_dir/.effects-venv"
model_dir="$repo_dir/effects/models"

python3 -m venv "$venv_dir"
"$venv_dir/bin/python" -m pip install --upgrade pip
"$venv_dir/bin/python" -m pip install -r "$repo_dir/effects/requirements.txt"

mkdir -p "$model_dir"
curl --fail --location --retry 3 \
  --output "$model_dir/face_landmarker.task" \
  "https://storage.googleapis.com/mediapipe-models/face_landmarker/face_landmarker/float16/1/face_landmarker.task"
curl --fail --location --retry 3 \
  --output "$model_dir/selfie_segmenter.tflite" \
  "https://storage.googleapis.com/mediapipe-models/image_segmenter/selfie_segmenter/float16/1/selfie_segmenter.tflite"

echo "Effects runtime installed in $venv_dir"
