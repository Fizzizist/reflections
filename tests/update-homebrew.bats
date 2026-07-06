#!/usr/bin/env bats

setup() {
    test_dir=$(mktemp -d)
    cd "$test_dir"
}

teardown() {
    rm -rf "$test_dir"
}

@test "No arguments: prints usage and exits 1" {
    run "$BATS_TEST_DIRNAME/../packaging/scripts/update-homebrew.sh"
    assert_failure
    [[ "$output" == *"Usage"* ]]
}

@test "Only version: errors on missing sha256" {
    run "$BATS_TEST_DIRNAME/../packaging/scripts/update-homebrew.sh" "1.6.0"
    assert_failure
    [[ "$output" == *"Usage"* ]]
}

@test "Sed substitution: correctly injects version and sha256 into formula" {
    template_dir="$test_dir/packaging/homebrew"
    mkdir -p "$template_dir"
    cat > "$template_dir/reflections-bin.rb" << 'EOF'
class ReflectionsBin < Formula
  desc "A terminal-based work journaling app with calendar sync"
  homepage "https://github.com/Fizzizist/reflections"
  version "PLACEHOLDER_VERSION"
  url "https://peter.vlasveld.info/releases/reflections/reflections-v#{version}-aarch64-apple-darwin.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "MIT"

  def install
    bin.install "reflect"
  end

  test do
    assert_match "Usage", shell_output("#{bin}/reflect --help")
  end
end
EOF

    target_file="$test_dir/result.rb"
    cp "$template_dir/reflections-bin.rb" "$target_file"

    sed -i.bak "s/^  version .*/  version \"1.6.0\"/" "$target_file"
    sed -i.bak "s/^  sha256 .*/  sha256 \"abc123def456\"/" "$target_file"
    rm -f "${target_file}.bak"

    grep -q 'version "1.6.0"' "$target_file"
    grep -q 'sha256 "abc123def456"' "$target_file"
}

@test "Idempotent commit: no changes exits cleanly" {
    clone_dir="$test_dir/clone"
    mkdir -p "$clone_dir"
    cd "$clone_dir"
    git init -q
    git config user.email "test@test.com"
    git config user.name "Test"

    mkdir -p Formula
    cat > Formula/reflections-bin.rb << 'EOF'
class ReflectionsBin < Formula
  version "1.6.0"
end
EOF
    git add -A
    git commit -q -m "initial"

    # Re-add the same content — should detect no changes
    git add Formula/reflections-bin.rb
    if [[ -n $(git status --porcelain) ]]; then
        git commit -m "should not happen"
        assert_failure
    else
        echo "Formula already up to date — nothing to commit"
    fi
    assert_success
}