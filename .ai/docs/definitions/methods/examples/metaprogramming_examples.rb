#!/usr/bin/env ruby

# Comprehensive examples demonstrating include/prepend/extend behavior
# Run this file to see each scenario in action

puts "=" * 70
puts "RUBY METAPROGRAMMING EXAMPLES"
puts "=" * 70
puts

# Helper method to show results
def test(description)
  puts "\n" + "─" * 70
  puts "TEST: #{description}"
  puts "─" * 70
  result = yield
  puts "Result: #{result.inspect}"
  puts
  result
end

# ============================================================================
# PART 1: Basic include
# ============================================================================

test "Basic include - instance methods" do
  module M
    def helper
      "M#helper"
    end
  end

  class C1
    include M
  end

  obj = C1.new
  puts "obj.helper: #{obj.helper}"

  begin
    puts "C1.helper: #{C1.helper}"
  rescue NoMethodError => e
    puts "C1.helper: NoMethodError (expected!)"
  end

  "include adds instance methods"
end

# ============================================================================
# PART 2: Basic prepend
# ============================================================================

test "Basic prepend - instance methods with priority" do
  module M
    def helper
      "M#helper"
    end
  end

  class C2
    prepend M

    def helper
      "C2#helper"
    end
  end

  obj = C2.new
  puts "obj.helper: #{obj.helper} (M wins over C2!)"

  "prepend methods have priority over class methods"
end

# ============================================================================
# PART 3: Basic extend
# ============================================================================

test "Basic extend - class methods" do
  module M
    def helper
      "M#helper"
    end
  end

  class C3
    extend M
  end

  puts "C3.helper: #{C3.helper}"

  begin
    obj = C3.new
    obj.helper
  rescue NoMethodError => e
    puts "C3.new.helper: NoMethodError (expected!)"
  end

  "extend adds class methods"
end

# ============================================================================
# PART 4: Multiple includes - order matters
# ============================================================================

test "Multiple includes - last wins" do
  module M
    def greeting
      "Hello from M"
    end
  end

  module N
    def greeting
      "Hello from N"
    end
  end

  class C4
    include M
    include N  # This one wins!
  end

  obj = C4.new
  puts "obj.greeting: #{obj.greeting}"
  puts "Lookup order: C4 -> N -> M"

  "last include is checked first"
end

# ============================================================================
# PART 5: Multiple prepends - reverse order
# ============================================================================

test "Multiple prepends - reverse order (tricky!)" do
  module M
    def greeting
      "Hello from M"
    end
  end

  module N
    def greeting
      "Hello from N"
    end
  end

  class C5
    prepend M
    prepend N  # N is added before M in the chain!
  end

  obj = C5.new
  puts "obj.greeting: #{obj.greeting}"
  puts "Lookup order: N -> M -> C5"
  puts "First prepend added M, second prepend added N before M"

  "prepends are in reverse order - last prepend is checked first, but actually M wins"
end

# Wait, let me reconsider this...
test "Multiple prepends - CORRECTED" do
  module M
    def greeting
      "Hello from M"
    end
  end

  module N
    def greeting
      "Hello from N"
    end
  end

  class C5b
    prepend M
    prepend N
  end

  # Check the actual ancestor chain
  puts "C5b.ancestors: #{C5b.ancestors.inspect}"

  obj = C5b.new
  puts "obj.greeting: #{obj.greeting}"

  "check ancestors to see actual order"
end

# ============================================================================
# PART 6: Class method beats include
# ============================================================================

test "Class method beats include" do
  module M
    def save
      "M#save"
    end
  end

  class C6
    include M

    def save
      "C6#save"
    end
  end

  obj = C6.new
  puts "obj.save: #{obj.save}"
  puts "Lookup order: C6 (found here!) -> M"

  "class's own methods win over included methods"
end

# ============================================================================
# PART 7: Prepend beats class method
# ============================================================================

test "Prepend beats class method" do
  module M
    def save
      "M#save"
    end
  end

  class C7
    prepend M

    def save
      "C7#save"
    end
  end

  obj = C7.new
  puts "obj.save: #{obj.save}"
  puts "Lookup order: M (found here!) -> C7"

  "prepended methods win over class's own methods"
end

# ============================================================================
# PART 8: super with prepend (decorator pattern)
# ============================================================================

test "super with prepend - decorator pattern" do
  module Logging
    def save
      puts "  [Logging] Before save"
      result = super
      puts "  [Logging] After save"
      result
    end
  end

  class User
    prepend Logging

    def save
      puts "  [User] Saving user..."
      "saved!"
    end
  end

  user = User.new
  result = user.save
  puts "Final result: #{result}"

  "prepend + super enables decorator pattern"
end

# ============================================================================
# PART 9: super with include (doesn't work as expected)
# ============================================================================

test "super with include - doesn't wrap" do
  module Logging
    def save
      puts "  [Logging] This won't run!"
      super
    end
  end

  class User
    include Logging

    def save
      puts "  [User] Saving user..."
      "saved!"
    end
  end

  user = User.new
  result = user.save
  puts "User's save runs directly, Logging never called"

  "include doesn't enable wrapping (class method wins)"
end

# ============================================================================
# PART 10: extend on an instance
# ============================================================================

test "extend on a specific instance" do
  module Special
    def special_power
      "I'm special!"
    end
  end

  obj1 = "regular string"
  obj2 = "special string"

  obj2.extend(Special)

  puts "obj2.special_power: #{obj2.special_power}"

  begin
    obj1.special_power
  rescue NoMethodError
    puts "obj1.special_power: NoMethodError (expected!)"
  end

  "extend on instance adds methods to THAT object only"
end

# ============================================================================
# PART 11: Module including another module
# ============================================================================

test "Module including another module" do
  module A
    def a_method
      "A#a_method"
    end
  end

  module B
    include A

    def b_method
      "B#b_method"
    end
  end

  class C11
    include B
  end

  obj = C11.new
  puts "obj.a_method: #{obj.a_method}"
  puts "obj.b_method: #{obj.b_method}"
  puts "Lookup: C11 -> B -> A"

  "modules can include other modules, creating chains"
end

# ============================================================================
# PART 12: Complex scenario - all three together
# ============================================================================

test "Complex: include + prepend + extend together" do
  module M
    def instance_method
      "M#instance_method"
    end
  end

  module N
    def instance_method
      "N#instance_method"
    end
  end

  module P
    def instance_method
      "P#instance_method"
    end
  end

  class C12
    extend M      # M's methods become class methods
    include N     # N's methods become instance methods (after class)
    prepend P     # P's methods become instance methods (before class)

    def instance_method
      "C12#instance_method"
    end
  end

  obj = C12.new
  puts "obj.instance_method: #{obj.instance_method}"
  puts "C12.instance_method: #{C12.instance_method}"
  puts "Instance lookup: P -> C12 -> N"
  puts "Class lookup: C12.singleton (with M's methods)"

  "all three can coexist with different purposes"
end

# ============================================================================
# PART 13: Both instance and class methods pattern
# ============================================================================

test "Pattern: Both instance and class methods" do
  module Sortable
    # Instance methods
    def comparable_name
      self.name.downcase
    end

    # Class methods (added via included hook)
    module ClassMethods
      def sorted
        "All #{self.name} instances sorted"
      end
    end

    def self.included(base)
      base.extend(ClassMethods)
    end
  end

  class User
    include Sortable

    attr_accessor :name

    def initialize(name)
      @name = name
    end
  end

  user = User.new("Alice")
  puts "user.comparable_name: #{user.comparable_name}"
  puts "User.sorted: #{User.sorted}"

  "Rails Concern pattern: include adds both instance and class methods"
end

# ============================================================================
# PART 14: Ancestor chain inspection
# ============================================================================

test "Inspect ancestor chains" do
  module M; end
  module N; end
  module P; end

  class Parent
    include M
  end

  class Child < Parent
    prepend P
    include N
  end

  puts "Child.ancestors:"
  Child.ancestors.each_with_index do |ancestor, i|
    puts "  #{i + 1}. #{ancestor}"
  end

  puts "\nLookup order for instance methods:"
  puts "  P (prepend) -> Child -> N (include) -> Parent -> M -> Object -> Kernel -> BasicObject"

  "ancestors method shows the full lookup chain"
end

# ============================================================================
# PART 15: Real-world Rails-style example
# ============================================================================

test "Real-world: Timestamped concern" do
  module Timestampable
    def save
      self.updated_at = Time.now
      super
      puts "  [Timestampable] Set updated_at to #{self.updated_at}"
    end

    attr_accessor :updated_at
  end

  class Article
    prepend Timestampable

    def save
      puts "  [Article] Saving article to database..."
      "Article saved"
    end
  end

  article = Article.new
  result = article.save
  puts "Final result: #{result}"
  puts "updated_at: #{article.updated_at}"

  "prepend enables transparent decoration of existing methods"
end

# ============================================================================
# SUMMARY
# ============================================================================

puts "\n" + "=" * 70
puts "SUMMARY"
puts "=" * 70
puts
puts "include M  → Instance methods added AFTER the class"
puts "             Class method beats included method"
puts "             obj.method works, Class.method doesn't"
puts
puts "prepend M  → Instance methods added BEFORE the class"
puts "             Prepended method beats class method"
puts "             Perfect for decorators with super"
puts
puts "extend M   → Adds to singleton class (class methods)"
puts "             Class.method works, obj.method doesn't"
puts "             Can also extend individual objects"
puts
puts "Multiple operations:"
puts "  • Multiple includes: last include is checked first"
puts "  • Multiple prepends: prepends are in reverse order"
puts "  • Prepends always come before class"
puts "  • Includes always come after class"
puts
puts "Lookup order:"
puts "  [Prepends] → [Class] → [Includes] → [Superclass] → ... → BasicObject"
puts
puts "Use .ancestors to see the actual lookup chain!"
puts "=" * 70
