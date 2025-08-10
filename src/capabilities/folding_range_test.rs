use super::*;
use tower_lsp::lsp_types::Url;

#[test]
fn test_comprehensive_folding_ranges() {
    let content = r#"
class TestClass
  def initialize
    @value = 42
  end
end

if true
  puts "hello"
else
  puts "world"
end

while x < 10
  puts x
  x += 1
end

case value
when 1
  puts "one"
when 2
  puts "two"
end

begin
  risky_operation
rescue
  handle_error
end

array = [
  1,
  2,
  3
]

hash = {
  key: "value",
  another: "data"
}

[1, 2, 3].each do |item|
  puts item
end

for i in 1..5
  puts i
end
"#;

    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, content.to_string(), 1);

    let parse_result = ruby_prism::parse(content.as_bytes());
    let node = parse_result.node();

    let mut visitor = FoldingRangeVisitor::new(&document);
    visitor.visit(&node);

    let ranges = visitor.folding_ranges();
    
    println!("Found {} folding ranges:", ranges.len());
    for (i, range) in ranges.iter().enumerate() {
        println!("  {}: lines {}-{}", i, range.start_line, range.end_line);
    }
    
    // We should have folding ranges for:
    // 1. class
    // 2. def (method)
    // 3. if statement
    // 4. while loop
    // 5. case statement
    // 6. begin block
    // 7. array (multi-line)
    // 8. hash (multi-line)
    // 9. block
    // 10. for loop
    assert!(ranges.len() >= 10, "Expected at least 10 folding ranges, got {}", ranges.len());
}

#[test]
fn test_single_line_constructs_no_folding() {
    let content = r#"
puts "hello"
x = [1, 2, 3]
y = { a: 1, b: 2 }
if true then puts "inline" end
"#;

    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, content.to_string(), 1);

    let parse_result = ruby_prism::parse(content.as_bytes());
    let node = parse_result.node();

    let mut visitor = FoldingRangeVisitor::new(&document);
    visitor.visit(&node);

    let ranges = visitor.folding_ranges();
    
    // Single-line constructs should not create folding ranges
    assert_eq!(ranges.len(), 0, "Single-line constructs should not create folding ranges");
}