#!/usr/bin/env bats

# Tests with own copy & paste. Ideally this would run on all platforms

bats_require_minimum_version 1.5.0

ROOT_DIR=$(realpath "$BATS_TEST_DIRNAME/../../..")
RICHCLIP="$ROOT_DIR/target/debug/richclip"

XVFB_PID=""

setup_file() {
    unset WAYLAND_DISPLAY
    export DISPLAY=":42"
    # Start a headless X server for testing
    Xvfb $DISPLAY 3>&- &
    XVFB_PID=$!
    sleep 1
    run -0 cargo build
}

teardown_file() {
    if [ -n "$XVFB_PID" ]; then
        kill "$XVFB_PID"
    fi
}

teardown() {
    killall -w richclip > /dev/null || echo ""
}

@test "one-shot mode:  no '--type'" {
    # one-shot no type
    echo "TestDaTA" | $RICHCLIP copy --one-shot

    run -0 "$RICHCLIP" paste -l
    [ "${lines[0]}" = "TARGETS" ]
    [ "${lines[1]}" = "text/plain" ]
    [ "${lines[2]}" = "text/plain;charset=utf-8" ]
    [ "${lines[3]}" = "TEXT" ]
    [ "${lines[4]}" = "STRING" ]
    [ "${lines[5]}" = "UTF8_STRING" ]

    run -0 "$RICHCLIP" paste
    [ "$output" = "TestDaTA" ]
}

@test "one-shot mode:  with '--type'" {
    # one-shot, one type
    echo "TestDaTA" | $RICHCLIP copy --type TypE

    run -0 "$RICHCLIP" paste -l
    [ "${lines[0]}" = "TARGETS" ]
    [ "${lines[1]}" = "TypE" ]

    run -0 "$RICHCLIP" paste -t TypE
    [ "$output" = "TestDaTA" ]

    # one-shot, multi types
    echo "TestDaTA" | $RICHCLIP copy --type TypE --type Faker
    run -0 "$RICHCLIP" paste -l
    [ "${lines[0]}" = "TARGETS" ]
    [ "${lines[1]}" = "TypE" ]
    [ "${lines[2]}" = "Faker" ]

    run -0 "$RICHCLIP" paste -t Faker
    [ "$output" = "TestDaTA" ]
}
