#!/usr/bin/env bats

setup() {
    test_dir=$(mktemp -d)
    cd "$test_dir"
}

teardown() {
    rm -rf "$test_dir"
}

@test "No arguments: prints usage and exits 1" {
    run "$BATS_TEST_DIRNAME/../packaging/scripts/sync-artifacts.sh"
    assert_failure
    [[ "$output" == *"Usage"* ]]
}

@test "Missing DEPLOY_SSH_KEY: errors on missing env vars" {
    touch dummy.tar.gz
    DEPLOY_SSH_KEY="" DEPLOY_HOST="h" DEPLOY_USER="u" DEPLOY_PATH="/p" \
        run "$BATS_TEST_DIRNAME/../packaging/scripts/sync-artifacts.sh" dummy.tar.gz
    assert_failure
    [[ "$output" == *"Missing required env vars"* ]]
}

@test "Missing DEPLOY_HOST: errors on missing env vars" {
    touch dummy.tar.gz
    DEPLOY_SSH_KEY="key" DEPLOY_HOST="" DEPLOY_USER="u" DEPLOY_PATH="/p" \
        run "$BATS_TEST_DIRNAME/../packaging/scripts/sync-artifacts.sh" dummy.tar.gz
    assert_failure
    [[ "$output" == *"Missing required env vars"* ]]
}

@test "Non-existent tarball: errors on file not found" {
    DEPLOY_SSH_KEY="key" DEPLOY_HOST="h" DEPLOY_USER="u" DEPLOY_PATH="/p" \
        run "$BATS_TEST_DIRNAME/../packaging/scripts/sync-artifacts.sh" nonexistent.tar.gz
    assert_failure
    [[ "$output" == *"File not found"* ]]
}