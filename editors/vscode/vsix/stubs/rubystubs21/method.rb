# frozen_string_literal: true

# Method
class Method
  # Two method objects are equal if they are bound to the same
  # object and refer to the same method definition and their owners are the
  # same class or module.
  def ==(other) end
  alias eql? ==

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

  # Returns a clone of this method.
  #
  #   class A
  #     def foo
  #       return "bar"
  #     end
  #   end
  #
  #   m = A.new.method(:foo)
  #   m.call # => "bar"
  #   n = m.clone.call # => "bar"
  def clone; end

  # Returns a hash value corresponding to the method object.
  def hash; end

  # Returns the name of the underlying method.
  #
  #   "cat".method(:count).inspect   #=> "#<Method: String#count>"
  def inspect; end
  alias to_s inspect

  # Returns the name of the method.
  def name; end

  # Returns the original name of the method.
  def original_name; end

  # Returns the class or module that defines the method.
  def owner; end

  # Returns the parameter information of this method.
  def parameters; end

  # Returns the bound receiver of the method object.
  def receiver; end

  # Returns the Ruby source filename and line number containing this method
  # or nil if this method was not defined in Ruby (i.e. native)
  def source_location; end

  # Returns a <code>Proc</code> object corresponding to this method.
  def to_proc; end

  # Dissociates <i>meth</i> from its current receiver. The resulting
  # <code>UnboundMethod</code> can subsequently be bound to a new object
  # of the same class (see <code>UnboundMethod</code>).
  def unbind; end
end
