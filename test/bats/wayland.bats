#!/usr/bin/env bats
bats_require_minimum_version 1.5.0

ROOT_DIR=$(realpath "$BATS_TEST_DIRNAME/../..")
TEST_DATA_DIR=$(realpath "$BATS_TEST_DIRNAME/../data")
# "cargo run" cannot be used since it may mess up the output
# If hardcode path creates problems, use:
# https://github.com/rust-lang/cargo/issues/7895#issuecomment-2323050826
RICHCLIP="$ROOT_DIR/target/debug/richclip"

setup_file() {
    if [ -z "$WAYLAND_DISPLAY" ]; then
        skip
    fi
    run -0 cargo build
}

teardown() {
    killall wl-copy > /dev/null || echo ""
    killall richclip > /dev/null || echo ""
}

@test "wayland paste simple data" {
    # Simple data
    wl-copy "TestDaTA" 3>&-
    run -0 "$RICHCLIP" paste
    [ "$output" = "TestDaTA" ]
}

@test "wayland paste with mime-type" {
    # Specific mime-type
    wl-copy -t "spec/type" "special_mime_type" 3>&-
    run -0 "$RICHCLIP" paste -t "spec/type"
    [ "$output" = "special_mime_type" ]

    # Expected mime-type does not exist
    run -0 --separate-stderr "$RICHCLIP" paste -t "not_this_type"
    [ "$output" = "" ]
}

@test "wayland paste with empty clipbaord" {
    # Empty clipbaord
    wl-copy -c
    run -0 --separate-stderr "$RICHCLIP" paste
    [ "$output" = "" ]
}

@test "wayland paste simple data from primary" {
    # Simple data
    wl-copy "NotThis" 3>&-
    wl-copy -p "TestDaTA" 3>&-
    run -0 "$RICHCLIP" paste -p
    [ "$output" = "TestDaTA" ]
}

@test "wayland paste list mime-types only" {
    # wl-copy doesn't support multiple types
    wl-copy -t "some-type" "TestDaTA" 3>&-
    run -0 "$RICHCLIP" paste -l
    [ "$output" = "some-type" ]

    # Test primary
    wl-copy -p -t "other-type" "TestDaTA" 3>&-
    run -0 "$RICHCLIP" paste -l -p
    [ "$output" = "other-type" ]
}

@test "wayland copy" {
    "$RICHCLIP" copy 3>&- < "$TEST_DATA_DIR/test_data_0"

    run -0 wl-paste -l
    [ "${lines[0]}" = "text/plain" ]
    [ "${lines[1]}" = "TEXT" ]
    [ "${lines[2]}" = "text/html" ]
    run -0 wl-paste
    [ "$output" = "GOOD" ]
    run -0 wl-paste -t "text/html"
    [ "$output" = "BAD" ]

    # Test primary selection
    "$RICHCLIP" copy -p 3>&- < "$TEST_DATA_DIR/test_data_0"
    run -0 wl-paste -p
    [ "$output" = "GOOD" ]
    run -0 wl-paste -p -t "text/html"
    [ "$output" = "BAD" ]
}
