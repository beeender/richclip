#!/usr/bin/env bats
bats_require_minimum_version 1.5.0

ROOT_DIR=$(realpath "$BATS_TEST_DIRNAME/../../..")
TEST_DATA_DIR=$(realpath "$ROOT_DIR/test/data")
# "cargo run" cannot be used since it may mess up the output
# If hardcode path creates problems, use:
# https://github.com/rust-lang/cargo/issues/7895#issuecomment-2323050826
RICHCLIP="$ROOT_DIR/target/debug/richclip"

teardown() {
    killall -w xclip || echo ""
    killall -w richclip || echo ""
}

@test "X paste simple data" {
    # Simple data
    echo "TestDaTA" | xclip -i -selection clipboard 3>&-
    run -0 "$RICHCLIP" paste
    [ "$output" = "TestDaTA" ]
}

@test "X paste with mime-type" {
    # Specific mime-type
    echo "special_mime_type" | xclip -i -selection clipboard -target "spec/type" 3>&-
    run -0 "$RICHCLIP" paste -t "spec/type"
    [ "$output" = "special_mime_type" ]

    # Expected mime-type does not exist
    run -0 "$RICHCLIP" paste -t "not_this_type"
    [ "$output" = "" ]
}

@test "X paste with empty clipboard" {
    # NOTE: This test fails with gnome, it seems the clipboard is not empty after xclip getting
    # killed
    # Empty clipboard
    echo "TestDaTA" | xclip -i -selection clipboard 3>&-
    killall xclip
    run -0 --separate-stderr "$RICHCLIP" paste
    [ "$output" = "" ]
}

@test "X paste simple data from primary" {
    # Simple data
    echo "NotThis" | xclip -i -selection clipboard 3>&-
    echo "TestDaTA" | xclip -i -selection primary 3>&-
    run -0 "$RICHCLIP" paste -p
    [ "$output" = "TestDaTA" ]
}

@test "X paste list mime-types only" {
    # xclip doesn't support multiple types
    echo "TestDaTA" | xclip -i -selection clipboard -target "some-type" 3>&-
    run -0 "$RICHCLIP" paste -l
    [ "${lines[0]}" = "TARGETS" ]
    [ "${lines[1]}" = "some-type" ]

    # Test primary
    echo "TestDaTA" | xclip -i -selection primary -target "other-type" 3>&-
    run -0 "$RICHCLIP" paste -l -p
    [ "${lines[0]}" = "TARGETS" ]
    [ "${lines[1]}" = "other-type" ]
}

@test "X copy" {
    "$RICHCLIP" copy 3>&- < "$TEST_DATA_DIR/test_data_0"

    run -0 xclip -o -selection clipboard -target TARGETS
    [ "${lines[0]}" = "TARGETS" ]
    [ "${lines[1]}" = "text/plain" ]
    [ "${lines[2]}" = "TEXT" ]
    [ "${lines[3]}" = "text/html" ]
    run -0 xclip -o -selection clipboard
    [ "$output" = "GOOD" ]
    run -0 xclip -o -selection clipboard -target "text/html"
    [ "$output" = "BAD" ]

    # Test primary selection
    "$RICHCLIP" copy -p 3>&- < "$TEST_DATA_DIR/test_data_0"
    run -0 xclip -o -selection primary
    [ "$output" = "GOOD" ]
    run -0 xclip -o -selection primary -target "text/html"
    [ "$output" = "BAD" ]
}
