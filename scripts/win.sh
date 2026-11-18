#!/usr/bin/env bash
# Build bridge: source lives in WSL, GPUI must compile on a Windows host.
set -euo pipefail

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WIN=/mnt/c/Users/radit/dev/nebucat
CARGO=/mnt/c/Users/radit/.cargo/bin/cargo.exe

profile_of() { [[ "${1:-}" == "--release" ]] && echo release || echo debug; }

# Copies only changed files and gives them a fresh mtime: cargo compares mtimes,
# so a preserved older mtime would hide an edit from the build.
sync() {
    mkdir -p "$WIN"
    local stage
    stage="$(mktemp -d)"
    tar -C "$SRC" --exclude=./target --exclude=./.git -cf - . | tar -C "$stage" -xf -
    (cd "$WIN" && find . -path ./target -prune -o -path ./vendor -prune -o -type f -print) |
        while IFS= read -r f; do
            [[ -e "$stage/$f" ]] || rm -f "$WIN/$f"
        done
    (cd "$stage" && find . -type f -print) | while IFS= read -r f; do
        if ! cmp -s "$stage/$f" "$WIN/$f"; then
            mkdir -p "$WIN/$(dirname "$f")"
            cp "$stage/$f" "$WIN/$f"
        fi
    done
    rm -rf "$stage"
}

ensure_everything_dll() {
    local out="$WIN/target/$1"
    if [[ ! -f "$WIN/vendor/Everything64.dll" ]]; then
        mkdir -p "$WIN/vendor"
        curl -sSfL -o "$WIN/vendor/sdk.zip" https://www.voidtools.com/Everything-SDK.zip
        unzip -j -o -q "$WIN/vendor/sdk.zip" dll/Everything64.dll -d "$WIN/vendor"
    fi
    cp "$WIN/vendor/Everything64.dll" "$out/"
}

build() {
    sync
    local flag=() profile
    profile="$(profile_of "${1:-}")"
    [[ "$profile" == release ]] && flag=(--release)
    (cd "$WIN" && "$CARGO" build -p nebucat "${flag[@]}")
    # Windows resolves the full dependency graph; keep its lockfile in the repo.
    cp "$WIN/Cargo.lock" "$SRC/Cargo.lock"
    ensure_everything_dll "$profile"
}

kill_app() { taskkill.exe /IM nebucat.exe /F >/dev/null 2>&1 || true; }

run() {
    # The running exe is locked by Windows, so it must stop before the linker writes it.
    kill_app
    build "${1:-}"
    local profile
    profile="$(profile_of "${1:-}")"
    # explorer.exe starts it with no inherited handles, so this shell never waits on it.
    explorer.exe "C:\\Users\\radit\\dev\\nebucat\\target\\$profile\\nebucat.exe" || true
}

cmd="${1:-}"
shift || true
case "$cmd" in
    sync) sync ;;
    build) build "${1:-}" ;;
    run) run "${1:-}" ;;
    kill) kill_app ;;
    test) sync; (cd "$WIN" && "$CARGO" test --workspace) ;;
    log) tail -n 200 /mnt/c/Users/radit/AppData/Local/nebucat/logs/nebucat.log ;;
    *) echo "usage: scripts/win.sh sync|build|run|kill|test|log [--release]" >&2; exit 2 ;;
esac
