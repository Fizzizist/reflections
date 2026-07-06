class ReflectionsBin < Formula
  desc "A terminal-based work journalling app with Google Calendar sync"
  homepage "https://github.com/Fizzizist/reflections"
  url "__URL__"
  sha256 "__SHA256__"
  version "__VERSION__"
  license "MIT"

  on_macos do
    depends_on "freetype" => :build
  end

  def install
    bin.install "reflect"
  end

  test do
    assert_match "reflections", shell_output("#{bin}/reflect --help")
  end
end