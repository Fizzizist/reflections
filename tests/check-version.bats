#!/usr/bin/env bats

setup() {
    test_dir=$(mktemp -d)
    cd "$test_dir"
}

teardown() {
    rm -rf "$test_dir"
}

@test "Happy path: Cargo.toml version differs from latest tag" {
    mkdir -p "$test_dir/repo"
    cd "$test_dir/repo"
    git init -q
    echo 'version = "1.4.0"' > Cargo.toml
    git config user.email "test@test.com"
    git config user.name "Test"
    git add Cargo.toml
    git commit -q -m "initial"
    git tag "1.3.0"

    run "$BATS_TEST_DIRNAME/../packaging/scripts/check-version.sh" "Cargo.toml" "."
    
    assert_success
    [[ "$output" == *"should_release=true"* ]]
    [[ "$output" == *"version=1.4.0"* ]]
}

@test "Skip path: Cargo.toml version matches latest tag" {
    mkdir -p "$test_dir/repo"
    cd "$test_dir/repo"
    git init -q
    echo 'version = "1.3.0"' > Cargo.toml
    git config user.email "test@test.com"
    git config user.name "Test"
    git add Cargo.toml
    git commit -q -m "initial"
    git tag "1.3.0"

    run "$BATS_TEST_DIRNAME/../packaging/scripts/check-version.sh" "Cargo.toml" "."
    
    assert_success
    [[ "$output" == *"should_release=false"* ]]
    [[ "$output" == *"version=1.3.0"* ]]
}

@test "First release: No tags exist" {
    mkdir -p "$test_dir/repo"
    cd "$test_dir/repo"
    git init -q
    echo 'version = "1.0.0"' > Cargo.toml
    git config user.email "test@test.com"
    git config user.name "Test"
    git add Cargo.toml
    git commit -q -m "initial"

    run "$BATS_TEST_DIRNAME/../packaging/scripts/check-version.sh" "Cargo.toml" "."
    
    assert_success
    [[ "$output" == *"should_release=true"* ]]
    [[ "$output" == *"version=1.0.0"* ]]
}

@test "Malformed Cargo.toml: Graceful error" {
    mkdir -p "$test_dir/repo"
    cd "$test_dir/repo"
    git init -q
    echo 'invalid toml content' > Cargo.toml

    run "$BATS_TEST_DIRNAME/../packaging/scripts/check-version.sh" "Cargo.toml" "."
    
    assert_failure
}

@test "Cargo.toml with v-prefixed tag" {
    mkdir -p "$test_dir/repo"
    cd "$test_dir/repo"
    git init -q
    echo 'version = "1.4.0"' > Cargo.toml
    git config user.email "test@test.com"
    git config user.name "Test"
    git add Cargo.toml
    git commit -q -m "initial"
    git tag "v1.3.0"

    run "$BATS_TEST_DIRNAME/../packaging/scripts/check-version.sh" "Cargo.toml" "."
    
    assert_success
    [[ "$output" == *"should_release=true"* ]]
    [[ "$output" == *"version=1.4.0"* ]]
}

@test "Cargo.toml matches v-prefixed tag" {
    mkdir -p "$test_dir/repo"
    cd "$test_dir/repo"
    git init -q
    echo 'version = "1.3.0"' > Cargo.toml
    git config user.email "test@test.com"
    git config user.name "Test"
    git add Cargo.toml
    git commit -q -m "initial"
    git tag "v1.3.0"

    run "$BATS_TEST_DIRNAME/../packaging/scripts/check-version.sh" "Cargo.toml" "."
    
    assert_success
    [[ "$output" == *"should_release=false"* ]]
    [[ "$output" == *"version=1.3.0"* ]]
}
