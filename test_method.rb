#!/usr/bin/env ruby

# This is a test comment
module TestModule
  # Module-level constant
  VERSION = '1.0.0'

  # Mixin module
  module Helpers
    def helper_method
      puts 'Helper method called'
    end
  end

  # Main class definition
  class TestClass
    include Helpers
    extend Enumerable

    # Class-level instance variable
    @instances = 0

    # Property accessors with symbols
    attr_accessor :name, :age
    attr_reader :created_at

    # Class variable
    @@total_instances = 0

    # Regular expression constant
    EMAIL_REGEX = /^[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+$/

    # Initialize method with default parameter
    def initialize(name, age = 30)
      @name = name        # Instance variable
      @age = age         # Instance variable
      @created_at = Time.now
      @@total_instances += 1
      self.class.increment_instances
    end

    # Class method using self
    def self.increment_instances
      @instances += 1
    end

    # Instance method with string interpolation
    def greet
      # Heredoc usage
      message = <<~HEREDOC
        Hello, #{@name}!
        You are #{@age} years old.
        You were created at #{@created_at}.
      HEREDOC
      puts message
    end

    # Method with block parameter
    def self.create_many(count, &block)
      count.times.map do |i|
        instance = new("Person #{i}", 20 + i)
        block.call(instance) if block
        instance
      end
    end

    # Method with keyword arguments
    def update(name: nil, age: nil)
      @name = name if name
      @age = age if age
    end

    # Method demonstrating various Ruby operators
    def calculate(x, y)
      result = case x <=> y
               when -1 then :less
               when 0 then :equal
               when 1 then :greater
               end

      # Parallel assignment
      a, b = y, x

      # Range operator
      (a..b).each { |n| puts n }

      # Safe navigation operator
      result&.to_s
    end

    # Method with rescue clause
    def risky_operation
      raise 'Error' if @age < 0
    rescue StandardError => e
      puts "Caught error: #{e.message}"
    ensure
      puts 'Always executed'
    end
  end
end

# Create instances and demonstrate usage
if __FILE__ == $PROGRAM_NAME
  # Array of symbols
  valid_names = %i[john jane alice bob]

  # Hash with symbol keys
  config = {
    enabled: true,
    max_retries: 3,
    timeout: 30
  }

  # Block usage with do..end
  TestModule::TestClass.create_many(3) do |person|
    person.greet
  end

  # Block usage with braces
  %w[Alice Bob].each { |name|
    person = TestModule::TestClass.new(name)
    person.update(age: 25)
  }

  # Regular expression matching
  name = 'test@example.com'
  if name =~ TestModule::TestClass::EMAIL_REGEX
    puts 'Valid email'
  end
end