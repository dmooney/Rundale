#!/usr/bin/env bash
# serve_local.sh — launch an OpenAI-compatible local inference server for a
# rundale-bench candidate, then point a `bench-it` target spec at its URL.
#
# Two backends:
#   vllm-mlx      Apple-Silicon MLX server (mlx-community/* repos). Fast, but
#                 cannot suppress "thinking" for some new architectures
#                 (e.g. gemma-4 `gemma4_unified` ignores every enable_thinking
#                 knob → the model burns its token budget reasoning and returns
#                 empty replies on realistic prompts).
#   llama-server  llama.cpp's OpenAI server (GGUF). Required when vllm-mlx can't
#                 serve a model cleanly. Uses llama.cpp's current
#                 `--reasoning on|off` switch, so mandatory-reasoner models
#                 (gemma-4) answer directly when `--thinking false`.
#                 `--model` accepts either a Hugging Face `repo:quant` selector
#                 or a local GGUF path, allowing an existing HF cache entry to
#                 be reused without downloading the weights a second time.
#
# Runs the server in the FOREGROUND; the caller backgrounds it (nohup … &) and
# polls `/v1/models`. The bench spec is then:  <alias-or-repo>@http://127.0.0.1:<port>/v1
#
# Examples
#   # gemma-4-12b via llama.cpp, thinking OFF (the reason this script exists):
#   serve_local.sh --backend llama-server \
#     --model unsloth/gemma-4-12B-it-GGUF:UD-Q4_K_XL \
#     --alias gemma-4-12b-it --port 8002
#   #   → target spec:  gemma-4-12b-it@http://127.0.0.1:8002/v1
#
#   # an MLX candidate via vllm-mlx:
#   serve_local.sh --backend vllm-mlx \
#     --model mlx-community/Qwen2.5-7B-Instruct-4bit --port 8000
set -euo pipefail

BACKEND=""
MODEL=""
PORT=8002
HOST=127.0.0.1
ALIAS=""
THINK=false # llama-server only: default thinking OFF for clean bench replies
CTX=8192    # llama-server context window (bench prompts are small)
EXTRA=()    # passthrough flags after `--`

usage() {
    sed -n '2,40p' "$0"
    exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --backend)
            BACKEND="$2"
            shift 2
            ;;
        --model)
            MODEL="$2"
            shift 2
            ;;
        --port)
            PORT="$2"
            shift 2
            ;;
        --host)
            HOST="$2"
            shift 2
            ;;
        --alias)
            ALIAS="$2"
            shift 2
            ;;
        --ctx)
            CTX="$2"
            shift 2
            ;;
        --think)
            THINK=true
            shift
            ;;
        --no-think)
            THINK=false
            shift
            ;;
        --)
            shift
            EXTRA=("$@")
            break
            ;;
        -h | --help) usage 0 ;;
        *)
            echo "unknown arg: $1" >&2
            usage 1
            ;;
    esac
done

[[ -n "$BACKEND" ]] || {
    echo "error: --backend {vllm-mlx|llama-server} required" >&2
    exit 2
}
[[ -n "$MODEL" ]] || {
    echo "error: --model <repo[:quant]> required" >&2
    exit 2
}

case "$BACKEND" in
    vllm-mlx)
        command -v vllm-mlx >/dev/null || {
            echo "error: vllm-mlx not found (uv tool install vllm-mlx)" >&2
            exit 3
        }
        echo ">>> vllm-mlx serve $MODEL on $HOST:$PORT" >&2
        exec vllm-mlx serve "$MODEL" --host "$HOST" --port "$PORT" ${EXTRA[@]+"${EXTRA[@]}"}
        ;;
    llama-server)
        command -v llama-server >/dev/null || {
            echo "error: llama-server not found. Install: brew install llama.cpp" >&2
            exit 3
        }
        alias_name="${ALIAS:-$MODEL}"
        if [[ -f "$MODEL" ]]; then
            model_args=(-m "$MODEL")
            model_source="local"
        else
            model_args=(-hf "$MODEL")
            model_source="huggingface"
        fi
        if [[ "$THINK" == "true" ]]; then
            reasoning_mode="on"
        else
            reasoning_mode="off"
        fi
        echo ">>> llama-server ${model_args[*]} on $HOST:$PORT (source=$model_source, alias=$alias_name, enable_thinking=$THINK)" >&2
        # --jinja applies the GGUF's own chat template. Sampler defaults per the
        # Gemma 4 model card; the bench overrides temperature per request, so
        # these are only fallbacks.
        # --no-mmproj: the bench is text-only. Multimodal GGUFs (gemma-4) ship a
        # vision/audio projector that -hf auto-downloads; loading it can fail
        # (encoder-free archs) and is pure overhead here, so skip it.
        exec llama-server \
            "${model_args[@]}" \
            --alias "$alias_name" \
            --host "$HOST" --port "$PORT" \
            --ctx-size "$CTX" \
            --jinja \
            --no-mmproj \
            --reasoning "$reasoning_mode" \
            --temp 1.0 --top-p 0.95 --top-k 64 \
            ${EXTRA[@]+"${EXTRA[@]}"}
        ;;
    *)
        echo "error: unknown backend '$BACKEND' (want vllm-mlx | llama-server)" >&2
        exit 2
        ;;
esac
