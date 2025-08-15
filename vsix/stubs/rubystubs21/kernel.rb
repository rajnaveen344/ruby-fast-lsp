# frozen_string_literal: true

# The Kernel module is included by class Object, so its methods are
# available in every Ruby object.
#
# The Kernel instance methods are documented in class Object while the
# module methods are documented here.  These methods are called without a
# receiver and thus can be called in functional form:
#
#   sprintf "%.1f", 1.234 #=> "1.2"
module Kernel
  # Returns the called name of the current method as a Symbol.
  # If called outside of a method, it returns <code>nil</code>.
  def self.__callee__; end

  # Returns the canonicalized absolute path of the directory of the file from
  # which this method is called. It means symlinks in the path is resolved.
  # If <code>__FILE__</code> is <code>nil</code>, it returns <code>nil</code>.
  # The return value equals to <code>File.dirname(File.realpath(__FILE__))</code>.
  def self.__dir__; end

  # Returns the name at the definition of the current method as a
  # Symbol.
  # If called outside of a method, it returns <code>nil</code>.
  def self.__method__; end

  # Returns the standard output of running _cmd_ in a subshell.
  # The built-in syntax <code>%x{...}</code> uses
  # this method. Sets <code>$?</code> to the process status.
  #
  #    `date`                   #=> "Wed Apr  9 08:56:30 CDT 2003\n"
  #    `ls testdir`.split[1]    #=> "main.rb"
  #    `echo oops && exit 99`   #=> "oops\n"
  #    $?.exitstatus            #=> 99
  def self.`(cmd) end

  # Returns +arg+ as an Array.
  #
  # First tries to call <code>to_ary</code> on +arg+, then <code>to_a</code>.
  #
  #    Array(1..5)   #=> [1, 2, 3, 4, 5]
  def self.Array(arg) end

  # See also BigDecimal::new
  def self.BigDecimal(*args) end

  # Returns x+i*y;
  #
  #    Complex(1, 2)    #=> (1+2i)
  #    Complex('1+2i')  #=> (1+2i)
  #    Complex(nil)     #=> TypeError
  #    Complex(1, nil)  #=> TypeError
  #
  # Syntax of string form:
  #
  #   string form = extra spaces , complex , extra spaces ;
  #   complex = real part | [ sign ] , imaginary part
  #           | real part , sign , imaginary part
  #           | rational , "@" , rational ;
  #   real part = rational ;
  #   imaginary part = imaginary unit | unsigned rational , imaginary unit ;
  #   rational = [ sign ] , unsigned rational ;
  #   unsigned rational = numerator | numerator , "/" , denominator ;
  #   numerator = integer part | fractional part | integer part , fractional part ;
  #   denominator = digits ;
  #   integer part = digits ;
  #   fractional part = "." , digits , [ ( "e" | "E" ) , [ sign ] , digits ] ;
  #   imaginary unit = "i" | "I" | "j" | "J" ;
  #   sign = "-" | "+" ;
  #   digits = digit , { digit | "_" , digit };
  #   digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
  #   extra spaces = ? \s* ? ;
  #
  # See String#to_c.
  def self.Complex(*args) end

  # Returns <i>arg</i> converted to a float. Numeric types are converted
  # directly, the rest are converted using <i>arg</i>.to_f.
  # Converting <code>nil</code> generates a <code>TypeError</code>.
  #
  #    Float(1)           #=> 1.0
  #    Float("123.456")   #=> 123.456
  def self.Float(arg) end

  # Converts <i>arg</i> to a <code>Hash</code> by calling
  # <i>arg</i><code>.to_hash</code>. Returns an empty <code>Hash</code> when
  # <i>arg</i> is <tt>nil</tt> or <tt>[]</tt>.
  #
  #    Hash([])          #=> {}
  #    Hash(nil)         #=> {}
  #    Hash(key: :value) #=> {:key => :value}
  #    Hash([1, 2, 3])   #=> TypeError
  def self.Hash(arg) end

  # Converts <i>arg</i> to a <code>Fixnum</code> or <code>Bignum</code>.
  # Numeric types are converted directly (with floating point numbers
  # being truncated).  <i>base</i> (0, or between 2 and 36) is a base for
  # integer string representation.  If <i>arg</i> is a <code>String</code>,
  # when <i>base</i> is omitted or equals zero, radix indicators
  # (<code>0</code>, <code>0b</code>, and <code>0x</code>) are honored.
  # In any case, strings should be strictly conformed to numeric
  # representation. This behavior is different from that of
  # <code>String#to_i</code>.  Non string values will be converted by first
  # trying <code>to_int</code>, then <code>to_i</code>. Passing <code>nil</code>
  # raises a TypeError.
  #
  #    Integer(123.999)    #=> 123
  #    Integer("0x1a")     #=> 26
  #    Integer(Time.new)   #=> 1204973019
  #    Integer("0930", 10) #=> 930
  #    Integer("111", 2)   #=> 7
  #    Integer(nil)        #=> TypeError
  def self.Integer(arg, base = 0) end

  # Creates a new Pathname object from the given string, +path+, and returns
  # pathname object.
  #
  # In order to use this constructor, you must first require the Pathname
  # standard library extension.
  #
  #      require 'pathname'
  #      Pathname("/home/zzak")
  #      #=> #<Pathname:/home/zzak>
  #
  # See also Pathname::new for more information.
  def self.Pathname(path) end

  # Returns x/y;
  #
  #    Rational(1, 2)   #=> (1/2)
  #    Rational('1/2')  #=> (1/2)
  #    Rational(nil)    #=> TypeError
  #    Rational(1, nil) #=> TypeError
  #
  # Syntax of string form:
  #
  #   string form = extra spaces , rational , extra spaces ;
  #   rational = [ sign ] , unsigned rational ;
  #   unsigned rational = numerator | numerator , "/" , denominator ;
  #   numerator = integer part | fractional part | integer part , fractional part ;
  #   denominator = digits ;
  #   integer part = digits ;
  #   fractional part = "." , digits , [ ( "e" | "E" ) , [ sign ] , digits ] ;
  #   sign = "-" | "+" ;
  #   digits = digit , { digit | "_" , digit } ;
  #   digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
  #   extra spaces = ? \s* ? ;
  #
  # See String#to_r.
  def self.Rational(*args) end

  # Returns <i>arg</i> as a <code>String</code>.
  #
  # First tries to call its <code>to_str</code> method, then its <code>to_s</code> method.
  #
  #    String(self)        #=> "main"
  #    String(self.class)  #=> "Object"
  #    String(123456)      #=> "123456"
  def self.String(arg) end

  # Terminate execution immediately, effectively by calling
  # <code>Kernel.exit(false)</code>. If _msg_ is given, it is written
  # to STDERR prior to terminating.
  def self.abort(message = '') end

  # Converts _block_ to a +Proc+ object (and therefore
  # binds it at the point of call) and registers it for execution when
  # the program exits. If multiple handlers are registered, they are
  # executed in reverse order of registration.
  #
  #    def do_at_exit(str1)
  #      at_exit { print str1 }
  #    end
  #    at_exit { puts "cruel world" }
  #    do_at_exit("goodbye ")
  #    exit
  #
  # <em>produces:</em>
  #
  #    goodbye cruel world
  def self.at_exit; end

  # Registers _filename_ to be loaded (using <code>Kernel::require</code>)
  # the first time that _module_ (which may be a <code>String</code> or
  # a symbol) is accessed.
  #
  #    autoload(:MyModule, "/usr/local/lib/modules/my_module.rb")
  def self.autoload(module1, filename) end

  # Returns _filename_ to be loaded if _name_ is registered as
  # +autoload+.
  #
  #    autoload(:B, "b")
  #    autoload?(:B)            #=> "b"
  def self.autoload?(name) end

  # Returns a +Binding+ object, describing the variable and
  # method bindings at the point of call. This object can be used when
  # calling +eval+ to execute the evaluated command in this
  # environment. See also the description of class +Binding+.
  #
  #    def get_binding(param)
  #      return binding
  #    end
  #    b = get_binding("hello")
  #    eval("param", b)   #=> "hello"
  def self.binding; end

  # Returns <code>true</code> if <code>yield</code> would execute a
  # block in the current context. The <code>iterator?</code> form
  # is mildly deprecated.
  #
  #    def try
  #      if block_given?
  #        yield
  #      else
  #        "no block"
  #      end
  #    end
  #    try                  #=> "no block"
  #    try { "hello" }      #=> "hello"
  #    try do "hello" end   #=> "hello"
  def self.block_given?; end

  # Generates a Continuation object, which it passes to
  # the associated block. You need to <code>require
  # 'continuation'</code> before using this method. Performing a
  # <em>cont</em><code>.call</code> will cause the #callcc
  # to return (as will falling through the end of the block). The
  # value returned by the #callcc is the value of the
  # block, or the value passed to <em>cont</em><code>.call</code>. See
  # class Continuation for more details. Also see
  # Kernel#throw for an alternative mechanism for
  # unwinding a call stack.
  def self.callcc; end

  # Returns the current execution stack---an array containing strings in
  # the form <code>file:line</code> or <code>file:line: in
  # `method'</code>.
  #
  # The optional _start_ parameter determines the number of initial stack
  # entries to omit from the top of the stack.
  #
  # A second optional +length+ parameter can be used to limit how many entries
  # are returned from the stack.
  #
  # Returns +nil+ if _start_ is greater than the size of
  # current execution stack.
  #
  # Optionally you can pass a range, which will return an array containing the
  # entries within the specified range.
  #
  #    def a(skip)
  #      caller(skip)
  #    end
  #    def b(skip)
  #      a(skip)
  #    end
  #    def c(skip)
  #      b(skip)
  #    end
  #    c(0)   #=> ["prog:2:in `a'", "prog:5:in `b'", "prog:8:in `c'", "prog:10:in `<main>'"]
  #    c(1)   #=> ["prog:5:in `b'", "prog:8:in `c'", "prog:11:in `<main>'"]
  #    c(2)   #=> ["prog:8:in `c'", "prog:12:in `<main>'"]
  #    c(3)   #=> ["prog:13:in `<main>'"]
  #    c(4)   #=> []
  #    c(5)   #=> nil
  def self.caller(*several_variants) end

  # Returns the current execution stack---an array containing
  # backtrace location objects.
  #
  # See Thread::Backtrace::Location for more information.
  #
  # The optional _start_ parameter determines the number of initial stack
  # entries to omit from the top of the stack.
  #
  # A second optional +length+ parameter can be used to limit how many entries
  # are returned from the stack.
  #
  # Returns +nil+ if _start_ is greater than the size of
  # current execution stack.
  #
  # Optionally you can pass a range, which will return an array containing the
  # entries within the specified range.
  def self.caller_locations(*several_variants) end

  # +catch+ executes its block. If a +throw+ is
  # executed, Ruby searches up its stack for a +catch+ block
  # with a tag corresponding to the +throw+'s
  # _tag_. If found, that block is terminated, and
  # +catch+ returns the value given to +throw+. If
  # +throw+ is not called, the block terminates normally, and
  # the value of +catch+ is the value of the last expression
  # evaluated. +catch+ expressions may be nested, and the
  # +throw+ call need not be in lexical scope.
  #
  #    def routine(n)
  #      puts n
  #      throw :done if n <= 0
  #      routine(n-1)
  #    end
  #
  #    catch(:done) { routine(3) }
  #
  # <em>produces:</em>
  #
  #    3
  #    2
  #    1
  #    0
  #
  # when _arg_ is given, +catch+ yields it as is, or when no
  # _arg_ is given, +catch+ assigns a new unique object to
  # +throw+.  this is useful for nested +catch+.  _arg_ can
  # be an arbitrary object, not only Symbol.
  def self.catch(*arg) end

  # Equivalent to <code>$_ = $_.chomp(<em>string</em>)</code>. See
  # <code>String#chomp</code>.
  # Available only when -p/-n command line option specified.
  def self.chomp(*several_variants) end

  # Equivalent to <code>($_.dup).chop!</code>, except <code>nil</code>
  # is never returned. See <code>String#chop!</code>.
  # Available only when -p/-n command line option specified.
  def self.chop; end

  # Evaluates the Ruby expression(s) in <em>string</em>. If
  # <em>binding</em> is given, which must be a <code>Binding</code>
  # object, the evaluation is performed in its context. If the
  # optional <em>filename</em> and <em>lineno</em> parameters are
  # present, they will be used when reporting syntax errors.
  #
  #    def get_binding(str)
  #      return binding
  #    end
  #    str = "hello"
  #    eval "str + ' Fred'"                      #=> "hello Fred"
  #    eval "str + ' Fred'", get_binding("bye")  #=> "bye Fred"
  def self.eval(string, *binding_filename_lineno) end

  # Replaces the current process by running the given external _command_, which
  # can take one of the following forms:
  #
  # [<code>exec(commandline)</code>]
  #     command line string which is passed to the standard shell
  # [<code>exec(cmdname, arg1, ...)</code>]
  #     command name and one or more arguments (no shell)
  # [<code>exec([cmdname, argv0], arg1, ...)</code>]
  #     command name, argv[0] and zero or more arguments (no shell)
  #
  # In the first form, the string is taken as a command line that is subject to
  # shell expansion before being executed.
  #
  # The standard shell always means <code>"/bin/sh"</code> on Unix-like systems,
  # same as <code>ENV["RUBYSHELL"]</code>
  # (or <code>ENV["COMSPEC"]</code> on Windows NT series), and similar.
  #
  # If the string from the first form (<code>exec("command")</code>) follows
  # these simple rules:
  #
  # * no meta characters
  # * no shell reserved word and no special built-in
  # * Ruby invokes the command directly without shell
  #
  # You can force shell invocation by adding ";" to the string (because ";" is
  # a meta character).
  #
  # Note that this behavior is observable by pid obtained
  # (return value of spawn() and IO#pid for IO.popen) is the pid of the invoked
  # command, not shell.
  #
  # In the second form (<code>exec("command1", "arg1", ...)</code>), the first
  # is taken as a command name and the rest are passed as parameters to command
  # with no shell expansion.
  #
  # In the third form (<code>exec(["command", "argv0"], "arg1", ...)</code>),
  # starting a two-element array at the beginning of the command, the first
  # element is the command to be executed, and the second argument is used as
  # the <code>argv[0]</code> value, which may show up in process listings.
  #
  # In order to execute the command, one of the <code>exec(2)</code> system
  # calls are used, so the running command may inherit some of the environment
  # of the original program (including open file descriptors).
  #
  # This behavior is modified by the given +env+ and +options+ parameters. See
  # ::spawn for details.
  #
  # If the command fails to execute (typically <code>Errno::ENOENT</code> when
  # it was not found) a SystemCallError exception is raised.
  #
  # This method modifies process attributes according to given +options+ before
  # <code>exec(2)</code> system call. See ::spawn for more details about the
  # given +options+.
  #
  # The modified attributes may be retained when <code>exec(2)</code> system
  # call fails.
  #
  # For example, hard resource limits are not restorable.
  #
  # Consider to create a child process using ::spawn or Kernel#system if this
  # is not acceptable.
  #
  #    exec "echo *"       # echoes list of files in current directory
  #    # never get here
  #
  #    exec "echo", "*"    # echoes an asterisk
  #    # never get here
  def self.exec(*args) end

  # Initiates the termination of the Ruby script by raising the
  # <code>SystemExit</code> exception. This exception may be caught. The
  # optional parameter is used to return a status code to the invoking
  # environment.
  # +true+ and +FALSE+ of _status_ means success and failure
  # respectively.  The interpretation of other integer values are
  # system dependent.
  #
  #    begin
  #      exit
  #      puts "never get here"
  #    rescue SystemExit
  #      puts "rescued a SystemExit exception"
  #    end
  #    puts "after begin block"
  #
  # <em>produces:</em>
  #
  #    rescued a SystemExit exception
  #    after begin block
  #
  # Just prior to termination, Ruby executes any <code>at_exit</code> functions
  # (see Kernel::at_exit) and runs any object finalizers (see
  # ObjectSpace::define_finalizer).
  #
  #    at_exit { puts "at_exit function" }
  #    ObjectSpace.define_finalizer("string",  proc { puts "in finalizer" })
  #    exit
  #
  # <em>produces:</em>
  #
  #    at_exit function
  #    in finalizer
  def self.exit(status = true) end

  # Exits the process immediately. No exit handlers are
  # run. <em>status</em> is returned to the underlying system as the
  # exit status.
  #
  #    Process.exit!(true)
  def self.exit!(status = false) end

  # With no arguments, raises the exception in <code>$!</code> or raises
  # a <code>RuntimeError</code> if <code>$!</code> is +nil+.
  # With a single +String+ argument, raises a
  # +RuntimeError+ with the string as a message. Otherwise,
  # the first parameter should be the name of an +Exception+
  # class (or an object that returns an +Exception+ object when sent
  # an +exception+ message). The optional second parameter sets the
  # message associated with the exception, and the third parameter is an
  # array of callback information. Exceptions are caught by the
  # +rescue+ clause of <code>begin...end</code> blocks.
  #
  #    raise "Failed to create socket"
  #    raise ArgumentError, "No parameters", caller
  def self.fail(*several_variants) end

  # Creates a subprocess. If a block is specified, that block is run
  # in the subprocess, and the subprocess terminates with a status of
  # zero. Otherwise, the +fork+ call returns twice, once in
  # the parent, returning the process ID of the child, and once in
  # the child, returning _nil_. The child process can exit using
  # <code>Kernel.exit!</code> to avoid running any
  # <code>at_exit</code> functions. The parent process should
  # use <code>Process.wait</code> to collect the termination statuses
  # of its children or use <code>Process.detach</code> to register
  # disinterest in their status; otherwise, the operating system
  # may accumulate zombie processes.
  #
  # The thread calling fork is the only thread in the created child process.
  # fork doesn't copy other threads.
  #
  # If fork is not usable, Process.respond_to?(:fork) returns false.
  #
  # Note that fork(2) is not avaiable on some platforms like Windows and NetBSD 4.
  # Therefore you should use spawn() instead of fork().
  def self.fork; end

  # Returns the string resulting from applying <i>format_string</i> to
  # any additional arguments.  Within the format string, any characters
  # other than format sequences are copied to the result.
  #
  # The syntax of a format sequence is follows.
  #
  #   %[flags][width][.precision]type
  #
  # A format
  # sequence consists of a percent sign, followed by optional flags,
  # width, and precision indicators, then terminated with a field type
  # character.  The field type controls how the corresponding
  # <code>sprintf</code> argument is to be interpreted, while the flags
  # modify that interpretation.
  #
  # The field type characters are:
  #
  #     Field |  Integer Format
  #     ------+--------------------------------------------------------------
  #       b   | Convert argument as a binary number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..1'.
  #       B   | Equivalent to `b', but uses an uppercase 0B for prefix
  #           | in the alternative format by #.
  #       d   | Convert argument as a decimal number.
  #       i   | Identical to `d'.
  #       o   | Convert argument as an octal number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..7'.
  #       u   | Identical to `d'.
  #       x   | Convert argument as a hexadecimal number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..f' (representing an infinite string of
  #           | leading 'ff's).
  #       X   | Equivalent to `x', but uses uppercase letters.
  #
  #     Field |  Float Format
  #     ------+--------------------------------------------------------------
  #       e   | Convert floating point argument into exponential notation
  #           | with one digit before the decimal point as [-]d.dddddde[+-]dd.
  #           | The precision specifies the number of digits after the decimal
  #           | point (defaulting to six).
  #       E   | Equivalent to `e', but uses an uppercase E to indicate
  #           | the exponent.
  #       f   | Convert floating point argument as [-]ddd.dddddd,
  #           | where the precision specifies the number of digits after
  #           | the decimal point.
  #       g   | Convert a floating point number using exponential form
  #           | if the exponent is less than -4 or greater than or
  #           | equal to the precision, or in dd.dddd form otherwise.
  #           | The precision specifies the number of significant digits.
  #       G   | Equivalent to `g', but use an uppercase `E' in exponent form.
  #       a   | Convert floating point argument as [-]0xh.hhhhp[+-]dd,
  #           | which is consisted from optional sign, "0x", fraction part
  #           | as hexadecimal, "p", and exponential part as decimal.
  #       A   | Equivalent to `a', but use uppercase `X' and `P'.
  #
  #     Field |  Other Format
  #     ------+--------------------------------------------------------------
  #       c   | Argument is the numeric code for a single character or
  #           | a single character string itself.
  #       p   | The valuing of argument.inspect.
  #       s   | Argument is a string to be substituted.  If the format
  #           | sequence contains a precision, at most that many characters
  #           | will be copied.
  #       %   | A percent sign itself will be displayed.  No argument taken.
  #
  # The flags modifies the behavior of the formats.
  # The flag characters are:
  #
  #   Flag     | Applies to    | Meaning
  #   ---------+---------------+-----------------------------------------
  #   space    | bBdiouxX      | Leave a space at the start of
  #            | aAeEfgG       | non-negative numbers.
  #            | (numeric fmt) | For `o', `x', `X', `b' and `B', use
  #            |               | a minus sign with absolute value for
  #            |               | negative values.
  #   ---------+---------------+-----------------------------------------
  #   (digit)$ | all           | Specifies the absolute argument number
  #            |               | for this field.  Absolute and relative
  #            |               | argument numbers cannot be mixed in a
  #            |               | sprintf string.
  #   ---------+---------------+-----------------------------------------
  #    #       | bBoxX         | Use an alternative format.
  #            | aAeEfgG       | For the conversions `o', increase the precision
  #            |               | until the first digit will be `0' if
  #            |               | it is not formatted as complements.
  #            |               | For the conversions `x', `X', `b' and `B'
  #            |               | on non-zero, prefix the result with ``0x'',
  #            |               | ``0X'', ``0b'' and ``0B'', respectively.
  #            |               | For `a', `A', `e', `E', `f', `g', and 'G',
  #            |               | force a decimal point to be added,
  #            |               | even if no digits follow.
  #            |               | For `g' and 'G', do not remove trailing zeros.
  #   ---------+---------------+-----------------------------------------
  #   +        | bBdiouxX      | Add a leading plus sign to non-negative
  #            | aAeEfgG       | numbers.
  #            | (numeric fmt) | For `o', `x', `X', `b' and `B', use
  #            |               | a minus sign with absolute value for
  #            |               | negative values.
  #   ---------+---------------+-----------------------------------------
  #   -        | all           | Left-justify the result of this conversion.
  #   ---------+---------------+-----------------------------------------
  #   0 (zero) | bBdiouxX      | Pad with zeros, not spaces.
  #            | aAeEfgG       | For `o', `x', `X', `b' and `B', radix-1
  #            | (numeric fmt) | is used for negative numbers formatted as
  #            |               | complements.
  #   ---------+---------------+-----------------------------------------
  #   *        | all           | Use the next argument as the field width.
  #            |               | If negative, left-justify the result. If the
  #            |               | asterisk is followed by a number and a dollar
  #            |               | sign, use the indicated argument as the width.
  #
  # Examples of flags:
  #
  #  # `+' and space flag specifies the sign of non-negative numbers.
  #  sprintf("%d", 123)  #=> "123"
  #  sprintf("%+d", 123) #=> "+123"
  #  sprintf("% d", 123) #=> " 123"
  #
  #  # `#' flag for `o' increases number of digits to show `0'.
  #  # `+' and space flag changes format of negative numbers.
  #  sprintf("%o", 123)   #=> "173"
  #  sprintf("%#o", 123)  #=> "0173"
  #  sprintf("%+o", -123) #=> "-173"
  #  sprintf("%o", -123)  #=> "..7605"
  #  sprintf("%#o", -123) #=> "..7605"
  #
  #  # `#' flag for `x' add a prefix `0x' for non-zero numbers.
  #  # `+' and space flag disables complements for negative numbers.
  #  sprintf("%x", 123)   #=> "7b"
  #  sprintf("%#x", 123)  #=> "0x7b"
  #  sprintf("%+x", -123) #=> "-7b"
  #  sprintf("%x", -123)  #=> "..f85"
  #  sprintf("%#x", -123) #=> "0x..f85"
  #  sprintf("%#x", 0)    #=> "0"
  #
  #  # `#' for `X' uses the prefix `0X'.
  #  sprintf("%X", 123)  #=> "7B"
  #  sprintf("%#X", 123) #=> "0X7B"
  #
  #  # `#' flag for `b' add a prefix `0b' for non-zero numbers.
  #  # `+' and space flag disables complements for negative numbers.
  #  sprintf("%b", 123)   #=> "1111011"
  #  sprintf("%#b", 123)  #=> "0b1111011"
  #  sprintf("%+b", -123) #=> "-1111011"
  #  sprintf("%b", -123)  #=> "..10000101"
  #  sprintf("%#b", -123) #=> "0b..10000101"
  #  sprintf("%#b", 0)    #=> "0"
  #
  #  # `#' for `B' uses the prefix `0B'.
  #  sprintf("%B", 123)  #=> "1111011"
  #  sprintf("%#B", 123) #=> "0B1111011"
  #
  #  # `#' for `e' forces to show the decimal point.
  #  sprintf("%.0e", 1)  #=> "1e+00"
  #  sprintf("%#.0e", 1) #=> "1.e+00"
  #
  #  # `#' for `f' forces to show the decimal point.
  #  sprintf("%.0f", 1234)  #=> "1234"
  #  sprintf("%#.0f", 1234) #=> "1234."
  #
  #  # `#' for `g' forces to show the decimal point.
  #  # It also disables stripping lowest zeros.
  #  sprintf("%g", 123.4)   #=> "123.4"
  #  sprintf("%#g", 123.4)  #=> "123.400"
  #  sprintf("%g", 123456)  #=> "123456"
  #  sprintf("%#g", 123456) #=> "123456."
  #
  # The field width is an optional integer, followed optionally by a
  # period and a precision.  The width specifies the minimum number of
  # characters that will be written to the result for this field.
  #
  # Examples of width:
  #
  #  # padding is done by spaces,       width=20
  #  # 0 or radix-1.             <------------------>
  #  sprintf("%20d", 123)   #=> "                 123"
  #  sprintf("%+20d", 123)  #=> "                +123"
  #  sprintf("%020d", 123)  #=> "00000000000000000123"
  #  sprintf("%+020d", 123) #=> "+0000000000000000123"
  #  sprintf("% 020d", 123) #=> " 0000000000000000123"
  #  sprintf("%-20d", 123)  #=> "123                 "
  #  sprintf("%-+20d", 123) #=> "+123                "
  #  sprintf("%- 20d", 123) #=> " 123                "
  #  sprintf("%020x", -123) #=> "..ffffffffffffffff85"
  #
  # For
  # numeric fields, the precision controls the number of decimal places
  # displayed.  For string fields, the precision determines the maximum
  # number of characters to be copied from the string.  (Thus, the format
  # sequence <code>%10.10s</code> will always contribute exactly ten
  # characters to the result.)
  #
  # Examples of precisions:
  #
  #  # precision for `d', 'o', 'x' and 'b' is
  #  # minimum number of digits               <------>
  #  sprintf("%20.8d", 123)  #=> "            00000123"
  #  sprintf("%20.8o", 123)  #=> "            00000173"
  #  sprintf("%20.8x", 123)  #=> "            0000007b"
  #  sprintf("%20.8b", 123)  #=> "            01111011"
  #  sprintf("%20.8d", -123) #=> "           -00000123"
  #  sprintf("%20.8o", -123) #=> "            ..777605"
  #  sprintf("%20.8x", -123) #=> "            ..ffff85"
  #  sprintf("%20.8b", -11)  #=> "            ..110101"
  #
  #  # "0x" and "0b" for `#x' and `#b' is not counted for
  #  # precision but "0" for `#o' is counted.  <------>
  #  sprintf("%#20.8d", 123)  #=> "            00000123"
  #  sprintf("%#20.8o", 123)  #=> "            00000173"
  #  sprintf("%#20.8x", 123)  #=> "          0x0000007b"
  #  sprintf("%#20.8b", 123)  #=> "          0b01111011"
  #  sprintf("%#20.8d", -123) #=> "           -00000123"
  #  sprintf("%#20.8o", -123) #=> "            ..777605"
  #  sprintf("%#20.8x", -123) #=> "          0x..ffff85"
  #  sprintf("%#20.8b", -11)  #=> "          0b..110101"
  #
  #  # precision for `e' is number of
  #  # digits after the decimal point           <------>
  #  sprintf("%20.8e", 1234.56789) #=> "      1.23456789e+03"
  #
  #  # precision for `f' is number of
  #  # digits after the decimal point               <------>
  #  sprintf("%20.8f", 1234.56789) #=> "       1234.56789000"
  #
  #  # precision for `g' is number of
  #  # significant digits                          <------->
  #  sprintf("%20.8g", 1234.56789) #=> "           1234.5679"
  #
  #  #                                         <------->
  #  sprintf("%20.8g", 123456789)  #=> "       1.2345679e+08"
  #
  #  # precision for `s' is
  #  # maximum number of characters                    <------>
  #  sprintf("%20.8s", "string test") #=> "            string t"
  #
  # Examples:
  #
  #    sprintf("%d %04x", 123, 123)               #=> "123 007b"
  #    sprintf("%08b '%4s'", 123, 123)            #=> "01111011 ' 123'"
  #    sprintf("%1$*2$s %2$d %1$s", "hello", 8)   #=> "   hello 8 hello"
  #    sprintf("%1$*2$s %2$d", "hello", -8)       #=> "hello    -8"
  #    sprintf("%+g:% g:%-g", 1.23, 1.23, 1.23)   #=> "+1.23: 1.23:1.23"
  #    sprintf("%u", -123)                        #=> "-123"
  #
  # For more complex formatting, Ruby supports a reference by name.
  # %<name>s style uses format style, but %{name} style doesn't.
  #
  # Examples:
  #   sprintf("%<foo>d : %<bar>f", { :foo => 1, :bar => 2 })
  #     #=> 1 : 2.000000
  #   sprintf("%{foo}f", { :foo => 1 })
  #     # => "1f"
  def self.format(format_string, *args) end

  # Returns (and assigns to <code>$_</code>) the next line from the list
  # of files in +ARGV+ (or <code>$*</code>), or from standard input if
  # no files are present on the command line. Returns +nil+ at end of
  # file. The optional argument specifies the record separator. The
  # separator is included with the contents of each record. A separator
  # of +nil+ reads the entire contents, and a zero-length separator
  # reads the input one paragraph at a time, where paragraphs are
  # divided by two consecutive newlines.  If the first argument is an
  # integer, or optional second argument is given, the returning string
  # would not be longer than the given value in bytes.  If multiple
  # filenames are present in +ARGV+, +gets(nil)+ will read the contents
  # one file at a time.
  #
  #    ARGV << "testfile"
  #    print while gets
  #
  # <em>produces:</em>
  #
  #    This is line one
  #    This is line two
  #    This is line three
  #    And so on...
  #
  # The style of programming using <code>$_</code> as an implicit
  # parameter is gradually losing favor in the Ruby community.
  def self.gets(*several_variants) end

  # Returns an array of the names of global variables.
  #
  #    global_variables.grep /std/   #=> [:$stdin, :$stdout, :$stderr]
  def self.global_variables; end

  # Equivalent to <code>$_.gsub...</code>, except that <code>$_</code>
  # will be updated if substitution occurs.
  # Available only when -p/-n command line option specified.
  def self.gsub(*several_variants) end

  # Returns <code>true</code> if <code>yield</code> would execute a
  # block in the current context. The <code>iterator?</code> form
  # is mildly deprecated.
  #
  #    def try
  #      if block_given?
  #        yield
  #      else
  #        "no block"
  #      end
  #    end
  #    try                  #=> "no block"
  #    try { "hello" }      #=> "hello"
  #    try do "hello" end   #=> "hello"
  def self.iterator?; end

  # Equivalent to <code>Proc.new</code>, except the resulting Proc objects
  # check the number of parameters passed when called.
  def self.lambda; end

  # Loads and executes the Ruby
  # program in the file _filename_. If the filename does not
  # resolve to an absolute path, the file is searched for in the library
  # directories listed in <code>$:</code>. If the optional _wrap_
  # parameter is +true+, the loaded script will be executed
  # under an anonymous module, protecting the calling program's global
  # namespace. In no circumstance will any local variables in the loaded
  # file be propagated to the loading environment.
  def self.load(filename, wrap = false) end

  # Returns the names of the current local variables.
  #
  #    fred = 1
  #    for i in 1..10
  #       # ...
  #    end
  #    local_variables   #=> [:fred, :i]
  def self.local_variables; end

  # Repeatedly executes the block.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    loop do
  #      print "Input: "
  #      line = gets
  #      break if !line or line =~ /^qQ/
  #      # ...
  #    end
  #
  # StopIteration raised in the block breaks the loop.
  def self.loop; end

  # Creates an IO object connected to the given stream, file, or subprocess.
  #
  # If +path+ does not start with a pipe character (<code>|</code>), treat it
  # as the name of a file to open using the specified mode (defaulting to
  # "r").
  #
  # The +mode+ is either a string or an integer.  If it is an integer, it
  # must be bitwise-or of open(2) flags, such as File::RDWR or File::EXCL.  If
  # it is a string, it is either "fmode", "fmode:ext_enc", or
  # "fmode:ext_enc:int_enc".
  #
  # See the documentation of IO.new for full documentation of the +mode+ string
  # directives.
  #
  # If a file is being created, its initial permissions may be set using the
  # +perm+ parameter.  See File.new and the open(2) and chmod(2) man pages for
  # a description of permissions.
  #
  # If a block is specified, it will be invoked with the IO object as a
  # parameter, and the IO will be automatically closed when the block
  # terminates.  The call returns the value of the block.
  #
  # If +path+ starts with a pipe character (<code>"|"</code>), a subprocess is
  # created, connected to the caller by a pair of pipes.  The returned IO
  # object may be used to write to the standard input and read from the
  # standard output of this subprocess.
  #
  # If the command following the pipe is a single minus sign
  # (<code>"|-"</code>), Ruby forks, and this subprocess is connected to the
  # parent.  If the command is not <code>"-"</code>, the subprocess runs the
  # command.
  #
  # When the subprocess is ruby (opened via <code>"|-"</code>), the +open+
  # call returns +nil+.  If a block is associated with the open call, that
  # block will run twice --- once in the parent and once in the child.
  #
  # The block parameter will be an IO object in the parent and +nil+ in the
  # child. The parent's +IO+ object will be connected to the child's $stdin
  # and $stdout.  The subprocess will be terminated at the end of the block.
  #
  # === Examples
  #
  # Reading from "testfile":
  #
  #    open("testfile") do |f|
  #      print f.gets
  #    end
  #
  # Produces:
  #
  #    This is line one
  #
  # Open a subprocess and read its output:
  #
  #    cmd = open("|date")
  #    print cmd.gets
  #    cmd.close
  #
  # Produces:
  #
  #    Wed Apr  9 08:56:31 CDT 2003
  #
  # Open a subprocess running the same Ruby program:
  #
  #    f = open("|-", "w+")
  #    if f == nil
  #      puts "in Child"
  #      exit
  #    else
  #      puts "Got: #{f.gets}"
  #    end
  #
  # Produces:
  #
  #    Got: in Child
  #
  # Open a subprocess using a block to receive the IO object:
  #
  #    open "|-" do |f|
  #      if f then
  #        # parent process
  #        puts "Got: #{f.gets}"
  #      else
  #        # child process
  #        puts "in Child"
  #      end
  #    end
  #
  # Produces:
  #
  #    Got: in Child
  def self.open(*args) end

  # For each object, directly writes _obj_.+inspect+ followed by a
  # newline to the program's standard output.
  #
  #    S = Struct.new(:name, :state)
  #    s = S['dave', 'TX']
  #    p s
  #
  # <em>produces:</em>
  #
  #    #<S name="dave", state="TX">
  def self.p(*several_variants) end

  # Prints each object in turn to <code>$stdout</code>. If the output
  # field separator (<code>$,</code>) is not +nil+, its
  # contents will appear between each field. If the output record
  # separator (<code>$\\</code>) is not +nil+, it will be
  # appended to the output. If no arguments are given, prints
  # <code>$_</code>. Objects that aren't strings will be converted by
  # calling their <code>to_s</code> method.
  #
  #    print "cat", [1,2,3], 99, "\n"
  #    $, = ", "
  #    $\ = "\n"
  #    print "cat", [1,2,3], 99
  #
  # <em>produces:</em>
  #
  #    cat12399
  #    cat, 1, 2, 3, 99
  def self.print(obj, *args) end

  # Equivalent to:
  #    io.write(sprintf(string, obj, ...))
  # or
  #    $stdout.write(sprintf(string, obj, ...))
  def self.printf(*several_variants) end

  # Equivalent to <code>Proc.new</code>.
  def self.proc; end

  #  Equivalent to:
  #
  #    $stdout.putc(int)
  #
  # Refer to the documentation for IO#putc for important information regarding
  # multi-byte characters.
  def self.putc(int) end

  # Equivalent to
  #
  #     $stdout.puts(obj, ...)
  def self.puts(obj = '', *arg) end

  # With no arguments, raises the exception in <code>$!</code> or raises
  # a <code>RuntimeError</code> if <code>$!</code> is +nil+.
  # With a single +String+ argument, raises a
  # +RuntimeError+ with the string as a message. Otherwise,
  # the first parameter should be the name of an +Exception+
  # class (or an object that returns an +Exception+ object when sent
  # an +exception+ message). The optional second parameter sets the
  # message associated with the exception, and the third parameter is an
  # array of callback information. Exceptions are caught by the
  # +rescue+ clause of <code>begin...end</code> blocks.
  #
  #    raise "Failed to create socket"
  #    raise ArgumentError, "No parameters", caller
  def self.raise(*several_variants) end

  # If called without an argument, or if <tt>max.to_i.abs == 0</tt>, rand
  # returns a pseudo-random floating point number between 0.0 and 1.0,
  # including 0.0 and excluding 1.0.
  #
  #   rand        #=> 0.2725926052826416
  #
  # When +max.abs+ is greater than or equal to 1, +rand+ returns a pseudo-random
  # integer greater than or equal to 0 and less than +max.to_i.abs+.
  #
  #   rand(100)   #=> 12
  #
  # When +max+ is a Range, +rand+ returns a random number where
  # range.member?(number) == true.
  #
  # Negative or floating point values for +max+ are allowed, but may give
  # surprising results.
  #
  #   rand(-100) # => 87
  #   rand(-0.5) # => 0.8130921818028143
  #   rand(1.9)  # equivalent to rand(1), which is always 0
  #
  # Kernel.srand may be used to ensure that sequences of random numbers are
  # reproducible between different runs of a program.
  #
  # See also Random.rand.
  def self.rand(max = 0) end

  # Equivalent to <code>Kernel::gets</code>, except
  # +readline+ raises +EOFError+ at end of file.
  def self.readline(*several_variants) end

  # Returns an array containing the lines returned by calling
  # <code>Kernel.gets(<i>sep</i>)</code> until the end of file.
  def self.readlines(*several_variants) end

  # Loads the given +name+, returning +true+ if successful and +false+ if the
  # feature is already loaded.
  #
  # If the filename does not resolve to an absolute path, it will be searched
  # for in the directories listed in <code>$LOAD_PATH</code> (<code>$:</code>).
  #
  # If the filename has the extension ".rb", it is loaded as a source file; if
  # the extension is ".so", ".o", or ".dll", or the default shared library
  # extension on the current platform, Ruby loads the shared library as a
  # Ruby extension.  Otherwise, Ruby tries adding ".rb", ".so", and so on
  # to the name until found.  If the file named cannot be found, a LoadError
  # will be raised.
  #
  # For Ruby extensions the filename given may use any shared library
  # extension.  For example, on Linux the socket extension is "socket.so" and
  # <code>require 'socket.dll'</code> will load the socket extension.
  #
  # The absolute path of the loaded file is added to
  # <code>$LOADED_FEATURES</code> (<code>$"</code>).  A file will not be
  # loaded again if its path already appears in <code>$"</code>.  For example,
  # <code>require 'a'; require './a'</code> will not load <code>a.rb</code>
  # again.
  #
  #   require "my-library.rb"
  #   require "db-driver"
  #
  # Any constants or globals within the loaded source file will be available
  # in the calling program's global namespace. However, local variables will
  # not be propagated to the loading environment.
  def self.require(name) end

  # Ruby tries to load the library named _string_ relative to the requiring
  # file's path.  If the file's path cannot be determined a LoadError is raised.
  # If a file is loaded +true+ is returned and false otherwise.
  def self.require_relative(string) end

  # Calls select(2) system call.
  # It monitors given arrays of <code>IO</code> objects, waits one or more
  # of <code>IO</code> objects ready for reading, are ready for writing,
  # and have pending exceptions respectively, and returns an array that
  # contains arrays of those IO objects.  It will return <code>nil</code>
  # if optional <i>timeout</i> value is given and no <code>IO</code> object
  # is ready in <i>timeout</i> seconds.
  #
  # <code>IO.select</code> peeks the buffer of <code>IO</code> objects for testing readability.
  # If the <code>IO</code> buffer is not empty,
  # <code>IO.select</code> immediately notify readability.
  # This "peek" is only happen for <code>IO</code> objects.
  # It is not happen for IO-like objects such as OpenSSL::SSL::SSLSocket.
  #
  # The best way to use <code>IO.select</code> is invoking it
  # after nonblocking methods such as <code>read_nonblock</code>, <code>write_nonblock</code>, etc.
  # The methods raises an exception which is extended by
  # <code>IO::WaitReadable</code> or <code>IO::WaitWritable</code>.
  # The modules notify how the caller should wait with <code>IO.select</code>.
  # If <code>IO::WaitReadable</code> is raised, the caller should wait for reading.
  # If <code>IO::WaitWritable</code> is raised, the caller should wait for writing.
  #
  # So, blocking read (<code>readpartial</code>) can be emulated using
  # <code>read_nonblock</code> and <code>IO.select</code> as follows:
  #
  #   begin
  #     result = io_like.read_nonblock(maxlen)
  #   rescue IO::WaitReadable
  #     IO.select([io_like])
  #     retry
  #   rescue IO::WaitWritable
  #     IO.select(nil, [io_like])
  #     retry
  #   end
  #
  # Especially, the combination of nonblocking methods and
  # <code>IO.select</code> is preferred for <code>IO</code> like
  # objects such as <code>OpenSSL::SSL::SSLSocket</code>.
  # It has <code>to_io</code> method to return underlying <code>IO</code> object.
  # <code>IO.select</code> calls <code>to_io</code> to obtain the file descriptor to wait.
  #
  # This means that readability notified by <code>IO.select</code> doesn't mean
  # readability from <code>OpenSSL::SSL::SSLSocket</code> object.
  #
  # Most possible situation is <code>OpenSSL::SSL::SSLSocket</code> buffers some data.
  # <code>IO.select</code> doesn't see the buffer.
  # So <code>IO.select</code> can block when <code>OpenSSL::SSL::SSLSocket#readpartial</code> doesn't block.
  #
  # However several more complicated situation exists.
  #
  # SSL is a protocol which is sequence of records.
  # The record consists multiple bytes.
  # So, the remote side of SSL sends a partial record,
  # <code>IO.select</code> notifies readability but
  # <code>OpenSSL::SSL::SSLSocket</code> cannot decrypt a byte and
  # <code>OpenSSL::SSL::SSLSocket#readpartial</code> will blocks.
  #
  # Also, the remote side can request SSL renegotiation which forces
  # the local SSL engine writes some data.
  # This means <code>OpenSSL::SSL::SSLSocket#readpartial</code> may
  # invoke <code>write</code> system call and it can block.
  # In such situation, <code>OpenSSL::SSL::SSLSocket#read_nonblock</code>
  # raises IO::WaitWritable instead of blocking.
  # So, the caller should wait for ready for writability as above example.
  #
  # The combination of nonblocking methods and <code>IO.select</code> is
  # also useful for streams such as tty, pipe socket socket when
  # multiple process read form a stream.
  #
  # Finally, Linux kernel developers doesn't guarantee that
  # readability of select(2) means readability of following read(2) even
  # for single process.
  # See select(2) manual on GNU/Linux system.
  #
  # Invoking <code>IO.select</code> before <code>IO#readpartial</code> works well in usual.
  # However it is not the best way to use <code>IO.select</code>.
  #
  # The writability notified by select(2) doesn't show
  # how many bytes writable.
  # <code>IO#write</code> method blocks until given whole string is written.
  # So, <code>IO#write(two or more bytes)</code> can block after writability is notified by <code>IO.select</code>.
  # <code>IO#write_nonblock</code> is required to avoid the blocking.
  #
  # Blocking write (<code>write</code>) can be emulated using
  # <code>write_nonblock</code> and <code>IO.select</code> as follows:
  # IO::WaitReadable should also be rescued for SSL renegotiation in <code>OpenSSL::SSL::SSLSocket</code>.
  #
  #   while 0 < string.bytesize
  #     begin
  #       written = io_like.write_nonblock(string)
  #     rescue IO::WaitReadable
  #       IO.select([io_like])
  #       retry
  #     rescue IO::WaitWritable
  #       IO.select(nil, [io_like])
  #       retry
  #     end
  #     string = string.byteslice(written..-1)
  #   end
  #
  # === Parameters
  # read_array:: an array of <code>IO</code> objects that wait until ready for read
  # write_array:: an array of <code>IO</code> objects that wait until ready for write
  # error_array:: an array of <code>IO</code> objects that wait for exceptions
  # timeout:: a numeric value in second
  #
  # === Example
  #
  #     rp, wp = IO.pipe
  #     mesg = "ping "
  #     100.times {
  #       # IO.select follows IO#read.  Not the best way to use IO.select.
  #       rs, ws, = IO.select([rp], [wp])
  #       if r = rs[0]
  #         ret = r.read(5)
  #         print ret
  #         case ret
  #         when /ping/
  #           mesg = "pong\n"
  #         when /pong/
  #           mesg = "ping "
  #         end
  #       end
  #       if w = ws[0]
  #         w.write(mesg)
  #       end
  #     }
  #
  # <em>produces:</em>
  #
  #     ping pong
  #     ping pong
  #     ping pong
  #     (snipped)
  #     ping
  def self.select(p1, p2 = v2, p3 = v3, p4 = v4) end

  # Establishes _proc_ as the handler for tracing, or disables
  # tracing if the parameter is +nil+.
  #
  # *Note:* this method is obsolete, please use TracePoint instead.
  #
  # _proc_ takes up to six parameters:
  #
  # *   an event name
  # *   a filename
  # *   a line number
  # *   an object id
  # *   a binding
  # *   the name of a class
  #
  # _proc_ is invoked whenever an event occurs.
  #
  # Events are:
  #
  # +c-call+:: call a C-language routine
  # +c-return+:: return from a C-language routine
  # +call+:: call a Ruby method
  # +class+:: start a class or module definition),
  # +end+:: finish a class or module definition),
  # +line+:: execute code on a new line
  # +raise+:: raise an exception
  # +return+:: return from a Ruby method
  #
  # Tracing is disabled within the context of _proc_.
  #
  #     class Test
  #     def test
  #       a = 1
  #       b = 2
  #     end
  #     end
  #
  #     set_trace_func proc { |event, file, line, id, binding, classname|
  #        printf "%8s %s:%-2d %10s %8s\n", event, file, line, id, classname
  #     }
  #     t = Test.new
  #     t.test
  #
  #       line prog.rb:11               false
  #     c-call prog.rb:11        new    Class
  #     c-call prog.rb:11 initialize   Object
  #   c-return prog.rb:11 initialize   Object
  #   c-return prog.rb:11        new    Class
  #       line prog.rb:12               false
  #       call prog.rb:2        test     Test
  #       line prog.rb:3        test     Test
  #       line prog.rb:4        test     Test
  #     return prog.rb:4        test     Test
  def self.set_trace_func(*several_variants) end

  # Suspends the current thread for _duration_ seconds (which may be any number,
  # including a +Float+ with fractional seconds). Returns the actual number of
  # seconds slept (rounded), which may be less than that asked for if another
  # thread calls <code>Thread#run</code>. Called without an argument, sleep()
  # will sleep forever.
  #
  #    Time.new    #=> 2008-03-08 19:56:19 +0900
  #    sleep 1.2   #=> 1
  #    Time.new    #=> 2008-03-08 19:56:20 +0900
  #    sleep 1.9   #=> 2
  #    Time.new    #=> 2008-03-08 19:56:22 +0900
  def self.sleep(*duration) end

  # spawn executes specified command and return its pid.
  #
  #   pid = spawn("tar xf ruby-2.0.0-p195.tar.bz2")
  #   Process.wait pid
  #
  #   pid = spawn(RbConfig.ruby, "-eputs'Hello, world!'")
  #   Process.wait pid
  #
  # This method is similar to Kernel#system but it doesn't wait for the command
  # to finish.
  #
  # The parent process should
  # use <code>Process.wait</code> to collect
  # the termination status of its child or
  # use <code>Process.detach</code> to register
  # disinterest in their status;
  # otherwise, the operating system may accumulate zombie processes.
  #
  # spawn has bunch of options to specify process attributes:
  #
  #   env: hash
  #     name => val : set the environment variable
  #     name => nil : unset the environment variable
  #   command...:
  #     commandline                 : command line string which is passed to the standard shell
  #     cmdname, arg1, ...          : command name and one or more arguments (This form does not use the shell. See below for caveats.)
  #     [cmdname, argv0], arg1, ... : command name, argv[0] and zero or more arguments (no shell)
  #   options: hash
  #     clearing environment variables:
  #       :unsetenv_others => true   : clear environment variables except specified by env
  #       :unsetenv_others => false  : don't clear (default)
  #     process group:
  #       :pgroup => true or 0 : make a new process group
  #       :pgroup => pgid      : join to specified process group
  #       :pgroup => nil       : don't change the process group (default)
  #     create new process group: Windows only
  #       :new_pgroup => true  : the new process is the root process of a new process group
  #       :new_pgroup => false : don't create a new process group (default)
  #     resource limit: resourcename is core, cpu, data, etc.  See Process.setrlimit.
  #       :rlimit_resourcename => limit
  #       :rlimit_resourcename => [cur_limit, max_limit]
  #     umask:
  #       :umask => int
  #     redirection:
  #       key:
  #         FD              : single file descriptor in child process
  #         [FD, FD, ...]   : multiple file descriptor in child process
  #       value:
  #         FD                        : redirect to the file descriptor in parent process
  #         string                    : redirect to file with open(string, "r" or "w")
  #         [string]                  : redirect to file with open(string, File::RDONLY)
  #         [string, open_mode]       : redirect to file with open(string, open_mode, 0644)
  #         [string, open_mode, perm] : redirect to file with open(string, open_mode, perm)
  #         [:child, FD]              : redirect to the redirected file descriptor
  #         :close                    : close the file descriptor in child process
  #       FD is one of follows
  #         :in     : the file descriptor 0 which is the standard input
  #         :out    : the file descriptor 1 which is the standard output
  #         :err    : the file descriptor 2 which is the standard error
  #         integer : the file descriptor of specified the integer
  #         io      : the file descriptor specified as io.fileno
  #     file descriptor inheritance: close non-redirected non-standard fds (3, 4, 5, ...) or not
  #       :close_others => true  : don't inherit
  #     current directory:
  #       :chdir => str
  #
  #     The 'cmdname, arg1, ...' form does not use the shell. However,
  #     on different OSes, different things are provided as built-in
  #     commands. An example of this is 'echo', which is a built-in
  #     on Windows, but is a normal program on Linux and Mac OS X.
  #     This means that `Process.spawn 'echo', '%Path%'` will display
  #     the contents of the `%Path%` environment variable on Windows,
  #     but `Process.spawn 'echo', '$PATH'` prints the literal '$PATH'.
  #
  # If a hash is given as +env+, the environment is
  # updated by +env+ before <code>exec(2)</code> in the child process.
  # If a pair in +env+ has nil as the value, the variable is deleted.
  #
  #   # set FOO as BAR and unset BAZ.
  #   pid = spawn({"FOO"=>"BAR", "BAZ"=>nil}, command)
  #
  # If a hash is given as +options+,
  # it specifies
  # process group,
  # create new process group,
  # resource limit,
  # current directory,
  # umask and
  # redirects for the child process.
  # Also, it can be specified to clear environment variables.
  #
  # The <code>:unsetenv_others</code> key in +options+ specifies
  # to clear environment variables, other than specified by +env+.
  #
  #   pid = spawn(command, :unsetenv_others=>true) # no environment variable
  #   pid = spawn({"FOO"=>"BAR"}, command, :unsetenv_others=>true) # FOO only
  #
  # The <code>:pgroup</code> key in +options+ specifies a process group.
  # The corresponding value should be true, zero or positive integer.
  # true and zero means the process should be a process leader of a new
  # process group.
  # Other values specifies a process group to be belongs.
  #
  #   pid = spawn(command, :pgroup=>true) # process leader
  #   pid = spawn(command, :pgroup=>10) # belongs to the process group 10
  #
  # The <code>:new_pgroup</code> key in +options+ specifies to pass
  # +CREATE_NEW_PROCESS_GROUP+ flag to <code>CreateProcessW()</code> that is
  # Windows API. This option is only for Windows.
  # true means the new process is the root process of the new process group.
  # The new process has CTRL+C disabled. This flag is necessary for
  # <code>Process.kill(:SIGINT, pid)</code> on the subprocess.
  # :new_pgroup is false by default.
  #
  #   pid = spawn(command, :new_pgroup=>true)  # new process group
  #   pid = spawn(command, :new_pgroup=>false) # same process group
  #
  # The <code>:rlimit_</code><em>foo</em> key specifies a resource limit.
  # <em>foo</em> should be one of resource types such as <code>core</code>.
  # The corresponding value should be an integer or an array which have one or
  # two integers: same as cur_limit and max_limit arguments for
  # Process.setrlimit.
  #
  #   cur, max = Process.getrlimit(:CORE)
  #   pid = spawn(command, :rlimit_core=>[0,max]) # disable core temporary.
  #   pid = spawn(command, :rlimit_core=>max) # enable core dump
  #   pid = spawn(command, :rlimit_core=>0) # never dump core.
  #
  # The <code>:umask</code> key in +options+ specifies the umask.
  #
  #   pid = spawn(command, :umask=>077)
  #
  # The :in, :out, :err, a fixnum, an IO and an array key specifies a redirection.
  # The redirection maps a file descriptor in the child process.
  #
  # For example, stderr can be merged into stdout as follows:
  #
  #   pid = spawn(command, :err=>:out)
  #   pid = spawn(command, 2=>1)
  #   pid = spawn(command, STDERR=>:out)
  #   pid = spawn(command, STDERR=>STDOUT)
  #
  # The hash keys specifies a file descriptor
  # in the child process started by <code>spawn</code>.
  # :err, 2 and STDERR specifies the standard error stream (stderr).
  #
  # The hash values specifies a file descriptor
  # in the parent process which invokes <code>spawn</code>.
  # :out, 1 and STDOUT specifies the standard output stream (stdout).
  #
  # In the above example,
  # the standard output in the child process is not specified.
  # So it is inherited from the parent process.
  #
  # The standard input stream (stdin) can be specified by :in, 0 and STDIN.
  #
  # A filename can be specified as a hash value.
  #
  #   pid = spawn(command, :in=>"/dev/null") # read mode
  #   pid = spawn(command, :out=>"/dev/null") # write mode
  #   pid = spawn(command, :err=>"log") # write mode
  #   pid = spawn(command, 3=>"/dev/null") # read mode
  #
  # For stdout and stderr,
  # it is opened in write mode.
  # Otherwise read mode is used.
  #
  # For specifying flags and permission of file creation explicitly,
  # an array is used instead.
  #
  #   pid = spawn(command, :in=>["file"]) # read mode is assumed
  #   pid = spawn(command, :in=>["file", "r"])
  #   pid = spawn(command, :out=>["log", "w"]) # 0644 assumed
  #   pid = spawn(command, :out=>["log", "w", 0600])
  #   pid = spawn(command, :out=>["log", File::WRONLY|File::EXCL|File::CREAT, 0600])
  #
  # The array specifies a filename, flags and permission.
  # The flags can be a string or an integer.
  # If the flags is omitted or nil, File::RDONLY is assumed.
  # The permission should be an integer.
  # If the permission is omitted or nil, 0644 is assumed.
  #
  # If an array of IOs and integers are specified as a hash key,
  # all the elements are redirected.
  #
  #   # stdout and stderr is redirected to log file.
  #   # The file "log" is opened just once.
  #   pid = spawn(command, [:out, :err]=>["log", "w"])
  #
  # Another way to merge multiple file descriptors is [:child, fd].
  # \[:child, fd] means the file descriptor in the child process.
  # This is different from fd.
  # For example, :err=>:out means redirecting child stderr to parent stdout.
  # But :err=>[:child, :out] means redirecting child stderr to child stdout.
  # They differ if stdout is redirected in the child process as follows.
  #
  #   # stdout and stderr is redirected to log file.
  #   # The file "log" is opened just once.
  #   pid = spawn(command, :out=>["log", "w"], :err=>[:child, :out])
  #
  # \[:child, :out] can be used to merge stderr into stdout in IO.popen.
  # In this case, IO.popen redirects stdout to a pipe in the child process
  # and [:child, :out] refers the redirected stdout.
  #
  #   io = IO.popen(["sh", "-c", "echo out; echo err >&2", :err=>[:child, :out]])
  #   p io.read #=> "out\nerr\n"
  #
  # The <code>:chdir</code> key in +options+ specifies the current directory.
  #
  #   pid = spawn(command, :chdir=>"/var/tmp")
  #
  # spawn closes all non-standard unspecified descriptors by default.
  # The "standard" descriptors are 0, 1 and 2.
  # This behavior is specified by :close_others option.
  # :close_others doesn't affect the standard descriptors which are
  # closed only if :close is specified explicitly.
  #
  #   pid = spawn(command, :close_others=>true)  # close 3,4,5,... (default)
  #   pid = spawn(command, :close_others=>false) # don't close 3,4,5,...
  #
  # :close_others is true by default for spawn and IO.popen.
  #
  # Note that fds which close-on-exec flag is already set are closed
  # regardless of :close_others option.
  #
  # So IO.pipe and spawn can be used as IO.popen.
  #
  #   # similar to r = IO.popen(command)
  #   r, w = IO.pipe
  #   pid = spawn(command, :out=>w)   # r, w is closed in the child process.
  #   w.close
  #
  # :close is specified as a hash value to close a fd individually.
  #
  #   f = open(foo)
  #   system(command, f=>:close)        # don't inherit f.
  #
  # If a file descriptor need to be inherited,
  # io=>io can be used.
  #
  #   # valgrind has --log-fd option for log destination.
  #   # log_w=>log_w indicates log_w.fileno inherits to child process.
  #   log_r, log_w = IO.pipe
  #   pid = spawn("valgrind", "--log-fd=#{log_w.fileno}", "echo", "a", log_w=>log_w)
  #   log_w.close
  #   p log_r.read
  #
  # It is also possible to exchange file descriptors.
  #
  #   pid = spawn(command, :out=>:err, :err=>:out)
  #
  # The hash keys specify file descriptors in the child process.
  # The hash values specifies file descriptors in the parent process.
  # So the above specifies exchanging stdout and stderr.
  # Internally, +spawn+ uses an extra file descriptor to resolve such cyclic
  # file descriptor mapping.
  #
  # See <code>Kernel.exec</code> for the standard shell.
  def self.spawn(*args) end

  # Returns the string resulting from applying <i>format_string</i> to
  # any additional arguments.  Within the format string, any characters
  # other than format sequences are copied to the result.
  #
  # The syntax of a format sequence is follows.
  #
  #   %[flags][width][.precision]type
  #
  # A format
  # sequence consists of a percent sign, followed by optional flags,
  # width, and precision indicators, then terminated with a field type
  # character.  The field type controls how the corresponding
  # <code>sprintf</code> argument is to be interpreted, while the flags
  # modify that interpretation.
  #
  # The field type characters are:
  #
  #     Field |  Integer Format
  #     ------+--------------------------------------------------------------
  #       b   | Convert argument as a binary number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..1'.
  #       B   | Equivalent to `b', but uses an uppercase 0B for prefix
  #           | in the alternative format by #.
  #       d   | Convert argument as a decimal number.
  #       i   | Identical to `d'.
  #       o   | Convert argument as an octal number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..7'.
  #       u   | Identical to `d'.
  #       x   | Convert argument as a hexadecimal number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..f' (representing an infinite string of
  #           | leading 'ff's).
  #       X   | Equivalent to `x', but uses uppercase letters.
  #
  #     Field |  Float Format
  #     ------+--------------------------------------------------------------
  #       e   | Convert floating point argument into exponential notation
  #           | with one digit before the decimal point as [-]d.dddddde[+-]dd.
  #           | The precision specifies the number of digits after the decimal
  #           | point (defaulting to six).
  #       E   | Equivalent to `e', but uses an uppercase E to indicate
  #           | the exponent.
  #       f   | Convert floating point argument as [-]ddd.dddddd,
  #           | where the precision specifies the number of digits after
  #           | the decimal point.
  #       g   | Convert a floating point number using exponential form
  #           | if the exponent is less than -4 or greater than or
  #           | equal to the precision, or in dd.dddd form otherwise.
  #           | The precision specifies the number of significant digits.
  #       G   | Equivalent to `g', but use an uppercase `E' in exponent form.
  #       a   | Convert floating point argument as [-]0xh.hhhhp[+-]dd,
  #           | which is consisted from optional sign, "0x", fraction part
  #           | as hexadecimal, "p", and exponential part as decimal.
  #       A   | Equivalent to `a', but use uppercase `X' and `P'.
  #
  #     Field |  Other Format
  #     ------+--------------------------------------------------------------
  #       c   | Argument is the numeric code for a single character or
  #           | a single character string itself.
  #       p   | The valuing of argument.inspect.
  #       s   | Argument is a string to be substituted.  If the format
  #           | sequence contains a precision, at most that many characters
  #           | will be copied.
  #       %   | A percent sign itself will be displayed.  No argument taken.
  #
  # The flags modifies the behavior of the formats.
  # The flag characters are:
  #
  #   Flag     | Applies to    | Meaning
  #   ---------+---------------+-----------------------------------------
  #   space    | bBdiouxX      | Leave a space at the start of
  #            | aAeEfgG       | non-negative numbers.
  #            | (numeric fmt) | For `o', `x', `X', `b' and `B', use
  #            |               | a minus sign with absolute value for
  #            |               | negative values.
  #   ---------+---------------+-----------------------------------------
  #   (digit)$ | all           | Specifies the absolute argument number
  #            |               | for this field.  Absolute and relative
  #            |               | argument numbers cannot be mixed in a
  #            |               | sprintf string.
  #   ---------+---------------+-----------------------------------------
  #    #       | bBoxX         | Use an alternative format.
  #            | aAeEfgG       | For the conversions `o', increase the precision
  #            |               | until the first digit will be `0' if
  #            |               | it is not formatted as complements.
  #            |               | For the conversions `x', `X', `b' and `B'
  #            |               | on non-zero, prefix the result with ``0x'',
  #            |               | ``0X'', ``0b'' and ``0B'', respectively.
  #            |               | For `a', `A', `e', `E', `f', `g', and 'G',
  #            |               | force a decimal point to be added,
  #            |               | even if no digits follow.
  #            |               | For `g' and 'G', do not remove trailing zeros.
  #   ---------+---------------+-----------------------------------------
  #   +        | bBdiouxX      | Add a leading plus sign to non-negative
  #            | aAeEfgG       | numbers.
  #            | (numeric fmt) | For `o', `x', `X', `b' and `B', use
  #            |               | a minus sign with absolute value for
  #            |               | negative values.
  #   ---------+---------------+-----------------------------------------
  #   -        | all           | Left-justify the result of this conversion.
  #   ---------+---------------+-----------------------------------------
  #   0 (zero) | bBdiouxX      | Pad with zeros, not spaces.
  #            | aAeEfgG       | For `o', `x', `X', `b' and `B', radix-1
  #            | (numeric fmt) | is used for negative numbers formatted as
  #            |               | complements.
  #   ---------+---------------+-----------------------------------------
  #   *        | all           | Use the next argument as the field width.
  #            |               | If negative, left-justify the result. If the
  #            |               | asterisk is followed by a number and a dollar
  #            |               | sign, use the indicated argument as the width.
  #
  # Examples of flags:
  #
  #  # `+' and space flag specifies the sign of non-negative numbers.
  #  sprintf("%d", 123)  #=> "123"
  #  sprintf("%+d", 123) #=> "+123"
  #  sprintf("% d", 123) #=> " 123"
  #
  #  # `#' flag for `o' increases number of digits to show `0'.
  #  # `+' and space flag changes format of negative numbers.
  #  sprintf("%o", 123)   #=> "173"
  #  sprintf("%#o", 123)  #=> "0173"
  #  sprintf("%+o", -123) #=> "-173"
  #  sprintf("%o", -123)  #=> "..7605"
  #  sprintf("%#o", -123) #=> "..7605"
  #
  #  # `#' flag for `x' add a prefix `0x' for non-zero numbers.
  #  # `+' and space flag disables complements for negative numbers.
  #  sprintf("%x", 123)   #=> "7b"
  #  sprintf("%#x", 123)  #=> "0x7b"
  #  sprintf("%+x", -123) #=> "-7b"
  #  sprintf("%x", -123)  #=> "..f85"
  #  sprintf("%#x", -123) #=> "0x..f85"
  #  sprintf("%#x", 0)    #=> "0"
  #
  #  # `#' for `X' uses the prefix `0X'.
  #  sprintf("%X", 123)  #=> "7B"
  #  sprintf("%#X", 123) #=> "0X7B"
  #
  #  # `#' flag for `b' add a prefix `0b' for non-zero numbers.
  #  # `+' and space flag disables complements for negative numbers.
  #  sprintf("%b", 123)   #=> "1111011"
  #  sprintf("%#b", 123)  #=> "0b1111011"
  #  sprintf("%+b", -123) #=> "-1111011"
  #  sprintf("%b", -123)  #=> "..10000101"
  #  sprintf("%#b", -123) #=> "0b..10000101"
  #  sprintf("%#b", 0)    #=> "0"
  #
  #  # `#' for `B' uses the prefix `0B'.
  #  sprintf("%B", 123)  #=> "1111011"
  #  sprintf("%#B", 123) #=> "0B1111011"
  #
  #  # `#' for `e' forces to show the decimal point.
  #  sprintf("%.0e", 1)  #=> "1e+00"
  #  sprintf("%#.0e", 1) #=> "1.e+00"
  #
  #  # `#' for `f' forces to show the decimal point.
  #  sprintf("%.0f", 1234)  #=> "1234"
  #  sprintf("%#.0f", 1234) #=> "1234."
  #
  #  # `#' for `g' forces to show the decimal point.
  #  # It also disables stripping lowest zeros.
  #  sprintf("%g", 123.4)   #=> "123.4"
  #  sprintf("%#g", 123.4)  #=> "123.400"
  #  sprintf("%g", 123456)  #=> "123456"
  #  sprintf("%#g", 123456) #=> "123456."
  #
  # The field width is an optional integer, followed optionally by a
  # period and a precision.  The width specifies the minimum number of
  # characters that will be written to the result for this field.
  #
  # Examples of width:
  #
  #  # padding is done by spaces,       width=20
  #  # 0 or radix-1.             <------------------>
  #  sprintf("%20d", 123)   #=> "                 123"
  #  sprintf("%+20d", 123)  #=> "                +123"
  #  sprintf("%020d", 123)  #=> "00000000000000000123"
  #  sprintf("%+020d", 123) #=> "+0000000000000000123"
  #  sprintf("% 020d", 123) #=> " 0000000000000000123"
  #  sprintf("%-20d", 123)  #=> "123                 "
  #  sprintf("%-+20d", 123) #=> "+123                "
  #  sprintf("%- 20d", 123) #=> " 123                "
  #  sprintf("%020x", -123) #=> "..ffffffffffffffff85"
  #
  # For
  # numeric fields, the precision controls the number of decimal places
  # displayed.  For string fields, the precision determines the maximum
  # number of characters to be copied from the string.  (Thus, the format
  # sequence <code>%10.10s</code> will always contribute exactly ten
  # characters to the result.)
  #
  # Examples of precisions:
  #
  #  # precision for `d', 'o', 'x' and 'b' is
  #  # minimum number of digits               <------>
  #  sprintf("%20.8d", 123)  #=> "            00000123"
  #  sprintf("%20.8o", 123)  #=> "            00000173"
  #  sprintf("%20.8x", 123)  #=> "            0000007b"
  #  sprintf("%20.8b", 123)  #=> "            01111011"
  #  sprintf("%20.8d", -123) #=> "           -00000123"
  #  sprintf("%20.8o", -123) #=> "            ..777605"
  #  sprintf("%20.8x", -123) #=> "            ..ffff85"
  #  sprintf("%20.8b", -11)  #=> "            ..110101"
  #
  #  # "0x" and "0b" for `#x' and `#b' is not counted for
  #  # precision but "0" for `#o' is counted.  <------>
  #  sprintf("%#20.8d", 123)  #=> "            00000123"
  #  sprintf("%#20.8o", 123)  #=> "            00000173"
  #  sprintf("%#20.8x", 123)  #=> "          0x0000007b"
  #  sprintf("%#20.8b", 123)  #=> "          0b01111011"
  #  sprintf("%#20.8d", -123) #=> "           -00000123"
  #  sprintf("%#20.8o", -123) #=> "            ..777605"
  #  sprintf("%#20.8x", -123) #=> "          0x..ffff85"
  #  sprintf("%#20.8b", -11)  #=> "          0b..110101"
  #
  #  # precision for `e' is number of
  #  # digits after the decimal point           <------>
  #  sprintf("%20.8e", 1234.56789) #=> "      1.23456789e+03"
  #
  #  # precision for `f' is number of
  #  # digits after the decimal point               <------>
  #  sprintf("%20.8f", 1234.56789) #=> "       1234.56789000"
  #
  #  # precision for `g' is number of
  #  # significant digits                          <------->
  #  sprintf("%20.8g", 1234.56789) #=> "           1234.5679"
  #
  #  #                                         <------->
  #  sprintf("%20.8g", 123456789)  #=> "       1.2345679e+08"
  #
  #  # precision for `s' is
  #  # maximum number of characters                    <------>
  #  sprintf("%20.8s", "string test") #=> "            string t"
  #
  # Examples:
  #
  #    sprintf("%d %04x", 123, 123)               #=> "123 007b"
  #    sprintf("%08b '%4s'", 123, 123)            #=> "01111011 ' 123'"
  #    sprintf("%1$*2$s %2$d %1$s", "hello", 8)   #=> "   hello 8 hello"
  #    sprintf("%1$*2$s %2$d", "hello", -8)       #=> "hello    -8"
  #    sprintf("%+g:% g:%-g", 1.23, 1.23, 1.23)   #=> "+1.23: 1.23:1.23"
  #    sprintf("%u", -123)                        #=> "-123"
  #
  # For more complex formatting, Ruby supports a reference by name.
  # %<name>s style uses format style, but %{name} style doesn't.
  #
  # Examples:
  #   sprintf("%<foo>d : %<bar>f", { :foo => 1, :bar => 2 })
  #     #=> 1 : 2.000000
  #   sprintf("%{foo}f", { :foo => 1 })
  #     # => "1f"
  def self.sprintf(format_string, *args) end

  # Seeds the system pseudo-random number generator, Random::DEFAULT, with
  # +number+.  The previous seed value is returned.
  #
  # If +number+ is omitted, seeds the generator using a source of entropy
  # provided by the operating system, if available (/dev/urandom on Unix systems
  # or the RSA cryptographic provider on Windows), which is then combined with
  # the time, the process id, and a sequence number.
  #
  # srand may be used to ensure repeatable sequences of pseudo-random numbers
  # between different runs of the program. By setting the seed to a known value,
  # programs can be made deterministic during testing.
  #
  #   srand 1234               # => 268519324636777531569100071560086917274
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  #   [ rand(10), rand(1000) ] # => [4, 664]
  #   srand 1234               # => 1234
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  def self.srand(number = Random.new_seed) end

  # Equivalent to <code>$_.sub(<i>args</i>)</code>, except that
  # <code>$_</code> will be updated if substitution occurs.
  # Available only when -p/-n command line option specified.
  def self.sub(*several_variants) end

  #  Calls the operating system function identified by _num_ and
  #  returns the result of the function or raises SystemCallError if
  #  it failed.
  #
  #  Arguments for the function can follow _num_. They must be either
  #  +String+ objects or +Integer+ objects. A +String+ object is passed
  #  as a pointer to the byte sequence. An +Integer+ object is passed
  #  as an integer whose bit size is same as a pointer.
  #  Up to nine parameters may be passed (14 on the Atari-ST).
  #
  #  The function identified by _num_ is system
  #  dependent. On some Unix systems, the numbers may be obtained from a
  #  header file called <code>syscall.h</code>.
  #
  #     syscall 4, 1, "hello\n", 6   # '4' is write(2) on our box
  #
  #  <em>produces:</em>
  #
  #     hello
  #
  #  Calling +syscall+ on a platform which does not have any way to
  #  an arbitrary system function just fails with NotImplementedError.
  #
  # Note::
  #   +syscall+ is essentially unsafe and unportable. Feel free to shoot your foot.
  #   DL (Fiddle) library is preferred for safer and a bit more portable programming.
  def self.syscall(*args) end

  # Executes _command..._ in a subshell.
  # _command..._ is one of following forms.
  #
  #   commandline                 : command line string which is passed to the standard shell
  #   cmdname, arg1, ...          : command name and one or more arguments (no shell)
  #   [cmdname, argv0], arg1, ... : command name, argv[0] and zero or more arguments (no shell)
  #
  # system returns +true+ if the command gives zero exit status,
  # +false+ for non zero exit status.
  # Returns +nil+ if command execution fails.
  # An error status is available in <code>$?</code>.
  # The arguments are processed in the same way as
  # for <code>Kernel.spawn</code>.
  #
  # The hash arguments, env and options, are same as
  # <code>exec</code> and <code>spawn</code>.
  # See <code>Kernel.spawn</code> for details.
  #
  #    system("echo *")
  #    system("echo", "*")
  #
  # <em>produces:</em>
  #
  #    config.h main.rb
  #    *
  #
  # See <code>Kernel.exec</code> for the standard shell.
  def self.system(*args) end

  # Uses the integer +cmd+ to perform various tests on +file1+ (first
  # table below) or on +file1+ and +file2+ (second table).
  #
  # File tests on a single file:
  #
  #   Cmd    Returns   Meaning
  #   "A"  | Time    | Last access time for file1
  #   "b"  | boolean | True if file1 is a block device
  #   "c"  | boolean | True if file1 is a character device
  #   "C"  | Time    | Last change time for file1
  #   "d"  | boolean | True if file1 exists and is a directory
  #   "e"  | boolean | True if file1 exists
  #   "f"  | boolean | True if file1 exists and is a regular file
  #   "g"  | boolean | True if file1 has the \CF{setgid} bit
  #        |         | set (false under NT)
  #   "G"  | boolean | True if file1 exists and has a group
  #        |         | ownership equal to the caller's group
  #   "k"  | boolean | True if file1 exists and has the sticky bit set
  #   "l"  | boolean | True if file1 exists and is a symbolic link
  #   "M"  | Time    | Last modification time for file1
  #   "o"  | boolean | True if file1 exists and is owned by
  #        |         | the caller's effective uid
  #   "O"  | boolean | True if file1 exists and is owned by
  #        |         | the caller's real uid
  #   "p"  | boolean | True if file1 exists and is a fifo
  #   "r"  | boolean | True if file1 is readable by the effective
  #        |         | uid/gid of the caller
  #   "R"  | boolean | True if file is readable by the real
  #        |         | uid/gid of the caller
  #   "s"  | int/nil | If file1 has nonzero size, return the size,
  #        |         | otherwise return nil
  #   "S"  | boolean | True if file1 exists and is a socket
  #   "u"  | boolean | True if file1 has the setuid bit set
  #   "w"  | boolean | True if file1 exists and is writable by
  #        |         | the effective uid/gid
  #   "W"  | boolean | True if file1 exists and is writable by
  #        |         | the real uid/gid
  #   "x"  | boolean | True if file1 exists and is executable by
  #        |         | the effective uid/gid
  #   "X"  | boolean | True if file1 exists and is executable by
  #        |         | the real uid/gid
  #   "z"  | boolean | True if file1 exists and has a zero length
  #
  # Tests that take two files:
  #
  #   "-"  | boolean | True if file1 and file2 are identical
  #   "="  | boolean | True if the modification times of file1
  #        |         | and file2 are equal
  #   "<"  | boolean | True if the modification time of file1
  #        |         | is prior to that of file2
  #   ">"  | boolean | True if the modification time of file1
  #        |         | is after that of file2
  def self.test(*args) end

  # Transfers control to the end of the active +catch+ block
  # waiting for _tag_. Raises +ArgumentError+ if there
  # is no +catch+ block for the _tag_. The optional second
  # parameter supplies a return value for the +catch+ block,
  # which otherwise defaults to +nil+. For examples, see
  # <code>Kernel::catch</code>.
  def self.throw(p1, p2 = v2) end

  # Controls tracing of assignments to global variables. The parameter
  # +symbol_ identifies the variable (as either a string name or a
  # symbol identifier). _cmd_ (which may be a string or a
  # +Proc+ object) or block is executed whenever the variable
  # is assigned. The block or +Proc+ object receives the
  # variable's new value as a parameter. Also see
  # <code>Kernel::untrace_var</code>.
  #
  #    trace_var :$_, proc {|v| puts "$_ is now '#{v}'" }
  #    $_ = "hello"
  #    $_ = ' there'
  #
  # <em>produces:</em>
  #
  #    $_ is now 'hello'
  #    $_ is now ' there'
  def self.trace_var(*several_variants) end

  # Specifies the handling of signals. The first parameter is a signal
  # name (a string such as ``SIGALRM'', ``SIGUSR1'', and so on) or a
  # signal number. The characters ``SIG'' may be omitted from the
  # signal name. The command or block specifies code to be run when the
  # signal is raised.
  # If the command is the string ``IGNORE'' or ``SIG_IGN'', the signal
  # will be ignored.
  # If the command is ``DEFAULT'' or ``SIG_DFL'', the Ruby's default handler
  # will be invoked.
  # If the command is ``EXIT'', the script will be terminated by the signal.
  # If the command is ``SYSTEM_DEFAULT'', the operating system's default
  # handler will be invoked.
  # Otherwise, the given command or block will be run.
  # The special signal name ``EXIT'' or signal number zero will be
  # invoked just prior to program termination.
  # trap returns the previous handler for the given signal.
  #
  #     Signal.trap(0, proc { puts "Terminating: #{$$}" })
  #     Signal.trap("CLD")  { puts "Child died" }
  #     fork && Process.wait
  #
  # produces:
  #     Terminating: 27461
  #     Child died
  #     Terminating: 27460
  def self.trap(*several_variants) end

  # Removes tracing for the specified command on the given global
  # variable and returns +nil+. If no command is specified,
  # removes all tracing for that variable and returns an array
  # containing the commands actually removed.
  def self.untrace_var(symbol, *cmd) end

  # Displays each of the given messages followed by a record separator on
  # STDERR unless warnings have been disabled (for example with the
  # <code>-W0</code> flag).
  #
  #    warn("warning 1", "warning 2")
  #
  #  <em>produces:</em>
  #
  #    warning 1
  #    warning 2
  def self.warn(msg, *args) end

  # Returns true if two objects do not match (using the <i>=~</i>
  # method), otherwise false.
  def !~(other) end

  # Returns 0 if +obj+ and +other+ are the same object
  # or <code>obj == other</code>, otherwise nil.
  #
  # The <=> is used by various methods to compare objects, for example
  # Enumerable#sort, Enumerable#max etc.
  #
  # Your implementation of <=> should return one of the following values: -1, 0,
  # 1 or nil. -1 means self is smaller than other. 0 means self is equal to other.
  # 1 means self is bigger than other. Nil means the two values could not be
  # compared.
  #
  # When you define <=>, you can include Comparable to gain the methods <=, <,
  # ==, >=, > and between?.
  def <=>(other) end

  # Case Equality -- For class Object, effectively the same as calling
  # <code>#==</code>, but typically overridden by descendants to provide
  # meaningful semantics in +case+ statements.
  def ===(other) end

  # Pattern Match---Overridden by descendants (notably
  # <code>Regexp</code> and <code>String</code>) to provide meaningful
  # pattern-match semantics.
  def =~(other) end

  # Returns the class of <i>obj</i>. This method must always be
  # called with an explicit receiver, as <code>class</code> is also a
  # reserved word in Ruby.
  #
  #    1.class      #=> Fixnum
  #    self.class   #=> Object
  def class; end

  # Produces a shallow copy of <i>obj</i>---the instance variables of
  # <i>obj</i> are copied, but not the objects they reference.
  # <code>clone</code> copies the frozen and tainted state of <i>obj</i>.
  # See also the discussion under <code>Object#dup</code>.
  #
  #    class Klass
  #       attr_accessor :str
  #    end
  #    s1 = Klass.new      #=> #<Klass:0x401b3a38>
  #    s1.str = "Hello"    #=> "Hello"
  #    s2 = s1.clone       #=> #<Klass:0x401b3998 @str="Hello">
  #    s2.str[1,4] = "i"   #=> "i"
  #    s1.inspect          #=> "#<Klass:0x401b3a38 @str=\"Hi\">"
  #    s2.inspect          #=> "#<Klass:0x401b3998 @str=\"Hi\">"
  #
  # This method may have class-specific behavior.  If so, that
  # behavior will be documented under the #+initialize_copy+ method of
  # the class.
  def clone; end

  # Defines a singleton method in the receiver. The _method_
  # parameter can be a +Proc+, a +Method+ or an +UnboundMethod+ object.
  # If a block is specified, it is used as the method body.
  #
  #    class A
  #      class << self
  #        def class_name
  #          to_s
  #        end
  #      end
  #    end
  #    A.define_singleton_method(:who_am_i) do
  #      "I am: #{class_name}"
  #    end
  #    A.who_am_i   # ==> "I am: A"
  #
  #    guy = "Bob"
  #    guy.define_singleton_method(:hello) { "#{self}: Hello there!" }
  #    guy.hello    #=>  "Bob: Hello there!"
  def define_singleton_method(*several_variants) end

  # Prints <i>obj</i> on the given port (default <code>$></code>).
  # Equivalent to:
  #
  #    def display(port=$>)
  #      port.write self
  #    end
  #
  # For example:
  #
  #    1.display
  #    "cat".display
  #    [ 4, 5, 6 ].display
  #    puts
  #
  # <em>produces:</em>
  #
  #    1cat456
  def display(port = $>) end

  # Produces a shallow copy of <i>obj</i>---the instance variables of
  # <i>obj</i> are copied, but not the objects they reference.
  # <code>dup</code> copies the tainted state of <i>obj</i>.
  #
  # This method may have class-specific behavior.  If so, that
  # behavior will be documented under the #+initialize_copy+ method of
  # the class.
  #
  # === on dup vs clone
  #
  # In general, <code>clone</code> and <code>dup</code> may have different
  # semantics in descendant classes. While <code>clone</code> is used to
  # duplicate an object, including its internal state, <code>dup</code>
  # typically uses the class of the descendant object to create the new
  # instance.
  #
  # When using #dup, any modules that the object has been extended with will not
  # be copied.
  #
  #     class Klass
  #       attr_accessor :str
  #     end
  #
  #     module Foo
  #       def foo; 'foo'; end
  #     end
  #
  #     s1 = Klass.new #=> #<Klass:0x401b3a38>
  #     s1.extend(Foo) #=> #<Klass:0x401b3a38>
  #     s1.foo #=> "foo"
  #
  #     s2 = s1.clone #=> #<Klass:0x401b3a38>
  #     s2.foo #=> "foo"
  #
  #     s3 = s1.dup #=> #<Klass:0x401b3a38>
  #     s3.foo #=> NoMethodError: undefined method `foo' for #<Klass:0x401b3a38>
  def dup; end

  # Equality --- At the <code>Object</code> level, <code>==</code> returns
  # <code>true</code> only if +obj+ and +other+ are the same object.
  # Typically, this method is overridden in descendant classes to provide
  # class-specific meaning.
  #
  # Unlike <code>==</code>, the <code>equal?</code> method should never be
  # overridden by subclasses as it is used to determine object identity
  # (that is, <code>a.equal?(b)</code> if and only if <code>a</code> is the
  # same object as <code>b</code>):
  #
  #   obj = "a"
  #   other = obj.dup
  #
  #   obj == other      #=> true
  #   obj.equal? other  #=> false
  #   obj.equal? obj    #=> true
  #
  # The <code>eql?</code> method returns <code>true</code> if +obj+ and
  # +other+ refer to the same hash key.  This is used by Hash to test members
  # for equality.  For objects of class <code>Object</code>, <code>eql?</code>
  # is synonymous with <code>==</code>.  Subclasses normally continue this
  # tradition by aliasing <code>eql?</code> to their overridden <code>==</code>
  # method, but there are exceptions.  <code>Numeric</code> types, for
  # example, perform type conversion across <code>==</code>, but not across
  # <code>eql?</code>, so:
  #
  #    1 == 1.0     #=> true
  #    1.eql? 1.0   #=> false
  def eql?(other) end

  # Adds to _obj_ the instance methods from each module given as a
  # parameter.
  #
  #    module Mod
  #      def hello
  #        "Hello from Mod.\n"
  #      end
  #    end
  #
  #    class Klass
  #      def hello
  #        "Hello from Klass.\n"
  #      end
  #    end
  #
  #    k = Klass.new
  #    k.hello         #=> "Hello from Klass.\n"
  #    k.extend(Mod)   #=> #<Klass:0x401b3bc8>
  #    k.hello         #=> "Hello from Mod.\n"
  def extend(module1, *args) end

  # Prevents further modifications to <i>obj</i>. A
  # <code>RuntimeError</code> will be raised if modification is attempted.
  # There is no way to unfreeze a frozen object. See also
  # <code>Object#frozen?</code>.
  #
  # This method returns self.
  #
  #    a = [ "a", "b", "c" ]
  #    a.freeze
  #    a << "z"
  #
  # <em>produces:</em>
  #
  #    prog.rb:3:in `<<': can't modify frozen array (RuntimeError)
  #     from prog.rb:3
  def freeze; end

  # Returns the freeze status of <i>obj</i>.
  #
  #    a = [ "a", "b", "c" ]
  #    a.freeze    #=> ["a", "b", "c"]
  #    a.frozen?   #=> true
  def frozen?; end

  # Generates a Fixnum hash value for this object.  This function must have the
  # property that <code>a.eql?(b)</code> implies <code>a.hash == b.hash</code>.
  #
  # The hash value is used along with #eql? by the Hash class to determine if
  # two objects reference the same hash key.  Any hash value that exceeds the
  # capacity of a Fixnum will be truncated before being used.
  #
  # The hash value for an object may not be identical across invocations or
  # implementations of Ruby.  If you need a stable identifier across Ruby
  # invocations and implementations you will need to generate one with a custom
  # method.
  def hash; end

  # Returns a string containing a human-readable representation of <i>obj</i>.
  # The default <code>inspect</code> shows the object's class name,
  # an encoding of the object id, and a list of the instance variables and
  # their values (by calling #inspect on each of them).
  # User defined classes should override this method to provide a better
  # representation of <i>obj</i>.  When overriding this method, it should
  # return a string whose encoding is compatible with the default external
  # encoding.
  #
  #     [ 1, 2, 3..4, 'five' ].inspect   #=> "[1, 2, 3..4, \"five\"]"
  #     Time.new.inspect                 #=> "2008-03-08 19:43:39 +0900"
  #
  #     class Foo
  #     end
  #     Foo.new.inspect                  #=> "#<Foo:0x0300c868>"
  #
  #     class Bar
  #       def initialize
  #         @bar = 1
  #       end
  #     end
  #     Bar.new.inspect                  #=> "#<Bar:0x0300c868 @bar=1>"
  def inspect; end

  # Returns <code>true</code> if <i>obj</i> is an instance of the given
  # class. See also <code>Object#kind_of?</code>.
  #
  #    class A;     end
  #    class B < A; end
  #    class C < B; end
  #
  #    b = B.new
  #    b.instance_of? A   #=> false
  #    b.instance_of? B   #=> true
  #    b.instance_of? C   #=> false
  def instance_of?(class1) end

  # Returns <code>true</code> if the given instance variable is
  # defined in <i>obj</i>.
  # String arguments are converted to symbols.
  #
  #    class Fred
  #      def initialize(p1, p2)
  #        @a, @b = p1, p2
  #      end
  #    end
  #    fred = Fred.new('cat', 99)
  #    fred.instance_variable_defined?(:@a)    #=> true
  #    fred.instance_variable_defined?("@b")   #=> true
  #    fred.instance_variable_defined?("@c")   #=> false
  def instance_variable_defined?(*several_variants) end

  # Returns the value of the given instance variable, or nil if the
  # instance variable is not set. The <code>@</code> part of the
  # variable name should be included for regular instance
  # variables. Throws a <code>NameError</code> exception if the
  # supplied symbol is not valid as an instance variable name.
  # String arguments are converted to symbols.
  #
  #    class Fred
  #      def initialize(p1, p2)
  #        @a, @b = p1, p2
  #      end
  #    end
  #    fred = Fred.new('cat', 99)
  #    fred.instance_variable_get(:@a)    #=> "cat"
  #    fred.instance_variable_get("@b")   #=> 99
  def instance_variable_get(*several_variants) end

  # Sets the instance variable named by <i>symbol</i> to the given
  # object, thereby frustrating the efforts of the class's
  # author to attempt to provide proper encapsulation. The variable
  # does not have to exist prior to this call.
  # If the instance variable name is passed as a string, that string
  # is converted to a symbol.
  #
  #    class Fred
  #      def initialize(p1, p2)
  #        @a, @b = p1, p2
  #      end
  #    end
  #    fred = Fred.new('cat', 99)
  #    fred.instance_variable_set(:@a, 'dog')   #=> "dog"
  #    fred.instance_variable_set(:@c, 'cat')   #=> "cat"
  #    fred.inspect                             #=> "#<Fred:0x401b3da8 @a=\"dog\", @b=99, @c=\"cat\">"
  def instance_variable_set(*several_variants) end

  # Returns an array of instance variable names for the receiver. Note
  # that simply defining an accessor does not create the corresponding
  # instance variable.
  #
  #    class Fred
  #      attr_accessor :a1
  #      def initialize
  #        @iv = 3
  #      end
  #    end
  #    Fred.new.instance_variables   #=> [:@iv]
  def instance_variables; end

  # Returns <code>true</code> if <i>class</i> is the class of
  # <i>obj</i>, or if <i>class</i> is one of the superclasses of
  # <i>obj</i> or modules included in <i>obj</i>.
  #
  #    module M;    end
  #    class A
  #      include M
  #    end
  #    class B < A; end
  #    class C < B; end
  #
  #    b = B.new
  #    b.is_a? A          #=> true
  #    b.is_a? B          #=> true
  #    b.is_a? C          #=> false
  #    b.is_a? M          #=> true
  #
  #    b.kind_of? A       #=> true
  #    b.kind_of? B       #=> true
  #    b.kind_of? C       #=> false
  #    b.kind_of? M       #=> true
  def kind_of?(class1) end
  alias is_a? kind_of?

  # Looks up the named method as a receiver in <i>obj</i>, returning a
  # <code>Method</code> object (or raising <code>NameError</code>). The
  # <code>Method</code> object acts as a closure in <i>obj</i>'s object
  # instance, so instance variables and the value of <code>self</code>
  # remain available.
  #
  #    class Demo
  #      def initialize(n)
  #        @iv = n
  #      end
  #      def hello()
  #        "Hello, @iv = #{@iv}"
  #      end
  #    end
  #
  #    k = Demo.new(99)
  #    m = k.method(:hello)
  #    m.call   #=> "Hello, @iv = 99"
  #
  #    l = Demo.new('Fred')
  #    m = l.method("hello")
  #    m.call   #=> "Hello, @iv = Fred"
  def method(sym) end

  # Returns a list of the names of public and protected methods of
  # <i>obj</i>. This will include all the methods accessible in
  # <i>obj</i>'s ancestors.
  # If the <i>regular</i> parameter is set to <code>false</code>,
  # Returns an array of obj's public and protected singleton methods,
  # the array will not include methods in modules included in <i>obj</i>.
  #
  #    class Klass
  #      def klass_method()
  #      end
  #    end
  #    k = Klass.new
  #    k.methods[0..9]    #=> [:klass_method, :nil?, :===,
  #                       #    :==~, :!, :eql?
  #                       #    :hash, :<=>, :class, :singleton_class]
  #    k.methods.length   #=> 57
  #
  #    k.methods(false)   #=> []
  #    def k.singleton_method; end
  #    k.methods(false)   #=> [:singleton_method]
  #
  #    module M123; def m123; end end
  #    k.extend M123
  #    k.methods(false)   #=> [:singleton_method]
  def methods(regular = true) end

  # Only the object <i>nil</i> responds <code>true</code> to <code>nil?</code>.
  #
  #    Object.new.nil?   #=> false
  #    nil.nil?          #=> true
  def nil?; end

  # Returns an integer identifier for +obj+.
  #
  # The same number will be returned on all calls to +id+ for a given object,
  # and no two active objects will share an id.
  #
  # Object#object_id is a different concept from the +:name+ notation, which
  # returns the symbol id of +name+.
  #
  # Replaces the deprecated Object#id.
  def object_id; end

  # Returns the list of private methods accessible to <i>obj</i>. If
  # the <i>all</i> parameter is set to <code>false</code>, only those methods
  # in the receiver will be listed.
  def private_methods(all = true) end

  # Returns the list of protected methods accessible to <i>obj</i>. If
  # the <i>all</i> parameter is set to <code>false</code>, only those methods
  # in the receiver will be listed.
  def protected_methods(all = true) end

  # Similar to _method_, searches public method only.
  def public_method(sym) end

  # Returns the list of public methods accessible to <i>obj</i>. If
  # the <i>all</i> parameter is set to <code>false</code>, only those methods
  # in the receiver will be listed.
  def public_methods(all = true) end

  # Invokes the method identified by _symbol_, passing it any
  # arguments specified. Unlike send, public_send calls public
  # methods only.
  # When the method is identified by a string, the string is converted
  # to a symbol.
  #
  #    1.public_send(:puts, "hello")  # causes NoMethodError
  def public_send(*several_variants) end

  # Removes the named instance variable from <i>obj</i>, returning that
  # variable's value.
  #
  #    class Dummy
  #      attr_reader :var
  #      def initialize
  #        @var = 99
  #      end
  #      def remove
  #        remove_instance_variable(:@var)
  #      end
  #    end
  #    d = Dummy.new
  #    d.var      #=> 99
  #    d.remove   #=> 99
  #    d.var      #=> nil
  def remove_instance_variable(symbol) end

  # Returns +true+ if _obj_ responds to the given method.  Private and
  # protected methods are included in the search only if the optional
  # second parameter evaluates to +true+.
  #
  # If the method is not implemented,
  # as Process.fork on Windows, File.lchmod on GNU/Linux, etc.,
  # false is returned.
  #
  # If the method is not defined, <code>respond_to_missing?</code>
  # method is called and the result is returned.
  #
  # When the method name parameter is given as a string, the string is
  # converted to a symbol.
  def respond_to?(*several_variants) end

  # DO NOT USE THIS DIRECTLY.
  #
  # Hook method to return whether the _obj_ can respond to _id_ method
  # or not.
  #
  # When the method name parameter is given as a string, the string is
  # converted to a symbol.
  #
  # See #respond_to?, and the example of BasicObject.
  def respond_to_missing?(*several_variants) end

  #  Invokes the method identified by _symbol_, passing it any
  #  arguments specified. You can use <code>__send__</code> if the name
  #  +send+ clashes with an existing method in _obj_.
  #  When the method is identified by a string, the string is converted
  #  to a symbol.
  #
  #     class Klass
  #       def hello(*args)
  #         "Hello " + args.join(' ')
  #       end
  #     end
  #     k = Klass.new
  #     k.send :hello, "gentle", "readers"   #=> "Hello gentle readers"
  def send(*several_variants) end

  # Returns the singleton class of <i>obj</i>.  This method creates
  # a new singleton class if <i>obj</i> does not have one.
  #
  # If <i>obj</i> is <code>nil</code>, <code>true</code>, or
  # <code>false</code>, it returns NilClass, TrueClass, or FalseClass,
  # respectively.
  # If <i>obj</i> is a Fixnum or a Symbol, it raises a TypeError.
  #
  #    Object.new.singleton_class  #=> #<Class:#<Object:0xb7ce1e24>>
  #    String.singleton_class      #=> #<Class:String>
  #    nil.singleton_class         #=> NilClass
  def singleton_class; end

  # Similar to _method_, searches singleton method only.
  #
  #    class Demo
  #      def initialize(n)
  #        @iv = n
  #      end
  #      def hello()
  #        "Hello, @iv = #{@iv}"
  #      end
  #    end
  #
  #    k = Demo.new(99)
  #    def k.hi
  #      "Hi, @iv = #{@iv}"
  #    end
  #    m = k.singleton_method(:hi)
  #    m.call   #=> "Hi, @iv = 99"
  #    m = k.singleton_method(:hello) #=> NameError
  def singleton_method(sym) end

  # Returns an array of the names of singleton methods for <i>obj</i>.
  # If the optional <i>all</i> parameter is true, the list will include
  # methods in modules included in <i>obj</i>.
  # Only public and protected singleton methods are returned.
  #
  #    module Other
  #      def three() end
  #    end
  #
  #    class Single
  #      def Single.four() end
  #    end
  #
  #    a = Single.new
  #
  #    def a.one()
  #    end
  #
  #    class << a
  #      include Other
  #      def two()
  #      end
  #    end
  #
  #    Single.singleton_methods    #=> [:four]
  #    a.singleton_methods(false)  #=> [:two, :one]
  #    a.singleton_methods         #=> [:two, :one, :three]
  def singleton_methods(all = true) end

  # Mark the object as tainted.
  #
  # Objects that are marked as tainted will be restricted from various built-in
  # methods. This is to prevent insecure data, such as command-line arguments
  # or strings read from Kernel#gets, from inadvertently compromising the user's
  # system.
  #
  # To check whether an object is tainted, use #tainted?.
  #
  # You should only untaint a tainted object if your code has inspected it and
  # determined that it is safe. To do so use #untaint.
  #
  # In $SAFE level 3, all newly created objects are tainted and you can't untaint
  # objects.
  def taint; end

  # Returns true if the object is tainted.
  #
  # See #taint for more information.
  def tainted?; end

  # Yields self to the block, and then returns self.
  # The primary purpose of this method is to "tap into" a method chain,
  # in order to perform operations on intermediate results within the chain.
  #
  #     (1..10)                .tap {|x| puts "original: #{x.inspect}"}
  #       .to_a                .tap {|x| puts "array: #{x.inspect}"}
  #       .select {|x| x%2==0} .tap {|x| puts "evens: #{x.inspect}"}
  #       .map {|x| x*x}       .tap {|x| puts "squares: #{x.inspect}"}
  def tap; end

  # Creates a new Enumerator which will enumerate by calling +method+ on
  # +obj+, passing +args+ if any.
  #
  # If a block is given, it will be used to calculate the size of
  # the enumerator without the need to iterate it (see Enumerator#size).
  #
  # === Examples
  #
  #   str = "xyz"
  #
  #   enum = str.enum_for(:each_byte)
  #   enum.each { |b| puts b }
  #   # => 120
  #   # => 121
  #   # => 122
  #
  #   # protect an array from being modified by some_method
  #   a = [1, 2, 3]
  #   some_method(a.to_enum)
  #
  # It is typical to call to_enum when defining methods for
  # a generic Enumerable, in case no block is passed.
  #
  # Here is such an example, with parameter passing and a sizing block:
  #
  #   module Enumerable
  #     # a generic method to repeat the values of any enumerable
  #     def repeat(n)
  #       raise ArgumentError, "#{n} is negative!" if n < 0
  #       unless block_given?
  #         return to_enum(__method__, n) do # __method__ is :repeat here
  #           sz = size     # Call size and multiply by n...
  #           sz * n if sz  # but return nil if size itself is nil
  #         end
  #       end
  #       each do |*val|
  #         n.times { yield *val }
  #       end
  #     end
  #   end
  #
  #   %i[hello world].repeat(2) { |w| puts w }
  #     # => Prints 'hello', 'hello', 'world', 'world'
  #   enum = (1..14).repeat(3)
  #     # => returns an Enumerator when called without a block
  #   enum.first(4) # => [1, 1, 1, 2]
  #   enum.size # => 42
  def to_enum(method = :each, *args) end
  alias enum_for to_enum

  # Returns a string representing <i>obj</i>. The default
  # <code>to_s</code> prints the object's class and an encoding of the
  # object id. As a special case, the top-level object that is the
  # initial execution context of Ruby programs returns ``main''.
  def to_s; end

  # Deprecated method that is equivalent to #untaint.
  def trust; end

  # Removes the tainted mark from the object.
  #
  # See #taint for more information.
  def untaint; end

  # Deprecated method that is equivalent to #taint.
  def untrust; end

  # Deprecated method that is equivalent to #tainted?.
  def untrusted?; end

  private

  # Returns the called name of the current method as a Symbol.
  # If called outside of a method, it returns <code>nil</code>.
  def __callee__; end

  # Returns the canonicalized absolute path of the directory of the file from
  # which this method is called. It means symlinks in the path is resolved.
  # If <code>__FILE__</code> is <code>nil</code>, it returns <code>nil</code>.
  # The return value equals to <code>File.dirname(File.realpath(__FILE__))</code>.
  def __dir__; end

  # Returns the name at the definition of the current method as a
  # Symbol.
  # If called outside of a method, it returns <code>nil</code>.
  def __method__; end

  # Returns the standard output of running _cmd_ in a subshell.
  # The built-in syntax <code>%x{...}</code> uses
  # this method. Sets <code>$?</code> to the process status.
  #
  #    `date`                   #=> "Wed Apr  9 08:56:30 CDT 2003\n"
  #    `ls testdir`.split[1]    #=> "main.rb"
  #    `echo oops && exit 99`   #=> "oops\n"
  #    $?.exitstatus            #=> 99
  def `(cmd) end

  # Returns +arg+ as an Array.
  #
  # First tries to call <code>to_ary</code> on +arg+, then <code>to_a</code>.
  #
  #    Array(1..5)   #=> [1, 2, 3, 4, 5]
  def Array(arg) end

  # See also BigDecimal::new
  def BigDecimal(*args) end

  # Returns x+i*y;
  #
  #    Complex(1, 2)    #=> (1+2i)
  #    Complex('1+2i')  #=> (1+2i)
  #    Complex(nil)     #=> TypeError
  #    Complex(1, nil)  #=> TypeError
  #
  # Syntax of string form:
  #
  #   string form = extra spaces , complex , extra spaces ;
  #   complex = real part | [ sign ] , imaginary part
  #           | real part , sign , imaginary part
  #           | rational , "@" , rational ;
  #   real part = rational ;
  #   imaginary part = imaginary unit | unsigned rational , imaginary unit ;
  #   rational = [ sign ] , unsigned rational ;
  #   unsigned rational = numerator | numerator , "/" , denominator ;
  #   numerator = integer part | fractional part | integer part , fractional part ;
  #   denominator = digits ;
  #   integer part = digits ;
  #   fractional part = "." , digits , [ ( "e" | "E" ) , [ sign ] , digits ] ;
  #   imaginary unit = "i" | "I" | "j" | "J" ;
  #   sign = "-" | "+" ;
  #   digits = digit , { digit | "_" , digit };
  #   digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
  #   extra spaces = ? \s* ? ;
  #
  # See String#to_c.
  def Complex(*args) end

  # Returns <i>arg</i> converted to a float. Numeric types are converted
  # directly, the rest are converted using <i>arg</i>.to_f.
  # Converting <code>nil</code> generates a <code>TypeError</code>.
  #
  #    Float(1)           #=> 1.0
  #    Float("123.456")   #=> 123.456
  def Float(arg) end

  # Converts <i>arg</i> to a <code>Hash</code> by calling
  # <i>arg</i><code>.to_hash</code>. Returns an empty <code>Hash</code> when
  # <i>arg</i> is <tt>nil</tt> or <tt>[]</tt>.
  #
  #    Hash([])          #=> {}
  #    Hash(nil)         #=> {}
  #    Hash(key: :value) #=> {:key => :value}
  #    Hash([1, 2, 3])   #=> TypeError
  def Hash(arg) end

  # Converts <i>arg</i> to a <code>Fixnum</code> or <code>Bignum</code>.
  # Numeric types are converted directly (with floating point numbers
  # being truncated).  <i>base</i> (0, or between 2 and 36) is a base for
  # integer string representation.  If <i>arg</i> is a <code>String</code>,
  # when <i>base</i> is omitted or equals zero, radix indicators
  # (<code>0</code>, <code>0b</code>, and <code>0x</code>) are honored.
  # In any case, strings should be strictly conformed to numeric
  # representation. This behavior is different from that of
  # <code>String#to_i</code>.  Non string values will be converted by first
  # trying <code>to_int</code>, then <code>to_i</code>. Passing <code>nil</code>
  # raises a TypeError.
  #
  #    Integer(123.999)    #=> 123
  #    Integer("0x1a")     #=> 26
  #    Integer(Time.new)   #=> 1204973019
  #    Integer("0930", 10) #=> 930
  #    Integer("111", 2)   #=> 7
  #    Integer(nil)        #=> TypeError
  def Integer(arg, base = 0) end

  # Creates a new Pathname object from the given string, +path+, and returns
  # pathname object.
  #
  # In order to use this constructor, you must first require the Pathname
  # standard library extension.
  #
  #      require 'pathname'
  #      Pathname("/home/zzak")
  #      #=> #<Pathname:/home/zzak>
  #
  # See also Pathname::new for more information.
  def Pathname(path) end

  # Returns x/y;
  #
  #    Rational(1, 2)   #=> (1/2)
  #    Rational('1/2')  #=> (1/2)
  #    Rational(nil)    #=> TypeError
  #    Rational(1, nil) #=> TypeError
  #
  # Syntax of string form:
  #
  #   string form = extra spaces , rational , extra spaces ;
  #   rational = [ sign ] , unsigned rational ;
  #   unsigned rational = numerator | numerator , "/" , denominator ;
  #   numerator = integer part | fractional part | integer part , fractional part ;
  #   denominator = digits ;
  #   integer part = digits ;
  #   fractional part = "." , digits , [ ( "e" | "E" ) , [ sign ] , digits ] ;
  #   sign = "-" | "+" ;
  #   digits = digit , { digit | "_" , digit } ;
  #   digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
  #   extra spaces = ? \s* ? ;
  #
  # See String#to_r.
  def Rational(*args) end

  # Returns <i>arg</i> as a <code>String</code>.
  #
  # First tries to call its <code>to_str</code> method, then its <code>to_s</code> method.
  #
  #    String(self)        #=> "main"
  #    String(self.class)  #=> "Object"
  #    String(123456)      #=> "123456"
  def String(arg) end

  # Terminate execution immediately, effectively by calling
  # <code>Kernel.exit(false)</code>. If _msg_ is given, it is written
  # to STDERR prior to terminating.
  def abort(message = '') end

  # Converts _block_ to a +Proc+ object (and therefore
  # binds it at the point of call) and registers it for execution when
  # the program exits. If multiple handlers are registered, they are
  # executed in reverse order of registration.
  #
  #    def do_at_exit(str1)
  #      at_exit { print str1 }
  #    end
  #    at_exit { puts "cruel world" }
  #    do_at_exit("goodbye ")
  #    exit
  #
  # <em>produces:</em>
  #
  #    goodbye cruel world
  def at_exit; end

  # Registers _filename_ to be loaded (using <code>Kernel::require</code>)
  # the first time that _module_ (which may be a <code>String</code> or
  # a symbol) is accessed.
  #
  #    autoload(:MyModule, "/usr/local/lib/modules/my_module.rb")
  def autoload(module1, filename) end

  # Returns _filename_ to be loaded if _name_ is registered as
  # +autoload+.
  #
  #    autoload(:B, "b")
  #    autoload?(:B)            #=> "b"
  def autoload?(name) end

  # Returns a +Binding+ object, describing the variable and
  # method bindings at the point of call. This object can be used when
  # calling +eval+ to execute the evaluated command in this
  # environment. See also the description of class +Binding+.
  #
  #    def get_binding(param)
  #      return binding
  #    end
  #    b = get_binding("hello")
  #    eval("param", b)   #=> "hello"
  def binding; end

  # Generates a Continuation object, which it passes to
  # the associated block. You need to <code>require
  # 'continuation'</code> before using this method. Performing a
  # <em>cont</em><code>.call</code> will cause the #callcc
  # to return (as will falling through the end of the block). The
  # value returned by the #callcc is the value of the
  # block, or the value passed to <em>cont</em><code>.call</code>. See
  # class Continuation for more details. Also see
  # Kernel#throw for an alternative mechanism for
  # unwinding a call stack.
  def callcc; end

  # Returns the current execution stack---an array containing strings in
  # the form <code>file:line</code> or <code>file:line: in
  # `method'</code>.
  #
  # The optional _start_ parameter determines the number of initial stack
  # entries to omit from the top of the stack.
  #
  # A second optional +length+ parameter can be used to limit how many entries
  # are returned from the stack.
  #
  # Returns +nil+ if _start_ is greater than the size of
  # current execution stack.
  #
  # Optionally you can pass a range, which will return an array containing the
  # entries within the specified range.
  #
  #    def a(skip)
  #      caller(skip)
  #    end
  #    def b(skip)
  #      a(skip)
  #    end
  #    def c(skip)
  #      b(skip)
  #    end
  #    c(0)   #=> ["prog:2:in `a'", "prog:5:in `b'", "prog:8:in `c'", "prog:10:in `<main>'"]
  #    c(1)   #=> ["prog:5:in `b'", "prog:8:in `c'", "prog:11:in `<main>'"]
  #    c(2)   #=> ["prog:8:in `c'", "prog:12:in `<main>'"]
  #    c(3)   #=> ["prog:13:in `<main>'"]
  #    c(4)   #=> []
  #    c(5)   #=> nil
  def caller(*several_variants) end

  # Returns the current execution stack---an array containing
  # backtrace location objects.
  #
  # See Thread::Backtrace::Location for more information.
  #
  # The optional _start_ parameter determines the number of initial stack
  # entries to omit from the top of the stack.
  #
  # A second optional +length+ parameter can be used to limit how many entries
  # are returned from the stack.
  #
  # Returns +nil+ if _start_ is greater than the size of
  # current execution stack.
  #
  # Optionally you can pass a range, which will return an array containing the
  # entries within the specified range.
  def caller_locations(*several_variants) end

  # +catch+ executes its block. If a +throw+ is
  # executed, Ruby searches up its stack for a +catch+ block
  # with a tag corresponding to the +throw+'s
  # _tag_. If found, that block is terminated, and
  # +catch+ returns the value given to +throw+. If
  # +throw+ is not called, the block terminates normally, and
  # the value of +catch+ is the value of the last expression
  # evaluated. +catch+ expressions may be nested, and the
  # +throw+ call need not be in lexical scope.
  #
  #    def routine(n)
  #      puts n
  #      throw :done if n <= 0
  #      routine(n-1)
  #    end
  #
  #    catch(:done) { routine(3) }
  #
  # <em>produces:</em>
  #
  #    3
  #    2
  #    1
  #    0
  #
  # when _arg_ is given, +catch+ yields it as is, or when no
  # _arg_ is given, +catch+ assigns a new unique object to
  # +throw+.  this is useful for nested +catch+.  _arg_ can
  # be an arbitrary object, not only Symbol.
  def catch(*arg) end

  # Equivalent to <code>$_ = $_.chomp(<em>string</em>)</code>. See
  # <code>String#chomp</code>.
  # Available only when -p/-n command line option specified.
  def chomp(*several_variants) end

  # Equivalent to <code>($_.dup).chop!</code>, except <code>nil</code>
  # is never returned. See <code>String#chop!</code>.
  # Available only when -p/-n command line option specified.
  def chop; end

  # Evaluates the Ruby expression(s) in <em>string</em>. If
  # <em>binding</em> is given, which must be a <code>Binding</code>
  # object, the evaluation is performed in its context. If the
  # optional <em>filename</em> and <em>lineno</em> parameters are
  # present, they will be used when reporting syntax errors.
  #
  #    def get_binding(str)
  #      return binding
  #    end
  #    str = "hello"
  #    eval "str + ' Fred'"                      #=> "hello Fred"
  #    eval "str + ' Fred'", get_binding("bye")  #=> "bye Fred"
  def eval(string, *binding_filename_lineno) end

  # Replaces the current process by running the given external _command_, which
  # can take one of the following forms:
  #
  # [<code>exec(commandline)</code>]
  #     command line string which is passed to the standard shell
  # [<code>exec(cmdname, arg1, ...)</code>]
  #     command name and one or more arguments (no shell)
  # [<code>exec([cmdname, argv0], arg1, ...)</code>]
  #     command name, argv[0] and zero or more arguments (no shell)
  #
  # In the first form, the string is taken as a command line that is subject to
  # shell expansion before being executed.
  #
  # The standard shell always means <code>"/bin/sh"</code> on Unix-like systems,
  # same as <code>ENV["RUBYSHELL"]</code>
  # (or <code>ENV["COMSPEC"]</code> on Windows NT series), and similar.
  #
  # If the string from the first form (<code>exec("command")</code>) follows
  # these simple rules:
  #
  # * no meta characters
  # * no shell reserved word and no special built-in
  # * Ruby invokes the command directly without shell
  #
  # You can force shell invocation by adding ";" to the string (because ";" is
  # a meta character).
  #
  # Note that this behavior is observable by pid obtained
  # (return value of spawn() and IO#pid for IO.popen) is the pid of the invoked
  # command, not shell.
  #
  # In the second form (<code>exec("command1", "arg1", ...)</code>), the first
  # is taken as a command name and the rest are passed as parameters to command
  # with no shell expansion.
  #
  # In the third form (<code>exec(["command", "argv0"], "arg1", ...)</code>),
  # starting a two-element array at the beginning of the command, the first
  # element is the command to be executed, and the second argument is used as
  # the <code>argv[0]</code> value, which may show up in process listings.
  #
  # In order to execute the command, one of the <code>exec(2)</code> system
  # calls are used, so the running command may inherit some of the environment
  # of the original program (including open file descriptors).
  #
  # This behavior is modified by the given +env+ and +options+ parameters. See
  # ::spawn for details.
  #
  # If the command fails to execute (typically <code>Errno::ENOENT</code> when
  # it was not found) a SystemCallError exception is raised.
  #
  # This method modifies process attributes according to given +options+ before
  # <code>exec(2)</code> system call. See ::spawn for more details about the
  # given +options+.
  #
  # The modified attributes may be retained when <code>exec(2)</code> system
  # call fails.
  #
  # For example, hard resource limits are not restorable.
  #
  # Consider to create a child process using ::spawn or Kernel#system if this
  # is not acceptable.
  #
  #    exec "echo *"       # echoes list of files in current directory
  #    # never get here
  #
  #    exec "echo", "*"    # echoes an asterisk
  #    # never get here
  def exec(*args) end

  # Initiates the termination of the Ruby script by raising the
  # <code>SystemExit</code> exception. This exception may be caught. The
  # optional parameter is used to return a status code to the invoking
  # environment.
  # +true+ and +FALSE+ of _status_ means success and failure
  # respectively.  The interpretation of other integer values are
  # system dependent.
  #
  #    begin
  #      exit
  #      puts "never get here"
  #    rescue SystemExit
  #      puts "rescued a SystemExit exception"
  #    end
  #    puts "after begin block"
  #
  # <em>produces:</em>
  #
  #    rescued a SystemExit exception
  #    after begin block
  #
  # Just prior to termination, Ruby executes any <code>at_exit</code> functions
  # (see Kernel::at_exit) and runs any object finalizers (see
  # ObjectSpace::define_finalizer).
  #
  #    at_exit { puts "at_exit function" }
  #    ObjectSpace.define_finalizer("string",  proc { puts "in finalizer" })
  #    exit
  #
  # <em>produces:</em>
  #
  #    at_exit function
  #    in finalizer
  def exit(status = true) end

  # Exits the process immediately. No exit handlers are
  # run. <em>status</em> is returned to the underlying system as the
  # exit status.
  #
  #    Process.exit!(true)
  def exit!(status = false) end

  # Creates a subprocess. If a block is specified, that block is run
  # in the subprocess, and the subprocess terminates with a status of
  # zero. Otherwise, the +fork+ call returns twice, once in
  # the parent, returning the process ID of the child, and once in
  # the child, returning _nil_. The child process can exit using
  # <code>Kernel.exit!</code> to avoid running any
  # <code>at_exit</code> functions. The parent process should
  # use <code>Process.wait</code> to collect the termination statuses
  # of its children or use <code>Process.detach</code> to register
  # disinterest in their status; otherwise, the operating system
  # may accumulate zombie processes.
  #
  # The thread calling fork is the only thread in the created child process.
  # fork doesn't copy other threads.
  #
  # If fork is not usable, Process.respond_to?(:fork) returns false.
  #
  # Note that fork(2) is not avaiable on some platforms like Windows and NetBSD 4.
  # Therefore you should use spawn() instead of fork().
  def fork; end

  # Returns (and assigns to <code>$_</code>) the next line from the list
  # of files in +ARGV+ (or <code>$*</code>), or from standard input if
  # no files are present on the command line. Returns +nil+ at end of
  # file. The optional argument specifies the record separator. The
  # separator is included with the contents of each record. A separator
  # of +nil+ reads the entire contents, and a zero-length separator
  # reads the input one paragraph at a time, where paragraphs are
  # divided by two consecutive newlines.  If the first argument is an
  # integer, or optional second argument is given, the returning string
  # would not be longer than the given value in bytes.  If multiple
  # filenames are present in +ARGV+, +gets(nil)+ will read the contents
  # one file at a time.
  #
  #    ARGV << "testfile"
  #    print while gets
  #
  # <em>produces:</em>
  #
  #    This is line one
  #    This is line two
  #    This is line three
  #    And so on...
  #
  # The style of programming using <code>$_</code> as an implicit
  # parameter is gradually losing favor in the Ruby community.
  def gets(*several_variants) end

  # Returns an array of the names of global variables.
  #
  #    global_variables.grep /std/   #=> [:$stdin, :$stdout, :$stderr]
  def global_variables; end

  # Equivalent to <code>$_.gsub...</code>, except that <code>$_</code>
  # will be updated if substitution occurs.
  # Available only when -p/-n command line option specified.
  def gsub(*several_variants) end

  # Returns <code>true</code> if <code>yield</code> would execute a
  # block in the current context. The <code>iterator?</code> form
  # is mildly deprecated.
  #
  #    def try
  #      if block_given?
  #        yield
  #      else
  #        "no block"
  #      end
  #    end
  #    try                  #=> "no block"
  #    try { "hello" }      #=> "hello"
  #    try do "hello" end   #=> "hello"
  def iterator?; end
  alias block_given? iterator?

  # Equivalent to <code>Proc.new</code>, except the resulting Proc objects
  # check the number of parameters passed when called.
  def lambda; end

  # Loads and executes the Ruby
  # program in the file _filename_. If the filename does not
  # resolve to an absolute path, the file is searched for in the library
  # directories listed in <code>$:</code>. If the optional _wrap_
  # parameter is +true+, the loaded script will be executed
  # under an anonymous module, protecting the calling program's global
  # namespace. In no circumstance will any local variables in the loaded
  # file be propagated to the loading environment.
  def load(filename, wrap = false) end

  # Returns the names of the current local variables.
  #
  #    fred = 1
  #    for i in 1..10
  #       # ...
  #    end
  #    local_variables   #=> [:fred, :i]
  def local_variables; end

  # Repeatedly executes the block.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    loop do
  #      print "Input: "
  #      line = gets
  #      break if !line or line =~ /^qQ/
  #      # ...
  #    end
  #
  # StopIteration raised in the block breaks the loop.
  def loop; end

  # Creates an IO object connected to the given stream, file, or subprocess.
  #
  # If +path+ does not start with a pipe character (<code>|</code>), treat it
  # as the name of a file to open using the specified mode (defaulting to
  # "r").
  #
  # The +mode+ is either a string or an integer.  If it is an integer, it
  # must be bitwise-or of open(2) flags, such as File::RDWR or File::EXCL.  If
  # it is a string, it is either "fmode", "fmode:ext_enc", or
  # "fmode:ext_enc:int_enc".
  #
  # See the documentation of IO.new for full documentation of the +mode+ string
  # directives.
  #
  # If a file is being created, its initial permissions may be set using the
  # +perm+ parameter.  See File.new and the open(2) and chmod(2) man pages for
  # a description of permissions.
  #
  # If a block is specified, it will be invoked with the IO object as a
  # parameter, and the IO will be automatically closed when the block
  # terminates.  The call returns the value of the block.
  #
  # If +path+ starts with a pipe character (<code>"|"</code>), a subprocess is
  # created, connected to the caller by a pair of pipes.  The returned IO
  # object may be used to write to the standard input and read from the
  # standard output of this subprocess.
  #
  # If the command following the pipe is a single minus sign
  # (<code>"|-"</code>), Ruby forks, and this subprocess is connected to the
  # parent.  If the command is not <code>"-"</code>, the subprocess runs the
  # command.
  #
  # When the subprocess is ruby (opened via <code>"|-"</code>), the +open+
  # call returns +nil+.  If a block is associated with the open call, that
  # block will run twice --- once in the parent and once in the child.
  #
  # The block parameter will be an IO object in the parent and +nil+ in the
  # child. The parent's +IO+ object will be connected to the child's $stdin
  # and $stdout.  The subprocess will be terminated at the end of the block.
  #
  # === Examples
  #
  # Reading from "testfile":
  #
  #    open("testfile") do |f|
  #      print f.gets
  #    end
  #
  # Produces:
  #
  #    This is line one
  #
  # Open a subprocess and read its output:
  #
  #    cmd = open("|date")
  #    print cmd.gets
  #    cmd.close
  #
  # Produces:
  #
  #    Wed Apr  9 08:56:31 CDT 2003
  #
  # Open a subprocess running the same Ruby program:
  #
  #    f = open("|-", "w+")
  #    if f == nil
  #      puts "in Child"
  #      exit
  #    else
  #      puts "Got: #{f.gets}"
  #    end
  #
  # Produces:
  #
  #    Got: in Child
  #
  # Open a subprocess using a block to receive the IO object:
  #
  #    open "|-" do |f|
  #      if f then
  #        # parent process
  #        puts "Got: #{f.gets}"
  #      else
  #        # child process
  #        puts "in Child"
  #      end
  #    end
  #
  # Produces:
  #
  #    Got: in Child
  def open(*args) end

  # For each object, directly writes _obj_.+inspect+ followed by a
  # newline to the program's standard output.
  #
  #    S = Struct.new(:name, :state)
  #    s = S['dave', 'TX']
  #    p s
  #
  # <em>produces:</em>
  #
  #    #<S name="dave", state="TX">
  def p(*several_variants) end

  # Prints each object in turn to <code>$stdout</code>. If the output
  # field separator (<code>$,</code>) is not +nil+, its
  # contents will appear between each field. If the output record
  # separator (<code>$\\</code>) is not +nil+, it will be
  # appended to the output. If no arguments are given, prints
  # <code>$_</code>. Objects that aren't strings will be converted by
  # calling their <code>to_s</code> method.
  #
  #    print "cat", [1,2,3], 99, "\n"
  #    $, = ", "
  #    $\ = "\n"
  #    print "cat", [1,2,3], 99
  #
  # <em>produces:</em>
  #
  #    cat12399
  #    cat, 1, 2, 3, 99
  def print(obj, *args) end

  # Equivalent to:
  #    io.write(sprintf(string, obj, ...))
  # or
  #    $stdout.write(sprintf(string, obj, ...))
  def printf(*several_variants) end

  # Equivalent to <code>Proc.new</code>.
  def proc; end

  #  Equivalent to:
  #
  #    $stdout.putc(int)
  #
  # Refer to the documentation for IO#putc for important information regarding
  # multi-byte characters.
  def putc(int) end

  # Equivalent to
  #
  #     $stdout.puts(obj, ...)
  def puts(obj = '', *arg) end

  # With no arguments, raises the exception in <code>$!</code> or raises
  # a <code>RuntimeError</code> if <code>$!</code> is +nil+.
  # With a single +String+ argument, raises a
  # +RuntimeError+ with the string as a message. Otherwise,
  # the first parameter should be the name of an +Exception+
  # class (or an object that returns an +Exception+ object when sent
  # an +exception+ message). The optional second parameter sets the
  # message associated with the exception, and the third parameter is an
  # array of callback information. Exceptions are caught by the
  # +rescue+ clause of <code>begin...end</code> blocks.
  #
  #    raise "Failed to create socket"
  #    raise ArgumentError, "No parameters", caller
  def raise(*several_variants) end
  alias fail raise

  # If called without an argument, or if <tt>max.to_i.abs == 0</tt>, rand
  # returns a pseudo-random floating point number between 0.0 and 1.0,
  # including 0.0 and excluding 1.0.
  #
  #   rand        #=> 0.2725926052826416
  #
  # When +max.abs+ is greater than or equal to 1, +rand+ returns a pseudo-random
  # integer greater than or equal to 0 and less than +max.to_i.abs+.
  #
  #   rand(100)   #=> 12
  #
  # When +max+ is a Range, +rand+ returns a random number where
  # range.member?(number) == true.
  #
  # Negative or floating point values for +max+ are allowed, but may give
  # surprising results.
  #
  #   rand(-100) # => 87
  #   rand(-0.5) # => 0.8130921818028143
  #   rand(1.9)  # equivalent to rand(1), which is always 0
  #
  # Kernel.srand may be used to ensure that sequences of random numbers are
  # reproducible between different runs of a program.
  #
  # See also Random.rand.
  def rand(max = 0) end

  # Equivalent to <code>Kernel::gets</code>, except
  # +readline+ raises +EOFError+ at end of file.
  def readline(*several_variants) end

  # Returns an array containing the lines returned by calling
  # <code>Kernel.gets(<i>sep</i>)</code> until the end of file.
  def readlines(*several_variants) end

  # Loads the given +name+, returning +true+ if successful and +false+ if the
  # feature is already loaded.
  #
  # If the filename does not resolve to an absolute path, it will be searched
  # for in the directories listed in <code>$LOAD_PATH</code> (<code>$:</code>).
  #
  # If the filename has the extension ".rb", it is loaded as a source file; if
  # the extension is ".so", ".o", or ".dll", or the default shared library
  # extension on the current platform, Ruby loads the shared library as a
  # Ruby extension.  Otherwise, Ruby tries adding ".rb", ".so", and so on
  # to the name until found.  If the file named cannot be found, a LoadError
  # will be raised.
  #
  # For Ruby extensions the filename given may use any shared library
  # extension.  For example, on Linux the socket extension is "socket.so" and
  # <code>require 'socket.dll'</code> will load the socket extension.
  #
  # The absolute path of the loaded file is added to
  # <code>$LOADED_FEATURES</code> (<code>$"</code>).  A file will not be
  # loaded again if its path already appears in <code>$"</code>.  For example,
  # <code>require 'a'; require './a'</code> will not load <code>a.rb</code>
  # again.
  #
  #   require "my-library.rb"
  #   require "db-driver"
  #
  # Any constants or globals within the loaded source file will be available
  # in the calling program's global namespace. However, local variables will
  # not be propagated to the loading environment.
  def require(name) end

  # Ruby tries to load the library named _string_ relative to the requiring
  # file's path.  If the file's path cannot be determined a LoadError is raised.
  # If a file is loaded +true+ is returned and false otherwise.
  def require_relative(string) end

  # Calls select(2) system call.
  # It monitors given arrays of <code>IO</code> objects, waits one or more
  # of <code>IO</code> objects ready for reading, are ready for writing,
  # and have pending exceptions respectively, and returns an array that
  # contains arrays of those IO objects.  It will return <code>nil</code>
  # if optional <i>timeout</i> value is given and no <code>IO</code> object
  # is ready in <i>timeout</i> seconds.
  #
  # <code>IO.select</code> peeks the buffer of <code>IO</code> objects for testing readability.
  # If the <code>IO</code> buffer is not empty,
  # <code>IO.select</code> immediately notify readability.
  # This "peek" is only happen for <code>IO</code> objects.
  # It is not happen for IO-like objects such as OpenSSL::SSL::SSLSocket.
  #
  # The best way to use <code>IO.select</code> is invoking it
  # after nonblocking methods such as <code>read_nonblock</code>, <code>write_nonblock</code>, etc.
  # The methods raises an exception which is extended by
  # <code>IO::WaitReadable</code> or <code>IO::WaitWritable</code>.
  # The modules notify how the caller should wait with <code>IO.select</code>.
  # If <code>IO::WaitReadable</code> is raised, the caller should wait for reading.
  # If <code>IO::WaitWritable</code> is raised, the caller should wait for writing.
  #
  # So, blocking read (<code>readpartial</code>) can be emulated using
  # <code>read_nonblock</code> and <code>IO.select</code> as follows:
  #
  #   begin
  #     result = io_like.read_nonblock(maxlen)
  #   rescue IO::WaitReadable
  #     IO.select([io_like])
  #     retry
  #   rescue IO::WaitWritable
  #     IO.select(nil, [io_like])
  #     retry
  #   end
  #
  # Especially, the combination of nonblocking methods and
  # <code>IO.select</code> is preferred for <code>IO</code> like
  # objects such as <code>OpenSSL::SSL::SSLSocket</code>.
  # It has <code>to_io</code> method to return underlying <code>IO</code> object.
  # <code>IO.select</code> calls <code>to_io</code> to obtain the file descriptor to wait.
  #
  # This means that readability notified by <code>IO.select</code> doesn't mean
  # readability from <code>OpenSSL::SSL::SSLSocket</code> object.
  #
  # Most possible situation is <code>OpenSSL::SSL::SSLSocket</code> buffers some data.
  # <code>IO.select</code> doesn't see the buffer.
  # So <code>IO.select</code> can block when <code>OpenSSL::SSL::SSLSocket#readpartial</code> doesn't block.
  #
  # However several more complicated situation exists.
  #
  # SSL is a protocol which is sequence of records.
  # The record consists multiple bytes.
  # So, the remote side of SSL sends a partial record,
  # <code>IO.select</code> notifies readability but
  # <code>OpenSSL::SSL::SSLSocket</code> cannot decrypt a byte and
  # <code>OpenSSL::SSL::SSLSocket#readpartial</code> will blocks.
  #
  # Also, the remote side can request SSL renegotiation which forces
  # the local SSL engine writes some data.
  # This means <code>OpenSSL::SSL::SSLSocket#readpartial</code> may
  # invoke <code>write</code> system call and it can block.
  # In such situation, <code>OpenSSL::SSL::SSLSocket#read_nonblock</code>
  # raises IO::WaitWritable instead of blocking.
  # So, the caller should wait for ready for writability as above example.
  #
  # The combination of nonblocking methods and <code>IO.select</code> is
  # also useful for streams such as tty, pipe socket socket when
  # multiple process read form a stream.
  #
  # Finally, Linux kernel developers doesn't guarantee that
  # readability of select(2) means readability of following read(2) even
  # for single process.
  # See select(2) manual on GNU/Linux system.
  #
  # Invoking <code>IO.select</code> before <code>IO#readpartial</code> works well in usual.
  # However it is not the best way to use <code>IO.select</code>.
  #
  # The writability notified by select(2) doesn't show
  # how many bytes writable.
  # <code>IO#write</code> method blocks until given whole string is written.
  # So, <code>IO#write(two or more bytes)</code> can block after writability is notified by <code>IO.select</code>.
  # <code>IO#write_nonblock</code> is required to avoid the blocking.
  #
  # Blocking write (<code>write</code>) can be emulated using
  # <code>write_nonblock</code> and <code>IO.select</code> as follows:
  # IO::WaitReadable should also be rescued for SSL renegotiation in <code>OpenSSL::SSL::SSLSocket</code>.
  #
  #   while 0 < string.bytesize
  #     begin
  #       written = io_like.write_nonblock(string)
  #     rescue IO::WaitReadable
  #       IO.select([io_like])
  #       retry
  #     rescue IO::WaitWritable
  #       IO.select(nil, [io_like])
  #       retry
  #     end
  #     string = string.byteslice(written..-1)
  #   end
  #
  # === Parameters
  # read_array:: an array of <code>IO</code> objects that wait until ready for read
  # write_array:: an array of <code>IO</code> objects that wait until ready for write
  # error_array:: an array of <code>IO</code> objects that wait for exceptions
  # timeout:: a numeric value in second
  #
  # === Example
  #
  #     rp, wp = IO.pipe
  #     mesg = "ping "
  #     100.times {
  #       # IO.select follows IO#read.  Not the best way to use IO.select.
  #       rs, ws, = IO.select([rp], [wp])
  #       if r = rs[0]
  #         ret = r.read(5)
  #         print ret
  #         case ret
  #         when /ping/
  #           mesg = "pong\n"
  #         when /pong/
  #           mesg = "ping "
  #         end
  #       end
  #       if w = ws[0]
  #         w.write(mesg)
  #       end
  #     }
  #
  # <em>produces:</em>
  #
  #     ping pong
  #     ping pong
  #     ping pong
  #     (snipped)
  #     ping
  def select(p1, p2 = v2, p3 = v3, p4 = v4) end

  # Establishes _proc_ as the handler for tracing, or disables
  # tracing if the parameter is +nil+.
  #
  # *Note:* this method is obsolete, please use TracePoint instead.
  #
  # _proc_ takes up to six parameters:
  #
  # *   an event name
  # *   a filename
  # *   a line number
  # *   an object id
  # *   a binding
  # *   the name of a class
  #
  # _proc_ is invoked whenever an event occurs.
  #
  # Events are:
  #
  # +c-call+:: call a C-language routine
  # +c-return+:: return from a C-language routine
  # +call+:: call a Ruby method
  # +class+:: start a class or module definition),
  # +end+:: finish a class or module definition),
  # +line+:: execute code on a new line
  # +raise+:: raise an exception
  # +return+:: return from a Ruby method
  #
  # Tracing is disabled within the context of _proc_.
  #
  #     class Test
  #     def test
  #       a = 1
  #       b = 2
  #     end
  #     end
  #
  #     set_trace_func proc { |event, file, line, id, binding, classname|
  #        printf "%8s %s:%-2d %10s %8s\n", event, file, line, id, classname
  #     }
  #     t = Test.new
  #     t.test
  #
  #       line prog.rb:11               false
  #     c-call prog.rb:11        new    Class
  #     c-call prog.rb:11 initialize   Object
  #   c-return prog.rb:11 initialize   Object
  #   c-return prog.rb:11        new    Class
  #       line prog.rb:12               false
  #       call prog.rb:2        test     Test
  #       line prog.rb:3        test     Test
  #       line prog.rb:4        test     Test
  #     return prog.rb:4        test     Test
  def set_trace_func(*several_variants) end

  # Suspends the current thread for _duration_ seconds (which may be any number,
  # including a +Float+ with fractional seconds). Returns the actual number of
  # seconds slept (rounded), which may be less than that asked for if another
  # thread calls <code>Thread#run</code>. Called without an argument, sleep()
  # will sleep forever.
  #
  #    Time.new    #=> 2008-03-08 19:56:19 +0900
  #    sleep 1.2   #=> 1
  #    Time.new    #=> 2008-03-08 19:56:20 +0900
  #    sleep 1.9   #=> 2
  #    Time.new    #=> 2008-03-08 19:56:22 +0900
  def sleep(*duration) end

  # spawn executes specified command and return its pid.
  #
  #   pid = spawn("tar xf ruby-2.0.0-p195.tar.bz2")
  #   Process.wait pid
  #
  #   pid = spawn(RbConfig.ruby, "-eputs'Hello, world!'")
  #   Process.wait pid
  #
  # This method is similar to Kernel#system but it doesn't wait for the command
  # to finish.
  #
  # The parent process should
  # use <code>Process.wait</code> to collect
  # the termination status of its child or
  # use <code>Process.detach</code> to register
  # disinterest in their status;
  # otherwise, the operating system may accumulate zombie processes.
  #
  # spawn has bunch of options to specify process attributes:
  #
  #   env: hash
  #     name => val : set the environment variable
  #     name => nil : unset the environment variable
  #   command...:
  #     commandline                 : command line string which is passed to the standard shell
  #     cmdname, arg1, ...          : command name and one or more arguments (This form does not use the shell. See below for caveats.)
  #     [cmdname, argv0], arg1, ... : command name, argv[0] and zero or more arguments (no shell)
  #   options: hash
  #     clearing environment variables:
  #       :unsetenv_others => true   : clear environment variables except specified by env
  #       :unsetenv_others => false  : don't clear (default)
  #     process group:
  #       :pgroup => true or 0 : make a new process group
  #       :pgroup => pgid      : join to specified process group
  #       :pgroup => nil       : don't change the process group (default)
  #     create new process group: Windows only
  #       :new_pgroup => true  : the new process is the root process of a new process group
  #       :new_pgroup => false : don't create a new process group (default)
  #     resource limit: resourcename is core, cpu, data, etc.  See Process.setrlimit.
  #       :rlimit_resourcename => limit
  #       :rlimit_resourcename => [cur_limit, max_limit]
  #     umask:
  #       :umask => int
  #     redirection:
  #       key:
  #         FD              : single file descriptor in child process
  #         [FD, FD, ...]   : multiple file descriptor in child process
  #       value:
  #         FD                        : redirect to the file descriptor in parent process
  #         string                    : redirect to file with open(string, "r" or "w")
  #         [string]                  : redirect to file with open(string, File::RDONLY)
  #         [string, open_mode]       : redirect to file with open(string, open_mode, 0644)
  #         [string, open_mode, perm] : redirect to file with open(string, open_mode, perm)
  #         [:child, FD]              : redirect to the redirected file descriptor
  #         :close                    : close the file descriptor in child process
  #       FD is one of follows
  #         :in     : the file descriptor 0 which is the standard input
  #         :out    : the file descriptor 1 which is the standard output
  #         :err    : the file descriptor 2 which is the standard error
  #         integer : the file descriptor of specified the integer
  #         io      : the file descriptor specified as io.fileno
  #     file descriptor inheritance: close non-redirected non-standard fds (3, 4, 5, ...) or not
  #       :close_others => true  : don't inherit
  #     current directory:
  #       :chdir => str
  #
  #     The 'cmdname, arg1, ...' form does not use the shell. However,
  #     on different OSes, different things are provided as built-in
  #     commands. An example of this is 'echo', which is a built-in
  #     on Windows, but is a normal program on Linux and Mac OS X.
  #     This means that `Process.spawn 'echo', '%Path%'` will display
  #     the contents of the `%Path%` environment variable on Windows,
  #     but `Process.spawn 'echo', '$PATH'` prints the literal '$PATH'.
  #
  # If a hash is given as +env+, the environment is
  # updated by +env+ before <code>exec(2)</code> in the child process.
  # If a pair in +env+ has nil as the value, the variable is deleted.
  #
  #   # set FOO as BAR and unset BAZ.
  #   pid = spawn({"FOO"=>"BAR", "BAZ"=>nil}, command)
  #
  # If a hash is given as +options+,
  # it specifies
  # process group,
  # create new process group,
  # resource limit,
  # current directory,
  # umask and
  # redirects for the child process.
  # Also, it can be specified to clear environment variables.
  #
  # The <code>:unsetenv_others</code> key in +options+ specifies
  # to clear environment variables, other than specified by +env+.
  #
  #   pid = spawn(command, :unsetenv_others=>true) # no environment variable
  #   pid = spawn({"FOO"=>"BAR"}, command, :unsetenv_others=>true) # FOO only
  #
  # The <code>:pgroup</code> key in +options+ specifies a process group.
  # The corresponding value should be true, zero or positive integer.
  # true and zero means the process should be a process leader of a new
  # process group.
  # Other values specifies a process group to be belongs.
  #
  #   pid = spawn(command, :pgroup=>true) # process leader
  #   pid = spawn(command, :pgroup=>10) # belongs to the process group 10
  #
  # The <code>:new_pgroup</code> key in +options+ specifies to pass
  # +CREATE_NEW_PROCESS_GROUP+ flag to <code>CreateProcessW()</code> that is
  # Windows API. This option is only for Windows.
  # true means the new process is the root process of the new process group.
  # The new process has CTRL+C disabled. This flag is necessary for
  # <code>Process.kill(:SIGINT, pid)</code> on the subprocess.
  # :new_pgroup is false by default.
  #
  #   pid = spawn(command, :new_pgroup=>true)  # new process group
  #   pid = spawn(command, :new_pgroup=>false) # same process group
  #
  # The <code>:rlimit_</code><em>foo</em> key specifies a resource limit.
  # <em>foo</em> should be one of resource types such as <code>core</code>.
  # The corresponding value should be an integer or an array which have one or
  # two integers: same as cur_limit and max_limit arguments for
  # Process.setrlimit.
  #
  #   cur, max = Process.getrlimit(:CORE)
  #   pid = spawn(command, :rlimit_core=>[0,max]) # disable core temporary.
  #   pid = spawn(command, :rlimit_core=>max) # enable core dump
  #   pid = spawn(command, :rlimit_core=>0) # never dump core.
  #
  # The <code>:umask</code> key in +options+ specifies the umask.
  #
  #   pid = spawn(command, :umask=>077)
  #
  # The :in, :out, :err, a fixnum, an IO and an array key specifies a redirection.
  # The redirection maps a file descriptor in the child process.
  #
  # For example, stderr can be merged into stdout as follows:
  #
  #   pid = spawn(command, :err=>:out)
  #   pid = spawn(command, 2=>1)
  #   pid = spawn(command, STDERR=>:out)
  #   pid = spawn(command, STDERR=>STDOUT)
  #
  # The hash keys specifies a file descriptor
  # in the child process started by <code>spawn</code>.
  # :err, 2 and STDERR specifies the standard error stream (stderr).
  #
  # The hash values specifies a file descriptor
  # in the parent process which invokes <code>spawn</code>.
  # :out, 1 and STDOUT specifies the standard output stream (stdout).
  #
  # In the above example,
  # the standard output in the child process is not specified.
  # So it is inherited from the parent process.
  #
  # The standard input stream (stdin) can be specified by :in, 0 and STDIN.
  #
  # A filename can be specified as a hash value.
  #
  #   pid = spawn(command, :in=>"/dev/null") # read mode
  #   pid = spawn(command, :out=>"/dev/null") # write mode
  #   pid = spawn(command, :err=>"log") # write mode
  #   pid = spawn(command, 3=>"/dev/null") # read mode
  #
  # For stdout and stderr,
  # it is opened in write mode.
  # Otherwise read mode is used.
  #
  # For specifying flags and permission of file creation explicitly,
  # an array is used instead.
  #
  #   pid = spawn(command, :in=>["file"]) # read mode is assumed
  #   pid = spawn(command, :in=>["file", "r"])
  #   pid = spawn(command, :out=>["log", "w"]) # 0644 assumed
  #   pid = spawn(command, :out=>["log", "w", 0600])
  #   pid = spawn(command, :out=>["log", File::WRONLY|File::EXCL|File::CREAT, 0600])
  #
  # The array specifies a filename, flags and permission.
  # The flags can be a string or an integer.
  # If the flags is omitted or nil, File::RDONLY is assumed.
  # The permission should be an integer.
  # If the permission is omitted or nil, 0644 is assumed.
  #
  # If an array of IOs and integers are specified as a hash key,
  # all the elements are redirected.
  #
  #   # stdout and stderr is redirected to log file.
  #   # The file "log" is opened just once.
  #   pid = spawn(command, [:out, :err]=>["log", "w"])
  #
  # Another way to merge multiple file descriptors is [:child, fd].
  # \[:child, fd] means the file descriptor in the child process.
  # This is different from fd.
  # For example, :err=>:out means redirecting child stderr to parent stdout.
  # But :err=>[:child, :out] means redirecting child stderr to child stdout.
  # They differ if stdout is redirected in the child process as follows.
  #
  #   # stdout and stderr is redirected to log file.
  #   # The file "log" is opened just once.
  #   pid = spawn(command, :out=>["log", "w"], :err=>[:child, :out])
  #
  # \[:child, :out] can be used to merge stderr into stdout in IO.popen.
  # In this case, IO.popen redirects stdout to a pipe in the child process
  # and [:child, :out] refers the redirected stdout.
  #
  #   io = IO.popen(["sh", "-c", "echo out; echo err >&2", :err=>[:child, :out]])
  #   p io.read #=> "out\nerr\n"
  #
  # The <code>:chdir</code> key in +options+ specifies the current directory.
  #
  #   pid = spawn(command, :chdir=>"/var/tmp")
  #
  # spawn closes all non-standard unspecified descriptors by default.
  # The "standard" descriptors are 0, 1 and 2.
  # This behavior is specified by :close_others option.
  # :close_others doesn't affect the standard descriptors which are
  # closed only if :close is specified explicitly.
  #
  #   pid = spawn(command, :close_others=>true)  # close 3,4,5,... (default)
  #   pid = spawn(command, :close_others=>false) # don't close 3,4,5,...
  #
  # :close_others is true by default for spawn and IO.popen.
  #
  # Note that fds which close-on-exec flag is already set are closed
  # regardless of :close_others option.
  #
  # So IO.pipe and spawn can be used as IO.popen.
  #
  #   # similar to r = IO.popen(command)
  #   r, w = IO.pipe
  #   pid = spawn(command, :out=>w)   # r, w is closed in the child process.
  #   w.close
  #
  # :close is specified as a hash value to close a fd individually.
  #
  #   f = open(foo)
  #   system(command, f=>:close)        # don't inherit f.
  #
  # If a file descriptor need to be inherited,
  # io=>io can be used.
  #
  #   # valgrind has --log-fd option for log destination.
  #   # log_w=>log_w indicates log_w.fileno inherits to child process.
  #   log_r, log_w = IO.pipe
  #   pid = spawn("valgrind", "--log-fd=#{log_w.fileno}", "echo", "a", log_w=>log_w)
  #   log_w.close
  #   p log_r.read
  #
  # It is also possible to exchange file descriptors.
  #
  #   pid = spawn(command, :out=>:err, :err=>:out)
  #
  # The hash keys specify file descriptors in the child process.
  # The hash values specifies file descriptors in the parent process.
  # So the above specifies exchanging stdout and stderr.
  # Internally, +spawn+ uses an extra file descriptor to resolve such cyclic
  # file descriptor mapping.
  #
  # See <code>Kernel.exec</code> for the standard shell.
  def spawn(*args) end

  # Returns the string resulting from applying <i>format_string</i> to
  # any additional arguments.  Within the format string, any characters
  # other than format sequences are copied to the result.
  #
  # The syntax of a format sequence is follows.
  #
  #   %[flags][width][.precision]type
  #
  # A format
  # sequence consists of a percent sign, followed by optional flags,
  # width, and precision indicators, then terminated with a field type
  # character.  The field type controls how the corresponding
  # <code>sprintf</code> argument is to be interpreted, while the flags
  # modify that interpretation.
  #
  # The field type characters are:
  #
  #     Field |  Integer Format
  #     ------+--------------------------------------------------------------
  #       b   | Convert argument as a binary number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..1'.
  #       B   | Equivalent to `b', but uses an uppercase 0B for prefix
  #           | in the alternative format by #.
  #       d   | Convert argument as a decimal number.
  #       i   | Identical to `d'.
  #       o   | Convert argument as an octal number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..7'.
  #       u   | Identical to `d'.
  #       x   | Convert argument as a hexadecimal number.
  #           | Negative numbers will be displayed as a two's complement
  #           | prefixed with `..f' (representing an infinite string of
  #           | leading 'ff's).
  #       X   | Equivalent to `x', but uses uppercase letters.
  #
  #     Field |  Float Format
  #     ------+--------------------------------------------------------------
  #       e   | Convert floating point argument into exponential notation
  #           | with one digit before the decimal point as [-]d.dddddde[+-]dd.
  #           | The precision specifies the number of digits after the decimal
  #           | point (defaulting to six).
  #       E   | Equivalent to `e', but uses an uppercase E to indicate
  #           | the exponent.
  #       f   | Convert floating point argument as [-]ddd.dddddd,
  #           | where the precision specifies the number of digits after
  #           | the decimal point.
  #       g   | Convert a floating point number using exponential form
  #           | if the exponent is less than -4 or greater than or
  #           | equal to the precision, or in dd.dddd form otherwise.
  #           | The precision specifies the number of significant digits.
  #       G   | Equivalent to `g', but use an uppercase `E' in exponent form.
  #       a   | Convert floating point argument as [-]0xh.hhhhp[+-]dd,
  #           | which is consisted from optional sign, "0x", fraction part
  #           | as hexadecimal, "p", and exponential part as decimal.
  #       A   | Equivalent to `a', but use uppercase `X' and `P'.
  #
  #     Field |  Other Format
  #     ------+--------------------------------------------------------------
  #       c   | Argument is the numeric code for a single character or
  #           | a single character string itself.
  #       p   | The valuing of argument.inspect.
  #       s   | Argument is a string to be substituted.  If the format
  #           | sequence contains a precision, at most that many characters
  #           | will be copied.
  #       %   | A percent sign itself will be displayed.  No argument taken.
  #
  # The flags modifies the behavior of the formats.
  # The flag characters are:
  #
  #   Flag     | Applies to    | Meaning
  #   ---------+---------------+-----------------------------------------
  #   space    | bBdiouxX      | Leave a space at the start of
  #            | aAeEfgG       | non-negative numbers.
  #            | (numeric fmt) | For `o', `x', `X', `b' and `B', use
  #            |               | a minus sign with absolute value for
  #            |               | negative values.
  #   ---------+---------------+-----------------------------------------
  #   (digit)$ | all           | Specifies the absolute argument number
  #            |               | for this field.  Absolute and relative
  #            |               | argument numbers cannot be mixed in a
  #            |               | sprintf string.
  #   ---------+---------------+-----------------------------------------
  #    #       | bBoxX         | Use an alternative format.
  #            | aAeEfgG       | For the conversions `o', increase the precision
  #            |               | until the first digit will be `0' if
  #            |               | it is not formatted as complements.
  #            |               | For the conversions `x', `X', `b' and `B'
  #            |               | on non-zero, prefix the result with ``0x'',
  #            |               | ``0X'', ``0b'' and ``0B'', respectively.
  #            |               | For `a', `A', `e', `E', `f', `g', and 'G',
  #            |               | force a decimal point to be added,
  #            |               | even if no digits follow.
  #            |               | For `g' and 'G', do not remove trailing zeros.
  #   ---------+---------------+-----------------------------------------
  #   +        | bBdiouxX      | Add a leading plus sign to non-negative
  #            | aAeEfgG       | numbers.
  #            | (numeric fmt) | For `o', `x', `X', `b' and `B', use
  #            |               | a minus sign with absolute value for
  #            |               | negative values.
  #   ---------+---------------+-----------------------------------------
  #   -        | all           | Left-justify the result of this conversion.
  #   ---------+---------------+-----------------------------------------
  #   0 (zero) | bBdiouxX      | Pad with zeros, not spaces.
  #            | aAeEfgG       | For `o', `x', `X', `b' and `B', radix-1
  #            | (numeric fmt) | is used for negative numbers formatted as
  #            |               | complements.
  #   ---------+---------------+-----------------------------------------
  #   *        | all           | Use the next argument as the field width.
  #            |               | If negative, left-justify the result. If the
  #            |               | asterisk is followed by a number and a dollar
  #            |               | sign, use the indicated argument as the width.
  #
  # Examples of flags:
  #
  #  # `+' and space flag specifies the sign of non-negative numbers.
  #  sprintf("%d", 123)  #=> "123"
  #  sprintf("%+d", 123) #=> "+123"
  #  sprintf("% d", 123) #=> " 123"
  #
  #  # `#' flag for `o' increases number of digits to show `0'.
  #  # `+' and space flag changes format of negative numbers.
  #  sprintf("%o", 123)   #=> "173"
  #  sprintf("%#o", 123)  #=> "0173"
  #  sprintf("%+o", -123) #=> "-173"
  #  sprintf("%o", -123)  #=> "..7605"
  #  sprintf("%#o", -123) #=> "..7605"
  #
  #  # `#' flag for `x' add a prefix `0x' for non-zero numbers.
  #  # `+' and space flag disables complements for negative numbers.
  #  sprintf("%x", 123)   #=> "7b"
  #  sprintf("%#x", 123)  #=> "0x7b"
  #  sprintf("%+x", -123) #=> "-7b"
  #  sprintf("%x", -123)  #=> "..f85"
  #  sprintf("%#x", -123) #=> "0x..f85"
  #  sprintf("%#x", 0)    #=> "0"
  #
  #  # `#' for `X' uses the prefix `0X'.
  #  sprintf("%X", 123)  #=> "7B"
  #  sprintf("%#X", 123) #=> "0X7B"
  #
  #  # `#' flag for `b' add a prefix `0b' for non-zero numbers.
  #  # `+' and space flag disables complements for negative numbers.
  #  sprintf("%b", 123)   #=> "1111011"
  #  sprintf("%#b", 123)  #=> "0b1111011"
  #  sprintf("%+b", -123) #=> "-1111011"
  #  sprintf("%b", -123)  #=> "..10000101"
  #  sprintf("%#b", -123) #=> "0b..10000101"
  #  sprintf("%#b", 0)    #=> "0"
  #
  #  # `#' for `B' uses the prefix `0B'.
  #  sprintf("%B", 123)  #=> "1111011"
  #  sprintf("%#B", 123) #=> "0B1111011"
  #
  #  # `#' for `e' forces to show the decimal point.
  #  sprintf("%.0e", 1)  #=> "1e+00"
  #  sprintf("%#.0e", 1) #=> "1.e+00"
  #
  #  # `#' for `f' forces to show the decimal point.
  #  sprintf("%.0f", 1234)  #=> "1234"
  #  sprintf("%#.0f", 1234) #=> "1234."
  #
  #  # `#' for `g' forces to show the decimal point.
  #  # It also disables stripping lowest zeros.
  #  sprintf("%g", 123.4)   #=> "123.4"
  #  sprintf("%#g", 123.4)  #=> "123.400"
  #  sprintf("%g", 123456)  #=> "123456"
  #  sprintf("%#g", 123456) #=> "123456."
  #
  # The field width is an optional integer, followed optionally by a
  # period and a precision.  The width specifies the minimum number of
  # characters that will be written to the result for this field.
  #
  # Examples of width:
  #
  #  # padding is done by spaces,       width=20
  #  # 0 or radix-1.             <------------------>
  #  sprintf("%20d", 123)   #=> "                 123"
  #  sprintf("%+20d", 123)  #=> "                +123"
  #  sprintf("%020d", 123)  #=> "00000000000000000123"
  #  sprintf("%+020d", 123) #=> "+0000000000000000123"
  #  sprintf("% 020d", 123) #=> " 0000000000000000123"
  #  sprintf("%-20d", 123)  #=> "123                 "
  #  sprintf("%-+20d", 123) #=> "+123                "
  #  sprintf("%- 20d", 123) #=> " 123                "
  #  sprintf("%020x", -123) #=> "..ffffffffffffffff85"
  #
  # For
  # numeric fields, the precision controls the number of decimal places
  # displayed.  For string fields, the precision determines the maximum
  # number of characters to be copied from the string.  (Thus, the format
  # sequence <code>%10.10s</code> will always contribute exactly ten
  # characters to the result.)
  #
  # Examples of precisions:
  #
  #  # precision for `d', 'o', 'x' and 'b' is
  #  # minimum number of digits               <------>
  #  sprintf("%20.8d", 123)  #=> "            00000123"
  #  sprintf("%20.8o", 123)  #=> "            00000173"
  #  sprintf("%20.8x", 123)  #=> "            0000007b"
  #  sprintf("%20.8b", 123)  #=> "            01111011"
  #  sprintf("%20.8d", -123) #=> "           -00000123"
  #  sprintf("%20.8o", -123) #=> "            ..777605"
  #  sprintf("%20.8x", -123) #=> "            ..ffff85"
  #  sprintf("%20.8b", -11)  #=> "            ..110101"
  #
  #  # "0x" and "0b" for `#x' and `#b' is not counted for
  #  # precision but "0" for `#o' is counted.  <------>
  #  sprintf("%#20.8d", 123)  #=> "            00000123"
  #  sprintf("%#20.8o", 123)  #=> "            00000173"
  #  sprintf("%#20.8x", 123)  #=> "          0x0000007b"
  #  sprintf("%#20.8b", 123)  #=> "          0b01111011"
  #  sprintf("%#20.8d", -123) #=> "           -00000123"
  #  sprintf("%#20.8o", -123) #=> "            ..777605"
  #  sprintf("%#20.8x", -123) #=> "          0x..ffff85"
  #  sprintf("%#20.8b", -11)  #=> "          0b..110101"
  #
  #  # precision for `e' is number of
  #  # digits after the decimal point           <------>
  #  sprintf("%20.8e", 1234.56789) #=> "      1.23456789e+03"
  #
  #  # precision for `f' is number of
  #  # digits after the decimal point               <------>
  #  sprintf("%20.8f", 1234.56789) #=> "       1234.56789000"
  #
  #  # precision for `g' is number of
  #  # significant digits                          <------->
  #  sprintf("%20.8g", 1234.56789) #=> "           1234.5679"
  #
  #  #                                         <------->
  #  sprintf("%20.8g", 123456789)  #=> "       1.2345679e+08"
  #
  #  # precision for `s' is
  #  # maximum number of characters                    <------>
  #  sprintf("%20.8s", "string test") #=> "            string t"
  #
  # Examples:
  #
  #    sprintf("%d %04x", 123, 123)               #=> "123 007b"
  #    sprintf("%08b '%4s'", 123, 123)            #=> "01111011 ' 123'"
  #    sprintf("%1$*2$s %2$d %1$s", "hello", 8)   #=> "   hello 8 hello"
  #    sprintf("%1$*2$s %2$d", "hello", -8)       #=> "hello    -8"
  #    sprintf("%+g:% g:%-g", 1.23, 1.23, 1.23)   #=> "+1.23: 1.23:1.23"
  #    sprintf("%u", -123)                        #=> "-123"
  #
  # For more complex formatting, Ruby supports a reference by name.
  # %<name>s style uses format style, but %{name} style doesn't.
  #
  # Examples:
  #   sprintf("%<foo>d : %<bar>f", { :foo => 1, :bar => 2 })
  #     #=> 1 : 2.000000
  #   sprintf("%{foo}f", { :foo => 1 })
  #     # => "1f"
  def sprintf(format_string, *args) end
  alias format sprintf

  # Seeds the system pseudo-random number generator, Random::DEFAULT, with
  # +number+.  The previous seed value is returned.
  #
  # If +number+ is omitted, seeds the generator using a source of entropy
  # provided by the operating system, if available (/dev/urandom on Unix systems
  # or the RSA cryptographic provider on Windows), which is then combined with
  # the time, the process id, and a sequence number.
  #
  # srand may be used to ensure repeatable sequences of pseudo-random numbers
  # between different runs of the program. By setting the seed to a known value,
  # programs can be made deterministic during testing.
  #
  #   srand 1234               # => 268519324636777531569100071560086917274
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  #   [ rand(10), rand(1000) ] # => [4, 664]
  #   srand 1234               # => 1234
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  def srand(number = Random.new_seed) end

  # Equivalent to <code>$_.sub(<i>args</i>)</code>, except that
  # <code>$_</code> will be updated if substitution occurs.
  # Available only when -p/-n command line option specified.
  def sub(*several_variants) end

  #  Calls the operating system function identified by _num_ and
  #  returns the result of the function or raises SystemCallError if
  #  it failed.
  #
  #  Arguments for the function can follow _num_. They must be either
  #  +String+ objects or +Integer+ objects. A +String+ object is passed
  #  as a pointer to the byte sequence. An +Integer+ object is passed
  #  as an integer whose bit size is same as a pointer.
  #  Up to nine parameters may be passed (14 on the Atari-ST).
  #
  #  The function identified by _num_ is system
  #  dependent. On some Unix systems, the numbers may be obtained from a
  #  header file called <code>syscall.h</code>.
  #
  #     syscall 4, 1, "hello\n", 6   # '4' is write(2) on our box
  #
  #  <em>produces:</em>
  #
  #     hello
  #
  #  Calling +syscall+ on a platform which does not have any way to
  #  an arbitrary system function just fails with NotImplementedError.
  #
  # Note::
  #   +syscall+ is essentially unsafe and unportable. Feel free to shoot your foot.
  #   DL (Fiddle) library is preferred for safer and a bit more portable programming.
  def syscall(*args) end

  # Executes _command..._ in a subshell.
  # _command..._ is one of following forms.
  #
  #   commandline                 : command line string which is passed to the standard shell
  #   cmdname, arg1, ...          : command name and one or more arguments (no shell)
  #   [cmdname, argv0], arg1, ... : command name, argv[0] and zero or more arguments (no shell)
  #
  # system returns +true+ if the command gives zero exit status,
  # +false+ for non zero exit status.
  # Returns +nil+ if command execution fails.
  # An error status is available in <code>$?</code>.
  # The arguments are processed in the same way as
  # for <code>Kernel.spawn</code>.
  #
  # The hash arguments, env and options, are same as
  # <code>exec</code> and <code>spawn</code>.
  # See <code>Kernel.spawn</code> for details.
  #
  #    system("echo *")
  #    system("echo", "*")
  #
  # <em>produces:</em>
  #
  #    config.h main.rb
  #    *
  #
  # See <code>Kernel.exec</code> for the standard shell.
  def system(*args) end

  # Uses the integer +cmd+ to perform various tests on +file1+ (first
  # table below) or on +file1+ and +file2+ (second table).
  #
  # File tests on a single file:
  #
  #   Cmd    Returns   Meaning
  #   "A"  | Time    | Last access time for file1
  #   "b"  | boolean | True if file1 is a block device
  #   "c"  | boolean | True if file1 is a character device
  #   "C"  | Time    | Last change time for file1
  #   "d"  | boolean | True if file1 exists and is a directory
  #   "e"  | boolean | True if file1 exists
  #   "f"  | boolean | True if file1 exists and is a regular file
  #   "g"  | boolean | True if file1 has the \CF{setgid} bit
  #        |         | set (false under NT)
  #   "G"  | boolean | True if file1 exists and has a group
  #        |         | ownership equal to the caller's group
  #   "k"  | boolean | True if file1 exists and has the sticky bit set
  #   "l"  | boolean | True if file1 exists and is a symbolic link
  #   "M"  | Time    | Last modification time for file1
  #   "o"  | boolean | True if file1 exists and is owned by
  #        |         | the caller's effective uid
  #   "O"  | boolean | True if file1 exists and is owned by
  #        |         | the caller's real uid
  #   "p"  | boolean | True if file1 exists and is a fifo
  #   "r"  | boolean | True if file1 is readable by the effective
  #        |         | uid/gid of the caller
  #   "R"  | boolean | True if file is readable by the real
  #        |         | uid/gid of the caller
  #   "s"  | int/nil | If file1 has nonzero size, return the size,
  #        |         | otherwise return nil
  #   "S"  | boolean | True if file1 exists and is a socket
  #   "u"  | boolean | True if file1 has the setuid bit set
  #   "w"  | boolean | True if file1 exists and is writable by
  #        |         | the effective uid/gid
  #   "W"  | boolean | True if file1 exists and is writable by
  #        |         | the real uid/gid
  #   "x"  | boolean | True if file1 exists and is executable by
  #        |         | the effective uid/gid
  #   "X"  | boolean | True if file1 exists and is executable by
  #        |         | the real uid/gid
  #   "z"  | boolean | True if file1 exists and has a zero length
  #
  # Tests that take two files:
  #
  #   "-"  | boolean | True if file1 and file2 are identical
  #   "="  | boolean | True if the modification times of file1
  #        |         | and file2 are equal
  #   "<"  | boolean | True if the modification time of file1
  #        |         | is prior to that of file2
  #   ">"  | boolean | True if the modification time of file1
  #        |         | is after that of file2
  def test(*args) end

  # Transfers control to the end of the active +catch+ block
  # waiting for _tag_. Raises +ArgumentError+ if there
  # is no +catch+ block for the _tag_. The optional second
  # parameter supplies a return value for the +catch+ block,
  # which otherwise defaults to +nil+. For examples, see
  # <code>Kernel::catch</code>.
  def throw(p1, p2 = v2) end

  # Controls tracing of assignments to global variables. The parameter
  # +symbol_ identifies the variable (as either a string name or a
  # symbol identifier). _cmd_ (which may be a string or a
  # +Proc+ object) or block is executed whenever the variable
  # is assigned. The block or +Proc+ object receives the
  # variable's new value as a parameter. Also see
  # <code>Kernel::untrace_var</code>.
  #
  #    trace_var :$_, proc {|v| puts "$_ is now '#{v}'" }
  #    $_ = "hello"
  #    $_ = ' there'
  #
  # <em>produces:</em>
  #
  #    $_ is now 'hello'
  #    $_ is now ' there'
  def trace_var(*several_variants) end

  # Specifies the handling of signals. The first parameter is a signal
  # name (a string such as ``SIGALRM'', ``SIGUSR1'', and so on) or a
  # signal number. The characters ``SIG'' may be omitted from the
  # signal name. The command or block specifies code to be run when the
  # signal is raised.
  # If the command is the string ``IGNORE'' or ``SIG_IGN'', the signal
  # will be ignored.
  # If the command is ``DEFAULT'' or ``SIG_DFL'', the Ruby's default handler
  # will be invoked.
  # If the command is ``EXIT'', the script will be terminated by the signal.
  # If the command is ``SYSTEM_DEFAULT'', the operating system's default
  # handler will be invoked.
  # Otherwise, the given command or block will be run.
  # The special signal name ``EXIT'' or signal number zero will be
  # invoked just prior to program termination.
  # trap returns the previous handler for the given signal.
  #
  #     Signal.trap(0, proc { puts "Terminating: #{$$}" })
  #     Signal.trap("CLD")  { puts "Child died" }
  #     fork && Process.wait
  #
  # produces:
  #     Terminating: 27461
  #     Child died
  #     Terminating: 27460
  def trap(*several_variants) end

  # Removes tracing for the specified command on the given global
  # variable and returns +nil+. If no command is specified,
  # removes all tracing for that variable and returns an array
  # containing the commands actually removed.
  def untrace_var(symbol, *cmd) end

  # Displays each of the given messages followed by a record separator on
  # STDERR unless warnings have been disabled (for example with the
  # <code>-W0</code> flag).
  #
  #    warn("warning 1", "warning 2")
  #
  #  <em>produces:</em>
  #
  #    warning 1
  #    warning 2
  def warn(msg, *args) end
end
