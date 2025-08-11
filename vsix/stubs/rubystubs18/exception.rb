# frozen_string_literal: true

# Descendents of class <code>Exception</code> are used to communicate
# between <code>raise</code> methods and <code>rescue</code>
# statements in <code>begin/end</code> blocks. <code>Exception</code>
# objects carry information about the exception---its type (the
# exception's class name), an optional descriptive string, and
# optional traceback information. Programs may subclass
# <code>Exception</code> to add additional information.
class Exception
  # With no argument, or if the argument is the same as the receiver,
  # return the receiver. Otherwise, create a new
  # exception object of the same class as the receiver, but with a
  # message equal to <code>string.to_str</code>.
  def self.exception(string) end

  #  Construct a new Exception object, optionally passing in
  #  a message.
  def initialize(msg = nil) end

  # Returns any backtrace associated with the exception. The backtrace
  # is an array of strings, each containing either ``filename:lineNo: in
  # `method''' or ``filename:lineNo.''
  #
  #    def a
  #      raise "boom"
  #    end
  #
  #    def b
  #      a()
  #    end
  #
  #    begin
  #      b()
  #    rescue => detail
  #      print detail.backtrace.join("\n")
  #    end
  #
  # <em>produces:</em>
  #
  #    prog.rb:2:in `a'
  #    prog.rb:6:in `b'
  #    prog.rb:10
  def backtrace; end

  # With no argument, or if the argument is the same as the receiver,
  # return the receiver. Otherwise, create a new
  # exception object of the same class as the receiver, but with a
  # message equal to <code>string.to_str</code>.
  def exception(string) end

  # Return this exception's class name an message
  def inspect; end

  # Sets the backtrace information associated with <i>exc</i>. The
  # argument must be an array of <code>String</code> objects in the
  # format described in <code>Exception#backtrace</code>.
  def set_backtrace(array) end

  # Returns exception's message (or the name of the exception if
  # no message is set).
  def to_s; end

  # Returns the result of invoking <code>exception.to_s</code>.
  # Normally this returns the exception's message or name. By
  # supplying a to_str method, exceptions are agreeing to
  # be used where Strings are expected.
  def to_str; end
  alias message to_str
end
