#!/usr/bin/env bats
bats_require_minimum_version 1.5.0

ROOT_DIR=$(realpath "$BATS_TEST_DIRNAME/../..")
TEST_DATA_DIR=$(realpath "$BATS_TEST_DIRNAME/../data")
# "cargo run" cannot be used since it may mess up the output
# If hardcode path creates problems, use:
# https://github.com/rust-lang/cargo/issues/7895#issuecomment-2323050826
RICHCLIP="$ROOT_DIR/target/debug/richclip"

setup_file() {
    if [ -n "$WAYLAND_DISPLAY" ] || [ -z "$DISPLAY" ]; then
        skip
    fi
    run -0 cargo build
}

teardown() {
    killall xclip || echo ""
    killall richclip || echo ""
}

@test "X INCR copy works for xclip" {
    "$RICHCLIP" copy --chunk-size=1 3>&- < "$TEST_DATA_DIR/test_data_0"

    run -0 xclip -o -selection clipboard -target TARGETS
    [ "${lines[0]}" = "TARGETS" ]
    [ "${lines[1]}" = "text/plain" ]
    [ "${lines[2]}" = "text" ]
    [ "${lines[3]}" = "text/html" ]
    run -0 xclip -o -selection clipboard
    [ "$output" = "GOOD" ]
    run -0 xclip -o -selection clipboard -target "text/html"
    [ "$output" = "BAD" ]

    # Test primary selection
    "$RICHCLIP" copy -p --chunk-size=1 3>&- < "$TEST_DATA_DIR/test_data_0"
    run -0 xclip -o -selection primary
    [ "$output" = "GOOD" ]
    run -0 xclip -o -selection primary -target "text/html"
    [ "$output" = "BAD" ]
}

@test "X INCR copy works for richclip" {
    "$RICHCLIP" copy --chunk-size=1 3>&- < "$TEST_DATA_DIR/test_data_0"

    run -0 "$RICHCLIP" paste -l
    [ "${lines[0]}" = "TARGETS" ]
    [ "${lines[1]}" = "text/plain" ]
    [ "${lines[2]}" = "text" ]
    [ "${lines[3]}" = "text/html" ]
    run -0 "$RICHCLIP" paste
    [ "$output" = "GOOD" ]
    run -0 "$RICHCLIP" paste -t "text/html"
    [ "$output" = "BAD" ]

    # Test primary selection
    "$RICHCLIP" copy -p --chunk-size=1 3>&- < "$TEST_DATA_DIR/test_data_0"
    run -0 "$RICHCLIP" paste
    [ "$output" = "GOOD" ]
    run -0 "$RICHCLIP" paste -p -t "text/html"
    [ "$output" = "BAD" ]
}
