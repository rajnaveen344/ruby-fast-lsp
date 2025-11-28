# YARD Type Inference Test Fixture
# This file tests YARD documentation parsing and type inference for all parameter types
# Using standard YARD format: @param name [Type] description

class User
  # Creates a new user with required params
  # Standard YARD format: @param name [Type] description
  # @param name [String] the user's name
  # @param age [Integer] the user's age
  # @return [User] the new user instance
  def initialize(name, age)
    @name = name
    @age = age
  end

  # Greets the user with optional param
  # @param greeting [String] optional greeting message
  # @return [String] the greeting message
  def greet(greeting = "Hello")
    "#{greeting}, #{@name}!"
  end

  # Checks if user is an adult (no params, just return type)
  # @return [Boolean] true if user is 18 or older
  def adult?
    @age >= 18
  end

  # Gets user's friends
  # @return [Array<User>] list of friends
  def friends
    []
  end

  # Gets user's settings with standard YARD Hash syntax
  # @return [Hash{Symbol => String}] user settings
  def settings
    {}
  end

  # Validates the user with union types
  # @param context [String, nil] optional validation context
  # @return [Boolean, nil] validation result or nil if skipped
  def validate(context = nil)
    return nil if context.nil?
    true
  end

  # Method with rest parameter (*args) - can accept multiple types
  # Note: YARD uses param name without the * prefix
  # @param prefix [String] the prefix
  # @param items [Array<String, Integer>] variable items (can be strings or integers)
  # @return [Array<String>] prefixed items
  def prefix_items(prefix, *items)
    items.map { |item| "#{prefix}#{item}" }
  end

  # Method with keyword arguments (required and optional)
  # @param name [String] the user's name
  # @param age [Integer] the user's age
  # @param city [String] optional city
  # @return [Hash{Symbol => Object}] user info with mixed value types
  def create_with_keywords(name:, age:, city: "Unknown")
    { name: name, age: age, city: city }
  end

  # Method with keyword rest parameter (**kwargs)
  # Note: YARD uses param name without the ** prefix
  # @param base [String] base value
  # @param options [Hash] additional options
  # @option options [Boolean] :force (false) force execution
  # @option options [String] :prefix prefix for output
  # @return [Hash{Symbol => Object}] merged result
  def with_options(base, **options)
    { base: base }.merge(options)
  end

  # Method with block parameter (&block)
  # @param items [Array<String>] items to process
  # @param block [Proc] the block to call
  # @return [Array] processed items
  def process_with_block(items, &block)
    items.map(&block)
  end

  # Method with all parameter types combined
  # @param required [String] a required param
  # @param optional [Integer] an optional param
  # @param rest [Array<String, Integer>] rest params can be mixed types
  # @param keyword [String] a keyword param
  # @param kwrest [Hash] keyword rest with any values
  # @option kwrest [Boolean] :verbose enable verbose mode
  # @param block [Proc] a block param
  # @return [Hash{Symbol => Object}] all params
  def all_param_types(required, optional = 1, *rest, keyword:, **kwrest, &block)
    {
      required: required,
      optional: optional,
      rest: rest,
      keyword: keyword,
      kwrest: kwrest,
      block: block
    }
  end
end

# Top-level method with YARD docs
# @param message [String] the message to print
# @return [nil]
def print_message(message)
  puts message
end

# Method without YARD docs - should NOT get type hints
def no_docs_method
  "no docs"
end

# Method with mismatched YARD param - should generate diagnostic
# @param wrong_name [String] this param doesn't exist
# @param another_wrong [Integer] also doesn't exist
# @return [String]
def mismatched_params(actual_param)
  actual_param.to_s
end
