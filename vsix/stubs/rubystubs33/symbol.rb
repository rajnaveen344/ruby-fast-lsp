# frozen_string_literal: true

# \Symbol objects represent named identifiers inside the Ruby interpreter.
#
# You can create a \Symbol object explicitly with:
#
# - A {symbol literal}[rdoc-ref:syntax/literals.rdoc@Symbol+Literals].
#
# The same \Symbol object will be
# created for a given name or string for the duration of a program's
# execution, regardless of the context or meaning of that name. Thus
# if <code>Fred</code> is a constant in one context, a method in
# another, and a class in a third, the \Symbol <code>:Fred</code>
# will be the same object in all three contexts.
#
#     module One
#       class Fred
#       end
#       $f1 = :Fred
#     end
#     module Two
#       Fred = 1
#       $f2 = :Fred
#     end
#     def Fred()
#     end
#     $f3 = :Fred
#     $f1.object_id   #=> 2514190
#     $f2.object_id   #=> 2514190
#     $f3.object_id   #=> 2514190
#
# Constant, method, and variable names are returned as symbols:
#
#     module One
#       Two = 2
#       def three; 3 end
#       @four = 4
#       @@five = 5
#       $six = 6
#     end
#     seven = 7
#
#     One.constants
#     # => [:Two]
#     One.instance_methods(true)
#     # => [:three]
#     One.instance_variables
#     # => [:@four]
#     One.class_variables
#     # => [:@@five]
#     global_variables.grep(/six/)
#     # => [:$six]
#     local_variables
#     # => [:seven]
#
# \Symbol objects are different from String objects in that
# \Symbol objects represent identifiers, while String objects
# represent text or data.
#
# == What's Here
#
# First, what's elsewhere. \Class \Symbol:
#
# - Inherits from {class Object}[rdoc-ref:Object@What-27s+Here].
# - Includes {module Comparable}[rdoc-ref:Comparable@What-27s+Here].
#
# Here, class \Symbol provides methods that are useful for:
#
# - {Querying}[rdoc-ref:Symbol@Methods+for+Querying]
# - {Comparing}[rdoc-ref:Symbol@Methods+for+Comparing]
# - {Converting}[rdoc-ref:Symbol@Methods+for+Converting]
#
# === Methods for Querying
#
# - ::all_symbols: Returns an array of the symbols currently in Ruby's symbol table.
# - #=~: Returns the index of the first substring in symbol that matches a
#   given Regexp or other object; returns +nil+ if no match is found.
# - #[], #slice : Returns a substring of symbol
#   determined by a given index, start/length, or range, or string.
# - #empty?: Returns +true+ if +self.length+ is zero; +false+ otherwise.
# - #encoding: Returns the Encoding object that represents the encoding
#   of symbol.
# - #end_with?: Returns +true+ if symbol ends with
#   any of the given strings.
# - #match: Returns a MatchData object if symbol
#   matches a given Regexp; +nil+ otherwise.
# - #match?: Returns +true+ if symbol
#   matches a given Regexp; +false+ otherwise.
# - #length, #size: Returns the number of characters in symbol.
# - #start_with?: Returns +true+ if symbol starts with
#   any of the given strings.
#
# === Methods for Comparing
#
# - #<=>: Returns -1, 0, or 1 as a given symbol is smaller than, equal to,
#   or larger than symbol.
# - #==, #===: Returns +true+ if a given symbol has the same content and
#   encoding.
# - #casecmp: Ignoring case, returns -1, 0, or 1 as a given
#   symbol is smaller than, equal to, or larger than symbol.
# - #casecmp?: Returns +true+ if symbol is equal to a given symbol
#   after Unicode case folding; +false+ otherwise.
#
# === Methods for Converting
#
# - #capitalize: Returns symbol with the first character upcased
#   and all other characters downcased.
# - #downcase: Returns symbol with all characters downcased.
# - #inspect: Returns the string representation of +self+ as a symbol literal.
# - #name: Returns the frozen string corresponding to symbol.
# - #succ, #next: Returns the symbol that is the successor to symbol.
# - #swapcase: Returns symbol with all upcase characters downcased
#   and all downcase characters upcased.
# - #to_proc: Returns a Proc object which responds to the method named by symbol.
# - #to_s, #id2name: Returns the string corresponding to +self+.
# - #to_sym, #intern: Returns +self+.
# - #upcase: Returns symbol with all characters upcased.
class Symbol
  include Comparable

  # Returns an array of all symbols currently in Ruby's symbol table:
  #
  #   Symbol.all_symbols.size    # => 9334
  #   Symbol.all_symbols.take(3) # => [:!, :"\"", :"#"]
  def self.all_symbols; end

  # If +object+ is a symbol,
  # returns the equivalent of <tt>symbol.to_s <=> object.to_s</tt>:
  #
  #   :bar <=> :foo # => -1
  #   :foo <=> :foo # => 0
  #   :foo <=> :bar # => 1
  #
  # Otherwise, returns +nil+:
  #
  #  :foo <=> 'bar' # => nil
  #
  # Related: String#<=>.
  def <=>(other) end

  # Returns +true+ if +object+ is the same object as +self+, +false+ otherwise.
  def ==(other) end
  alias === ==

  # Equivalent to <tt>symbol.to_s =~ object</tt>,
  # including possible updates to global variables;
  # see String#=~.
  def =~(object) end

  # Equivalent to <tt>symbol.to_s[]</tt>; see String#[].
  def [](...) end
  alias slice []

  # Equivalent to <tt>sym.to_s.capitalize.to_sym</tt>.
  #
  # See String#capitalize.
  def capitalize(*options) end

  # Like Symbol#<=>, but case-insensitive;
  # equivalent to <tt>self.to_s.casecmp(object.to_s)</tt>:
  #
  #   lower = :abc
  #   upper = :ABC
  #   upper.casecmp(lower) # => 0
  #   lower.casecmp(lower) # => 0
  #   lower.casecmp(upper) # => 0
  #
  # Returns nil if +self+ and +object+ have incompatible encodings,
  # or if +object+ is not a symbol:
  #
  #   sym = 'äöü'.encode("ISO-8859-1").to_sym
  #   other_sym = 'ÄÖÜ'
  #   sym.casecmp(other_sym) # => nil
  #   :foo.casecmp(2)        # => nil
  #
  # Unlike Symbol#casecmp?,
  # case-insensitivity does not work for characters outside of 'A'..'Z' and 'a'..'z':
  #
  #   lower = :äöü
  #   upper = :ÄÖÜ
  #   upper.casecmp(lower) # => -1
  #   lower.casecmp(lower) # => 0
  #   lower.casecmp(upper) # => 1
  #
  # Related: Symbol#casecmp?, String#casecmp.
  def casecmp(object) end

  # Returns +true+ if +self+ and +object+ are equal after Unicode case folding,
  # otherwise +false+:
  #
  #   lower = :abc
  #   upper = :ABC
  #   upper.casecmp?(lower) # => true
  #   lower.casecmp?(lower) # => true
  #   lower.casecmp?(upper) # => true
  #
  # Returns nil if +self+ and +object+ have incompatible encodings,
  # or if +object+ is not a symbol:
  #
  #   sym = 'äöü'.encode("ISO-8859-1").to_sym
  #   other_sym = 'ÄÖÜ'
  #   sym.casecmp?(other_sym) # => nil
  #   :foo.casecmp?(2)        # => nil
  #
  # Unlike Symbol#casecmp, works for characters outside of 'A'..'Z' and 'a'..'z':
  #
  #   lower = :äöü
  #   upper = :ÄÖÜ
  #   upper.casecmp?(lower) # => true
  #   lower.casecmp?(lower) # => true
  #   lower.casecmp?(upper) # => true
  #
  # Related: Symbol#casecmp, String#casecmp?.
  def casecmp?(object) end

  # Equivalent to <tt>sym.to_s.downcase.to_sym</tt>.
  #
  # See String#downcase.
  #
  # Related: Symbol#upcase.
  def downcase(*options) end

  # Returns +true+ if +self+ is <tt>:''</tt>, +false+ otherwise.
  def empty?; end

  # Equivalent to <tt>self.to_s.encoding</tt>; see String#encoding.
  def encoding; end

  # Equivalent to <tt>self.to_s.end_with?</tt>; see String#end_with?.
  def end_with?(*strings) end

  # Returns a string representation of +self+ (including the leading colon):
  #
  #   :foo.inspect # => ":foo"
  #
  # Related:  Symbol#to_s, Symbol#name.
  def inspect; end

  # Equivalent to <tt>self.to_s.length</tt>; see String#length.
  def length; end
  alias size length

  # Equivalent to <tt>self.to_s.match</tt>,
  # including possible updates to global variables;
  # see String#match.
  def match(pattern, offset = 0) end

  # Equivalent to <tt>sym.to_s.match?</tt>;
  # see String#match.
  def match?(pattern, offset) end

  # Returns a frozen string representation of +self+ (not including the leading colon):
  #
  #   :foo.name         # => "foo"
  #   :foo.name.frozen? # => true
  #
  # Related: Symbol#to_s, Symbol#inspect.
  def name; end

  # Equivalent to <tt>self.to_s.start_with?</tt>; see String#start_with?.
  def start_with?(*string_or_regexp) end

  # Equivalent to <tt>self.to_s.succ.to_sym</tt>:
  #
  #   :foo.succ # => :fop
  #
  # Related: String#succ.
  def succ; end
  alias next succ

  # Equivalent to <tt>sym.to_s.swapcase.to_sym</tt>.
  #
  # See String#swapcase.
  def swapcase(*options) end

  # Returns a Proc object which calls the method with name of +self+
  # on the first parameter and passes the remaining parameters to the method.
  #
  #   proc = :to_s.to_proc   # => #<Proc:0x000001afe0e48680(&:to_s) (lambda)>
  #   proc.call(1000)        # => "1000"
  #   proc.call(1000, 16)    # => "3e8"
  #   (1..3).collect(&:to_s) # => ["1", "2", "3"]
  def to_proc; end

  # Returns a string representation of +self+ (not including the leading colon):
  #
  #   :foo.to_s # => "foo"
  #
  # Related: Symbol#inspect, Symbol#name.
  def to_s; end
  alias id2name to_s

  # Returns +self+.
  #
  # Related: String#to_sym.
  def to_sym; end
  alias intern to_sym

  # Equivalent to <tt>sym.to_s.upcase.to_sym</tt>.
  #
  # See String#upcase.
  def upcase(*options) end
end
