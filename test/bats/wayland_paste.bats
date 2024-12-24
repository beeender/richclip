#!/usr/bin/env bats
bats_require_minimum_version 1.5.0

ROOT_DIR=$(realpath "$BATS_TEST_DIRNAME/../..")
# "cargo run" cannot be used since it may mess up the output
# If hardcode path creates problems, use https://github.com/rust-lang/cargo/issues/7895#issuecomment-2323050826
RICHCLIP="$ROOT_DIR/target/debug/richclip"

setup_file() {
    run -0 cargo build
}

@test "wayland paste simple data" {
    # Simple data
    wl-copy "TestDaTA" 3>&-
    sleep 1
    run -0 "$RICHCLIP" paste
    [ "$output" = "TestDaTA" ]
}

@test "wayland paste with mime-type" {
    # Specific mime-type
    wl-copy -t "spec/type" "special_mime_type" 3>&-
    sleep 1
    run -0 "$RICHCLIP" paste -t "spec/type"
    [ "$output" = "special_mime_type" ]
}

@test "wayland paste with empty clipbaord" {
    # Empty clipbaord
    wl-copy -c
    sleep 1
    run -0 --separate-stderr  "$RICHCLIP" paste
    [ "$output" = "" ]
}

@test "wayland paste simple data from primary" {
    # Simple data
    wl-copy "NotThis" 3>&-
    wl-copy -p "TestDaTA" 3>&-
    sleep 1
    run -0 "$RICHCLIP" paste -p
    [ "$output" = "TestDaTA" ]
}
