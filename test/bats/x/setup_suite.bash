#!/usr/bin/env bats
bats_require_minimum_version 1.5.0

XVFB_PID=""

setup_suite() {
    if [ -n "$WAYLAND_DISPLAY" ] || [ -z "$DISPLAY" ]; then
        unset WAYLAND_DISPLAY
        export DISPLAY=":42"
        # Start a headless X server for testing
        Xvfb $DISPLAY 3>&- &
        XVFB_PID=$!
        sleep 1
    fi
    run -0 cargo build
}

teardown_suite() {
    if [ -n "$XVFB_PID" ]; then
        kill "$XVFB_PID"
    fi
}
