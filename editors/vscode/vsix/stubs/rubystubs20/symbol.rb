# frozen_string_literal: true

# <code>Symbol</code> objects represent names and some strings
# inside the Ruby
# interpreter. They are generated using the <code>:name</code> and
# <code>:"string"</code> literals
# syntax, and by the various <code>to_sym</code> methods. The same
# <code>Symbol</code> object will be created for a given name or string
# for the duration of a program's execution, regardless of the context
# or meaning of that name. Thus if <code>Fred</code> is a constant in
# one context, a method in another, and a class in a third, the
# <code>Symbol</code> <code>:Fred</code> will be the same object in
# all three contexts.
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

  #   symbol <=> other_symbol       -> -1, 0, +1 or nil
  #
  # Compares +symbol+ with +other_symbol+ after calling #to_s on each of the
  # symbols. Returns -1, 0, +1 or nil depending on whether +symbol+ is less
  # than, equal to, or greater than +other_symbol+.
  #
  #  +nil+ is returned if the two values are incomparable.
  #
  # See String#<=> for more information.
  def <=>(other) end

  # Equality---If <i>sym</i> and <i>obj</i> are exactly the same
  # symbol, returns <code>true</code>.
  def ==(other) end
  alias === ==

  # Returns <code>sym.to_s =~ obj</code>.
  def =~(obj) end
  alias match =~

  # Returns <code>sym.to_s[]</code>.
  def [](*several_variants) end
  alias slice []

  # Same as <code>sym.to_s.capitalize.intern</code>.
  def capitalize; end

  #   sym.casecmp(other)  -> -1, 0, +1 or nil
  #
  # Case-insensitive version of <code>Symbol#<=></code>.
  def casecmp(p1) end

  # Same as <code>sym.to_s.downcase.intern</code>.
  def downcase; end

  # Returns that _sym_ is :"" or not.
  def empty?; end

  # Returns the Encoding object that represents the encoding of _sym_.
  def encoding; end

  # Returns the representation of <i>sym</i> as a symbol literal.
  #
  #    :fred.inspect   #=> ":fred"
  def inspect; end

  # In general, <code>to_sym</code> returns the <code>Symbol</code> corresponding
  # to an object. As <i>sym</i> is already a symbol, <code>self</code> is returned
  # in this case.
  def intern; end
  alias to_sym intern

  # Same as <code>sym.to_s.length</code>.
  def length; end
  alias size length

  #   sym.succ
  #
  # Same as <code>sym.to_s.succ.intern</code>.
  def succ; end
  alias next succ

  # Same as <code>sym.to_s.swapcase.intern</code>.
  def swapcase; end

  # Returns a _Proc_ object which respond to the given method by _sym_.
  #
  #   (1..3).collect(&:to_s)  #=> ["1", "2", "3"]
  def to_proc; end

  # Returns the name or string corresponding to <i>sym</i>.
  #
  #    :fred.id2name   #=> "fred"
  def to_s; end
  alias id2name to_s

  # Same as <code>sym.to_s.upcase.intern</code>.
  def upcase; end
end
