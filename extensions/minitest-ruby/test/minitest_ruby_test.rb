# frozen_string_literal: true

require "minitest/autorun"
require_relative "../runtime"

class MinitestRubyExtensionTest < Minitest::Test
  def extension
    RubyFastLspExtensionEntrypoint.ruby_fast_lsp_extension
  end

  def document(uri: "file:///repo/test/models/user_test.rb", text:)
    {"uri" => uri, "text" => text}
  end

  def test_discovers_classes_method_tests_and_declarative_tests
    source = <<~RUBY
      class UserTest < Minitest::Test
        def test_valid
        end

        test "rejects blanks" do
        end
      end
    RUBY

    symbols = extension.document_symbols(document(text: source))

    assert_equal(
      ["test_valid", "rejects blanks"],
      symbols.map { |patch| patch.dig("DocumentSymbol", "name") }
    )
    assert_equal ["Method", "Method"], symbols.map { |patch| patch.dig("DocumentSymbol", "kind") }
  end

  def test_emits_run_and_debug_lenses_with_exact_targets
    source = <<~RUBY
      class UserTest < ActiveSupport::TestCase
        def test_valid
        end
      end
    RUBY

    lenses = extension.code_lens(document(text: source)).map { |patch| patch.fetch("CodeLens") }

    assert_equal ["Run Minitest", "Debug Minitest", "Run Minitest", "Debug Minitest"], lenses.map { |lens| lens.fetch("title") }
    method_lenses = lenses.select { |lens| lens.fetch("range").dig("start", "line") == 1 }
    assert_equal [
      ["file:///repo/test/models/user_test.rb", "2", "test_valid"],
      ["file:///repo/test/models/user_test.rb", "2", "test_valid"]
    ], method_lenses.map { |lens| lens.fetch("arguments") }
    assert_equal ["ruby-fast-lsp.minitest.run", "ruby-fast-lsp.minitest.debug"], method_lenses.map { |lens| lens.fetch("command") }
  end

  def test_ignores_test_shaped_code_outside_test_files
    source = <<~RUBY
      class UserTest
        def test_valid
        end
      end
    RUBY

    assert_empty extension.document_symbols(document(uri: "file:///repo/lib/user.rb", text: source))
    assert_empty extension.code_lens(document(uri: "file:///repo/lib/user.rb", text: source))
  end
end
