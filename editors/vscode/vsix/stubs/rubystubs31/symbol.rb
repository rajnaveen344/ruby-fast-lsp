# frozen_string_literal: true

# Symbol objects represent named identifiers inside the Ruby interpreter.
#
# You can create a \Symbol object explicitly with:
#
# - A {symbol literal}[doc/syntax/literals_rdoc.html#label-Symbol+Literals].
#
# The same Symbol object will be
# created for a given name or string for the duration of a program's
# execution, regardless of the context or meaning of that name. Thus
# if <code>Fred</code> is a constant in one context, a method in
# another, and a class in a third, the Symbol <code>:Fred</code>
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
# Symbol objects are different from String objects in that
# Symbol objects represent identifiers, while String objects
# represent text or data.
#
# == What's Here
#
# First, what's elsewhere. \Class \Symbol:
#
# - Inherits from {class Object}[Object.html#class-Object-label-What-27s+Here].
# - Includes {module Comparable}[Comparable.html#module-Comparable-label-What-27s+Here].
#
# Here, class \Symbol provides methods that are useful for:
#
# - {Querying}[#class-Symbol-label-Methods+for+Querying]
# - {Comparing}[#class-Symbol-label-Methods+for+Comparing]
# - {Converting}[#class-Symbol-label-Methods+for+Converting]
#
# === Methods for Querying
#
# - ::all_symbols:: Returns an array of the symbols currently in Ruby's symbol table.
# - {#=~}[#method-i-3D~]:: Returns the index of the first substring
#                          in symbol that matches a given Regexp
#                          or other object; returns +nil+ if no match is found.
# - #[], #slice :: Returns a substring of symbol
#                  determined by a given index, start/length, or range, or string.
# - #empty?:: Returns +true+ if +self.length+ is zero; +false+ otherwise.
# - #encoding:: Returns the Encoding object that represents the encoding
#               of symbol.
# - #end_with?:: Returns +true+ if symbol ends with
#                any of the given strings.
# - #match:: Returns a MatchData object if symbol
#            matches a given Regexp; +nil+ otherwise.
# - #match?:: Returns +true+ if symbol
#             matches a given Regexp; +false+ otherwise.
# - #length, #size:: Returns the number of characters in symbol.
# - #start_with?:: Returns +true+ if symbol starts with
#                  any of the given strings.
#
# === Methods for Comparing
#
# - {#<=>}[#method-i-3C-3D-3E]:: Returns -1, 0, or 1 as a given symbol is smaller than, equal to, or larger than symbol.
# - {#==, #===}[#method-i-3D-3D]:: Returns +true+ if a given symbol
#                                  has the same content and encoding.
# - #casecmp:: Ignoring case, returns -1, 0, or 1 as a given
#              symbol is smaller than, equal to, or larger than symbol.
# - #casecmp?:: Returns +true+ if symbol is equal to a given symbol
#               after Unicode case folding; +false+ otherwise.
#
# === Methods for Converting
#
# - #capitalize:: Returns symbol with the first character upcased
#                 and all other characters downcased.
# - #downcase:: Returns symbol with all characters downcased.
# - #inspect:: Returns the string representation of +self+ as a symbol literal.
# - #name:: Returns the frozen string corresponding to symbol.
# - #succ, #next:: Returns the symbol that is the successor to symbol.
# - #swapcase:: Returns symbol with all upcase characters downcased
#               and all downcase characters upcased.
# - #to_proc:: Returns a Proc object which responds to the method named by symbol.
# - #to_s, #id2name:: Returns the string corresponding to +self+.
# - #to_sym, #intern:: Returns +self+.
# - #upcase:: Returns symbol with all characters upcased.
class Symbol
  include Comparable

  # Returns an array of all the symbols currently in Ruby's symbol
  # table.
  #
  #    Symbol.all_symbols.size    #=> 903
  #    Symbol.all_symbols[1,20]   #=> [:floor, :ARGV, :Binding, :symlink,
  #                                    :chown, :EOFError, :$;, :String,
  #                                    :LOCK_SH, :"setuid?", :$<,
  #                                    :default_proc, :compact, :extend,
  #                                    :Tms, :getwd, :$=, :ThreadGroup,
  #                                    :wait2, :$>]
  def self.all_symbols; end

  #   symbol <=> other_symbol       -> -1, 0, +1, or nil
  #
  # Compares +symbol+ with +other_symbol+ after calling #to_s on each of the
  # symbols. Returns -1, 0, +1, or +nil+ depending on whether +symbol+ is
  # less than, equal to, or greater than +other_symbol+.
  #
  # +nil+ is returned if the two values are incomparable.
  #
  # See String#<=> for more information.
  def <=>(other) end

  # Equality---If <i>sym</i> and <i>obj</i> are exactly the same
  # symbol, returns <code>true</code>.
  def ==(other) end
  alias === ==

  # Returns <code>sym.to_s =~ obj</code>.
  def =~(obj) end

  # Returns <code>sym.to_s[]</code>.
  def [](...) end
  alias slice []

  # Equivalent to <tt>sym.to_s.capitalize.to_sym</tt>.
  #
  # See String#capitalize.
  def capitalize(*options) end

  # Case-insensitive version of {Symbol#<=>}[#method-i-3C-3D-3E]:
  #
  #   :aBcDeF.casecmp(:abcde)   # => 1
  #   :aBcDeF.casecmp(:abcdef)  # => 0
  #   :aBcDeF.casecmp(:abcdefg) # => -1
  #   :abcdef.casecmp(:ABCDEF)  # => 0
  #
  # Returns +nil+ if the two symbols have incompatible encodings,
  # or if +other_symbol+ is not a symbol:
  #
  #   sym = "\u{e4 f6 fc}".encode("ISO-8859-1").to_sym
  #   other_sym = :"\u{c4 d6 dc}"
  #   sym.casecmp(other_sym) # => nil
  #   :foo.casecmp(2)        # => nil
  #
  # Currently, case-insensitivity only works on characters A-Z/a-z,
  # not all of Unicode. This is different from Symbol#casecmp?.
  #
  # Related: Symbol#casecmp?.
  def casecmp(other_symbol) end

  # Returns +true+ if +sym+ and +other_symbol+ are equal after
  # Unicode case folding, +false+ if they are not equal:
  #
  #   :aBcDeF.casecmp?(:abcde)                  # => false
  #   :aBcDeF.casecmp?(:abcdef)                 # => true
  #   :aBcDeF.casecmp?(:abcdefg)                # => false
  #   :abcdef.casecmp?(:ABCDEF)                 # => true
  #   :"\u{e4 f6 fc}".casecmp?(:"\u{c4 d6 dc}") #=> true
  #
  # Returns +nil+ if the two symbols have incompatible encodings,
  # or if +other_symbol+ is not a symbol:
  #
  #   sym = "\u{e4 f6 fc}".encode("ISO-8859-1").to_sym
  #   other_sym = :"\u{c4 d6 dc}"
  #   sym.casecmp?(other_sym) # => nil
  #   :foo.casecmp?(2)        # => nil
  #
  # See {Case Mapping}[doc/case_mapping_rdoc.html].
  #
  # Related: Symbol#casecmp.
  def casecmp?(other_symbol) end

  # Equivalent to <tt>sym.to_s.downcase.to_sym</tt>.
  #
  # See String#downcase.
  #
  # Related: Symbol#upcase.
  def downcase(*options) end

  # Returns whether _sym_ is :"" or not.
  def empty?; end

  # Returns the Encoding object that represents the encoding of _sym_.
  def encoding; end

  # Returns true if +sym+ ends with one of the +suffixes+ given.
  #
  #   :hello.end_with?("ello")               #=> true
  #
  #   # returns true if one of the +suffixes+ matches.
  #   :hello.end_with?("heaven", "ello")     #=> true
  #   :hello.end_with?("heaven", "paradise") #=> false
  def end_with?(*args) end

  # Returns the representation of <i>sym</i> as a symbol literal.
  #
  #    :fred.inspect   #=> ":fred"
  def inspect; end

  # In general, <code>to_sym</code> returns the Symbol corresponding
  # to an object. As <i>sym</i> is already a symbol, <code>self</code> is returned
  # in this case.
  def intern; end
  alias to_sym intern

  # Same as <code>sym.to_s.length</code>.
  def length; end
  alias size length

  # Returns <code>sym.to_s.match</code>.
  def match(...) end

  # Returns <code>sym.to_s.match?</code>.
  def match?(...) end

  # Returns the name or string corresponding to <i>sym</i>. Unlike #to_s, the
  # returned string is frozen.
  #
  #    :fred.name         #=> "fred"
  #    :fred.name.frozen? #=> true
  #    :fred.to_s         #=> "fred"
  #    :fred.to_s.frozen? #=> false
  def name; end

  # Returns true if +sym+ starts with one of the +prefixes+ given.
  # Each of the +prefixes+ should be a String or a Regexp.
  #
  #   :hello.start_with?("hell")               #=> true
  #   :hello.start_with?(/H/i)                 #=> true
  #
  #   # returns true if one of the prefixes matches.
  #   :hello.start_with?("heaven", "hell")     #=> true
  #   :hello.start_with?("heaven", "paradise") #=> false
  def start_with?(*args) end

  #   sym.succ
  #
  # Same as <code>sym.to_s.succ.intern</code>.
  def succ; end
  alias next succ

  # Equivalent to <tt>sym.to_s.swapcase.to_sym</tt>.
  #
  # See String#swapcase.
  def swapcase(*options) end

  # Returns a _Proc_ object which responds to the given method by _sym_.
  #
  #   (1..3).collect(&:to_s)  #=> ["1", "2", "3"]
  def to_proc; end

  # Returns the name or string corresponding to <i>sym</i>.
  #
  #    :fred.id2name   #=> "fred"
  #    :ginger.to_s    #=> "ginger"
  #
  # Note that this string is not frozen (unlike the symbol itself).
  # To get a frozen string, use #name.
  def to_s; end
  alias id2name to_s

  # Equivalent to <tt>sym.to_s.upcase.to_sym</tt>.
  #
  # See String#upcase.
  def upcase(*options) end
end
