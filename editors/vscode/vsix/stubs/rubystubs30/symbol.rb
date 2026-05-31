# frozen_string_literal: true

# Symbol objects represent names inside the Ruby interpreter. They
# are generated using the <code>:name</code> and
# <code>:"string"</code> literals syntax, and by the various
# <code>to_sym</code> methods. The same Symbol object will be
# created for a given name or string for the duration of a program's
# execution, regardless of the context or meaning of that name. Thus
# if <code>Fred</code> is a constant in one context, a method in
# another, and a class in a third, the Symbol <code>:Fred</code>
# will be the same object in all three contexts.
#
#    module One
#      class Fred
#      end
#      $f1 = :Fred
#    end
#    module Two
#      Fred = 1
#      $f2 = :Fred
#    end
#    def Fred()
#    end
#    $f3 = :Fred
#    $f1.object_id   #=> 2514190
#    $f2.object_id   #=> 2514190
#    $f3.object_id   #=> 2514190
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

  # Same as <code>sym.to_s.capitalize.intern</code>.
  def capitalize(...) end

  # Case-insensitive version of Symbol#<=>.
  # Currently, case-insensitivity only works on characters A-Z/a-z,
  # not all of Unicode. This is different from Symbol#casecmp?.
  #
  #   :aBcDeF.casecmp(:abcde)     #=> 1
  #   :aBcDeF.casecmp(:abcdef)    #=> 0
  #   :aBcDeF.casecmp(:abcdefg)   #=> -1
  #   :abcdef.casecmp(:ABCDEF)    #=> 0
  #
  # +nil+ is returned if the two symbols have incompatible encodings,
  # or if +other_symbol+ is not a symbol.
  #
  #   :foo.casecmp(2)   #=> nil
  #   "\u{e4 f6 fc}".encode("ISO-8859-1").to_sym.casecmp(:"\u{c4 d6 dc}")   #=> nil
  def casecmp(other_symbol) end

  # Returns +true+ if +sym+ and +other_symbol+ are equal after
  # Unicode case folding, +false+ if they are not equal.
  #
  #   :aBcDeF.casecmp?(:abcde)     #=> false
  #   :aBcDeF.casecmp?(:abcdef)    #=> true
  #   :aBcDeF.casecmp?(:abcdefg)   #=> false
  #   :abcdef.casecmp?(:ABCDEF)    #=> true
  #   :"\u{e4 f6 fc}".casecmp?(:"\u{c4 d6 dc}")   #=> true
  #
  # +nil+ is returned if the two symbols have incompatible encodings,
  # or if +other_symbol+ is not a symbol.
  #
  #   :foo.casecmp?(2)   #=> nil
  #   "\u{e4 f6 fc}".encode("ISO-8859-1").to_sym.casecmp?(:"\u{c4 d6 dc}")   #=> nil
  def casecmp?(other_symbol) end

  # Same as <code>sym.to_s.downcase.intern</code>.
  def downcase(...) end

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

  # Same as <code>sym.to_s.swapcase.intern</code>.
  def swapcase(...) end

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

  # Same as <code>sym.to_s.upcase.intern</code>.
  def upcase(...) end
end
