# frozen_string_literal: true

# \Class Exception and its subclasses are used to communicate between
# Kernel#raise and +rescue+ statements in <code>begin ... end</code> blocks.
#
# An Exception object carries information about an exception:
# - Its type (the exception's class).
# - An optional descriptive message.
# - Optional backtrace information.
#
# Some built-in subclasses of Exception have additional methods: e.g., NameError#name.
#
# == Defaults
#
# Two Ruby statements have default exception classes:
# - +raise+: defaults to RuntimeError.
# - +rescue+: defaults to StandardError.
#
# == Global Variables
#
# When an exception has been raised but not yet handled (in +rescue+,
# +ensure+, +at_exit+ and +END+ blocks), two global variables are set:
# - <code>$!</code> contains the current exception.
# - <code>$@</code> contains its backtrace.
#
# == Custom Exceptions
#
# To provide additional or alternate information,
# a program may create custom exception classes
# that derive from the built-in exception classes.
#
# A good practice is for a library to create a single "generic" exception class
# (typically a subclass of StandardError or RuntimeError)
# and have its other exception classes derive from that class.
# This allows the user to rescue the generic exception, thus catching all exceptions
# the library may raise even if future versions of the library add new
# exception subclasses.
#
# For example:
#
#   class MyLibrary
#     class Error < ::StandardError
#     end
#
#     class WidgetError < Error
#     end
#
#     class FrobError < Error
#     end
#
#   end
#
# To handle both MyLibrary::WidgetError and MyLibrary::FrobError the library
# user can rescue MyLibrary::Error.
#
# == Built-In Exception Classes
#
# The built-in subclasses of Exception are:
#
# * NoMemoryError
# * ScriptError
#   * LoadError
#   * NotImplementedError
#   * SyntaxError
# * SecurityError
# * SignalException
#   * Interrupt
# * StandardError
#   * ArgumentError
#     * UncaughtThrowError
#   * EncodingError
#   * FiberError
#   * IOError
#     * EOFError
#   * IndexError
#     * KeyError
#     * StopIteration
#       * ClosedQueueError
#   * LocalJumpError
#   * NameError
#     * NoMethodError
#   * RangeError
#     * FloatDomainError
#   * RegexpError
#   * RuntimeError
#     * FrozenError
#   * SystemCallError
#     * Errno::*
#   * ThreadError
#   * TypeError
#   * ZeroDivisionError
# * SystemExit
# * SystemStackError
# * fatal
class Exception
  # With no argument, or if the argument is the same as the receiver,
  # return the receiver. Otherwise, create a new
  # exception object of the same class as the receiver, but with a
  # message equal to <code>string.to_str</code>.
  def self.exception(*string) end

  # Returns +true+ if exception messages will be sent to a tty.
  def self.to_tty?; end

  #  Construct a new Exception object, optionally passing in
  #  a message.
  def initialize(msg = nil) end

  # Equality---If <i>obj</i> is not an Exception, returns
  # <code>false</code>. Otherwise, returns <code>true</code> if <i>exc</i> and
  # <i>obj</i> share same class, messages, and backtrace.
  def ==(other) end

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
  #
  # In the case no backtrace has been set, +nil+ is returned
  #
  #   ex = StandardError.new
  #   ex.backtrace
  #   #=> nil
  def backtrace; end

  # Returns any backtrace associated with the exception. This method is
  # similar to Exception#backtrace, but the backtrace is an array of
  # Thread::Backtrace::Location.
  #
  # This method is not affected by Exception#set_backtrace().
  def backtrace_locations; end

  # Returns the previous exception ($!) at the time this exception was raised.
  # This is useful for wrapping exceptions and retaining the original exception
  # information.
  def cause; end

  # Processes a string returned by #message.
  #
  # It may add the class name of the exception to the end of the first line.
  # Also, when +highlight+ keyword is true, it adds ANSI escape sequences to
  # make the message bold.
  #
  # If you override this method, it must be tolerant for unknown keyword
  # arguments. All keyword arguments passed to #full_message are delegated
  # to this method.
  #
  # This method is overridden by did_you_mean and error_highlight to add
  # their information.
  #
  # A user-defined exception class can also define their own
  # +detailed_message+ method to add supplemental information.
  # When +highlight+ is true, it can return a string containing escape
  # sequences, but use widely-supported ones. It is recommended to limit
  # the following codes:
  #
  # - Reset (+\e[0m+)
  # - Bold (+\e[1m+)
  # - Underline (+\e[4m+)
  # - Foreground color except white and black
  #   - Red (+\e[31m+)
  #   - Green (+\e[32m+)
  #   - Yellow (+\e[33m+)
  #   - Blue (+\e[34m+)
  #   - Magenta (+\e[35m+)
  #   - Cyan (+\e[36m+)
  #
  # Use escape sequences carefully even if +highlight+ is true.
  # Do not use escape sequences to express essential information;
  # the message should be readable even if all escape sequences are
  # ignored.
  def detailed_message(highlight: bool, **opt) end

  # With no argument, or if the argument is the same as the receiver,
  # return the receiver. Otherwise, create a new
  # exception object of the same class as the receiver, but with a
  # message equal to <code>string.to_str</code>.
  def exception(*string) end

  # Returns formatted string of _exception_.
  # The returned string is formatted using the same format that Ruby uses
  # when printing an uncaught exceptions to stderr.
  #
  # If _highlight_ is +true+ the default error handler will send the
  # messages to a tty.
  #
  # _order_ must be either of +:top+ or +:bottom+, and places the error
  # message and the innermost backtrace come at the top or the bottom.
  #
  # The default values of these options depend on <code>$stderr</code>
  # and its +tty?+ at the timing of a call.
  def full_message(p1 = {}) end

  # Return this exception's class name and message.
  def inspect; end

  # Returns the result of invoking <code>exception.to_s</code>.
  # Normally this returns the exception's message or name.
  def message; end

  # Sets the backtrace information associated with +exc+. The +backtrace+ must
  # be an array of String objects or a single String in the format described
  # in Exception#backtrace.
  def set_backtrace(backtrace) end

  # Returns exception's message (or the name of the exception if
  # no message is set).
  def to_s; end
end
