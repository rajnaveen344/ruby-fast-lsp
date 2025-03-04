use ruby_fast_lsp::parser::document::RubyDocument;
use tempfile::NamedTempFile;
use std::io::Write;
use std::fs;
use std::path::Path;

// Helper to create a temporary Ruby file with content
pub fn create_temp_ruby_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file
}

// Helper to create a RubyDocument from content
pub fn create_document(content: &str, version: i32) -> RubyDocument {
    RubyDocument::new(content.to_string(), version)
}

// Sample Ruby code snippets for tests
pub fn get_sample_class() -> &'static str {
    r#"
class Person
  attr_accessor :name, :age
  
  def initialize(name, age)
    @name = name
    @age = age
  end
  
  def greeting
    "Hello, my name is #{@name} and I am #{@age} years old."
  end
  
  def self.create_from_hash(hash)
    new(hash[:name], hash[:age])
  end
end
    "#
}

pub fn get_sample_module() -> &'static str {
    r#"
module Utilities
  def self.format_string(str)
    str.strip.downcase
  end
  
  def self.to_camel_case(str)
    str.split('_').map(&:capitalize).join
  end
  
  class Helper
    def initialize
      @cache = {}
    end
    
    def memoize(key, &block)
      @cache[key] ||= block.call
    end
  end
end
    "#
}

pub fn get_sample_script() -> &'static str {
    r#"
# This is a sample Ruby script
require 'json'
require 'date'

# Define a constant
MAX_ATTEMPTS = 3

# Process data
def process_data(data)
  return nil if data.nil?
  
  result = {}
  data.each do |key, value|
    result[key.to_sym] = value.to_s.upcase
  end
  
  result
end

# Main execution
data = JSON.parse('{"name": "Ruby", "type": "language"}')
processed = process_data(data)
puts "Processed: #{processed}"
puts "Current date: #{Date.today}"
    "#
}

// Helper to clean up temporary files
pub fn cleanup_temp_file(file: NamedTempFile) {
    let path = file.path().to_owned();
    drop(file); // Close the file
    if Path::new(&path).exists() {
        fs::remove_file(path).unwrap_or_else(|e| {
            eprintln!("Failed to remove temporary file: {}", e);
        });
    }
}
