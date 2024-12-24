#!/usr/bin/env bats
bats_require_minimum_version 1.5.0

ROOT_DIR=$(realpath "$BATS_TEST_DIRNAME/../..")
# "cargo run" cannot be used since it may mess up the output
# If hardcode path creates problems, use https://github.com/rust-lang/cargo/issues/7895#issuecomment-2323050826
RICHCLIP="$ROOT_DIR/target/debug/richclip"

setup_file() {
    run -0 cargo build
}

teardown_file() {
    killall wl-copy
}

@test "wayland paste" {
  wl-copy "TestDaTA" 3>&-

  run -0 "$RICHCLIP" paste
  [ "$output" = "TestDaTA" ]
}
