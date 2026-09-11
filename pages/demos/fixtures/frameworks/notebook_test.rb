require "minitest/autorun"

class NotebookTest < Minitest::Test
  def test_title
    notebook = { title: "Field guide" }
    assert_equal "Field guide", notebook[:title]
  end
end
