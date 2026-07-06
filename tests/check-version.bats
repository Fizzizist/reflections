#!/usr/bin/env bats

# Tests for packaging/scripts/check-version.sh
# These tests exercise the version-delta detection logic in isolation.

setup() {
    TEST_DIR=$(mktemp -d)
    export SCRIPT_DIR="$BATS_TEST_DIRNAME/../packaging/scripts"
}

teardown() {
    rm -rf "$TEST_DIR"
}

# Helper: create a git repo in $TEST_DIR and seed it with at least one commit
# so git tag operations work.
init_git_repo() {
    cd "$TEST_DIR"
    git init --quiet
    git config user.email "test@test.com"
    git config user.name "Test"
    echo "initial" > initial.txt
    git add initial.txt
    git commit --quiet -m "initial"
}

# Helper: write a Cargo.toml with the given version string.
write_cargo_toml() {
    local version="$1"
    cat > "$TEST_DIR/Cargo.toml" <<EOF
[package]
name = "reflections"
version = "${version}"
edition = "2024"
EOF
}

@test "happy path: cargo version differs from latest tag -> should_release=true" {
    init_git_repo
    write_cargo_toml "2.0.0"
    git tag "v1.0.0"

    run bash -c "cd '$TEST_DIR' && bash '$SCRIPT_DIR/check-version.sh'"
    [ "$status" -eq 0 ]
    echo "$output" | grep "^should_release=true$"
    echo "$output" | grep "^version=2.0.0$"
}

@test "skip path: cargo version matches latest tag -> should_release=false" {
    init_git_repo
    write_cargo_toml "1.0.0"
    git tag "v1.0.0"

    run bash -c "cd '$TEST_DIR' && bash '$SCRIPT_DIR/check-version.sh'"
    [ "$status" -eq 0 ]
    echo "$output" | grep "^should_release=false$"
    echo "$output" | grep "^version=1.0.0$"
}

@test "first release: no tags exist -> should_release=true" {
    init_git_repo
    write_cargo_toml "1.0.0"

    run bash -c "cd '$TEST_DIR' && bash '$SCRIPT_DIR/check-version.sh'"
    [ "$status" -eq 0 ]
    echo "$output" | grep "^should_release=true$"
    echo "$output" | grep "^version=1.0.0$"
}

@test "malformed Cargo.toml: missing version field -> error exit" {
    init_git_repo
    cat > "$TEST_DIR/Cargo.toml" <<EOF
[package]
name = "reflections"
edition = "2024"
EOF

    run bash -c "cd '$TEST_DIR' && bash '$SCRIPT_DIR/check-version.sh'"
    [ "$status" -ne 0 ]
}

@test "missing Cargo.toml -> error exit" {
    init_git_repo

    run bash -c "cd '$TEST_DIR' && bash '$SCRIPT_DIR/check-version.sh' '$TEST_DIR/nonexistent.toml'"
    [ "$status" -ne 0 ]
}

@test "multiple version lines: uses first match" {
    init_git_repo
    cat > "$TEST_DIR/Cargo.toml" <<EOF
[package]
name = "reflections"
version = "3.1.0"
edition = "2024"

[dependencies]
serde = "1.0"
EOF
    git tag "v3.0.0"

    run bash -c "cd '$TEST_DIR' && bash '$SCRIPT_DIR/check-version.sh'"
    [ "$status" -eq 0 ]
    echo "$output" | grep "^should_release=true$"
    echo "$output" | grep "^version=3.1.0$"
}