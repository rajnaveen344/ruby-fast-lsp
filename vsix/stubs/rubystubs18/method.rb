# frozen_string_literal: true

class Method
  # Two method objects are equal if that are bound to the same
  # object and contain the same body.
  def ==(other) end

  # Returns an indication of the number of arguments accepted by a
  # method. Returns a nonnegative integer for methods that take a fixed
  # number of arguments. For Ruby methods that take a variable number of
  # arguments, returns -n-1, where n is the number of required
  # arguments. For methods written in C, returns -1 if the call takes a
  # variable number of arguments.
  #
  #    class C
  #      def one;    end
  #      def two(a); end
  #      def three(*a);  end
  #      def four(a, b); end
  #      def five(a, b, *c);    end
  #      def six(a, b, *c, &d); end
  #    end
  #    c = C.new
  #    c.method(:one).arity     #=> 0
  #    c.method(:two).arity     #=> 1
  #    c.method(:three).arity   #=> -1
  #    c.method(:four).arity    #=> 2
  #    c.method(:five).arity    #=> -3
  #    c.method(:six).arity     #=> -3
  #
  #    "cat".method(:size).arity      #=> 0
  #    "cat".method(:replace).arity   #=> 1
  #    "cat".method(:squeeze).arity   #=> -1
  #    "cat".method(:count).arity     #=> -1
  def arity; end

  # Invokes the <i>meth</i> with the specified arguments, returning the
  # method's return value.
  #
  #    m = 12.method("+")
  #    m.call(3)    #=> 15
  #    m.call(20)   #=> 32
  def call(*args) end
  alias [] call

  # MISSING: documentation
  def clone; end

  # Show the name of the underlying method.
  #
  #   "cat".method(:count).inspect   #=> "#<Method: String#count>"
  def inspect; end
  alias to_s inspect

  # Returns the name of the method.
  def name; end

  # Returns the class or module that defines the method.
  def owner; end

  # Returns the bound receiver of the method object.
  def receiver; end

  # Returns a <code>Proc</code> object corresponding to this method.
  def to_proc; end

  # Dissociates <i>meth</i> from it's current receiver. The resulting
  # <code>UnboundMethod</code> can subsequently be bound to a new object
  # of the same class (see <code>UnboundMethod</code>).
  def unbind; end
end
