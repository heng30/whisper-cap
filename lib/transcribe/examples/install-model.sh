#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODELS_DIR="${SCRIPT_DIR}/models"

TINY_MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin?download=true"
BASE_MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin?download=true"

TINY_MODEL_OUT="${MODELS_DIR}/ggml-tiny.bin"
BASE_MODEL_OUT="${MODELS_DIR}/ggml-base.bin"

mkdir -p "${MODELS_DIR}"

download() {
  local url="$1"
  local out="$2"
  if command -v curl >/dev/null 2>&1; then
    echo "[INFO] Downloading ${url} -> ${out}"
    curl -L --fail --retry 3 --connect-timeout 10 -o "$out" "$url"
  elif command -v wget >/dev/null 2>&1; then
    echo "[INFO] Downloading ${url} -> ${out}"
    wget -O "$out" "$url"
  else
    echo "[ERROR] need curl or wget to download files" >&2
    exit 1
  fi
}

# if [ ! -f "${TINY_MODEL_OUT}" ]; then
#   download "${TINY_MODEL_URL}" "${TINY_MODEL_OUT}"
# else
#   echo "[INFO] Model already exists: ${TINY_MODEL_OUT}"
# fi

if [ ! -f "${BASE_MODEL_OUT}" ]; then
  download "${BASE_MODEL_URL}" "${BASE_MODEL_OUT}"
else
  echo "[INFO] Model already exists: ${BASE_MODEL_OUT}"
fi
