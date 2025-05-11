#!/usr/bin/env bats
bats_require_minimum_version 1.5.0

ROOT_DIR=$(realpath "$BATS_TEST_DIRNAME/../../..")
TEST_DATA_DIR=$(realpath "$ROOT_DIR/test/data")
# "cargo run" cannot be used since it may mess up the output
# If hardcode path creates problems, use:
# https://github.com/rust-lang/cargo/issues/7895#issuecomment-2323050826
RICHCLIP="$ROOT_DIR/target/debug/richclip"

setup_file() {
    OSTYPE=$(uname -s)
    case $OSTYPE in
        Darwin*) echo "Running tests on MacOS"  >&3;;
        *)  skip; echo "Skip MacOS tests" >&3;;
    esac
    run -0 cargo build
}

@test "MacOS paste simple data" {
    # Simple data
    echo "TestDaTA" | pbcopy 3>&-
    run -0 "$RICHCLIP" paste
    [ "$output" = "TestDaTA" ]
}

@test "MacOS copy simple data" {
    "$RICHCLIP" copy 3>&- < "$TEST_DATA_DIR/test_data_0"

    run -0 pbpaste
    [ "$output" = "GOOD" ]

    run -0 "$RICHCLIP" paste -l
    [ "${lines[0]}" = "public.utf8-plain-text" ]
    [ "${lines[1]}" = "public.html" ]
}
