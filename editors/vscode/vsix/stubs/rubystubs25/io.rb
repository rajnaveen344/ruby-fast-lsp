# frozen_string_literal: true

# The IO class is the basis for all input and output in Ruby.
# An I/O stream may be <em>duplexed</em> (that is, bidirectional), and
# so may use more than one native operating system stream.
#
# Many of the examples in this section use the File class, the only standard
# subclass of IO. The two classes are closely associated.  Like the File
# class, the Socket library subclasses from IO (such as TCPSocket or
# UDPSocket).
#
# The Kernel#open method can create an IO (or File) object for these types
# of arguments:
#
# * A plain string represents a filename suitable for the underlying
#   operating system.
#
# * A string starting with <code>"|"</code> indicates a subprocess.
#   The remainder of the string following the <code>"|"</code> is
#   invoked as a process with appropriate input/output channels
#   connected to it.
#
# * A string equal to <code>"|-"</code> will create another Ruby
#   instance as a subprocess.
#
# The IO may be opened with different file modes (read-only, write-only) and
# encodings for proper conversion.  See IO.new for these options.  See
# Kernel#open for details of the various command formats described above.
#
# IO.popen, the Open3 library, or  Process#spawn may also be used to
# communicate with subprocesses through an IO.
#
# Ruby will convert pathnames between different operating system
# conventions if possible.  For instance, on a Windows system the
# filename <code>"/gumby/ruby/test.rb"</code> will be opened as
# <code>"\gumby\ruby\test.rb"</code>.  When specifying a Windows-style
# filename in a Ruby string, remember to escape the backslashes:
#
#   "C:\\gumby\\ruby\\test.rb"
#
# Our examples here will use the Unix-style forward slashes;
# File::ALT_SEPARATOR can be used to get the platform-specific separator
# character.
#
# The global constant ARGF (also accessible as <code>$<</code>) provides an
# IO-like stream which allows access to all files mentioned on the
# command line (or STDIN if no files are mentioned). ARGF#path and its alias
# ARGF#filename are provided to access the name of the file currently being
# read.
#
# == io/console
#
# The io/console extension provides methods for interacting with the
# console.  The console can be accessed from IO.console or the standard
# input/output/error IO objects.
#
# Requiring io/console adds the following methods:
#
# * IO::console
# * IO#raw
# * IO#raw!
# * IO#cooked
# * IO#cooked!
# * IO#getch
# * IO#echo=
# * IO#echo?
# * IO#noecho
# * IO#winsize
# * IO#winsize=
# * IO#iflush
# * IO#ioflush
# * IO#oflush
#
# Example:
#
#   require 'io/console'
#   rows, columns = $stdout.winsize
#   puts "Your screen is #{columns} wide and #{rows} tall"
class IO
  include Enumerable
  include File::Constants

  # same as IO::EAGAINWaitReadable
  EWOULDBLOCKWaitReadable = _
  # same as IO::EAGAINWaitWritable
  EWOULDBLOCKWaitWritable = _
  # Set I/O position from the current position
  SEEK_CUR = _
  # Set I/O position to the next location containing data
  SEEK_DATA = _
  # Set I/O position from the end
  SEEK_END = _
  # Set I/O position to the next hole
  SEEK_HOLE = _
  # Set I/O position from the beginning
  SEEK_SET = _

  # Opens the file, optionally seeks to the given <i>offset</i>, then returns
  # <i>length</i> bytes (defaulting to the rest of the file).
  # <code>binread</code> ensures the file is closed before returning.
  # The open mode would be "rb:ASCII-8BIT".
  #
  #    IO.binread("testfile")           #=> "This is line one\nThis is line two\nThis is line three\nAnd so on...\n"
  #    IO.binread("testfile", 20)       #=> "This is line one\nThi"
  #    IO.binread("testfile", 20, 10)   #=> "ne one\nThis is line "
  def self.binread(p1, p2 = v2, p3 = v3) end

  # Same as <code>IO.write</code> except opening the file in binary mode
  # and ASCII-8BIT encoding ("wb:ASCII-8BIT").
  def self.binwrite(*several_variants) end

  # Returns an File instance opened console.
  #
  # If +sym+ is given, it will be sent to the opened console with
  # +args+ and the result will be returned instead of the console IO
  # itself.
  #
  # You must require 'io/console' to use this method.
  def self.console(*several_variants) end

  # IO.copy_stream copies <i>src</i> to <i>dst</i>.
  # <i>src</i> and <i>dst</i> is either a filename or an IO-like object.
  # IO-like object for <i>src</i> should have <code>readpartial</code> or
  # <code>read</code> method.
  # IO-like object for <i>dst</i> should have <code>write</code> method.
  # (Specialized mechanisms, such as sendfile system call, may be used
  # on appropriate situation.)
  #
  # This method returns the number of bytes copied.
  #
  # If optional arguments are not given,
  # the start position of the copy is
  # the beginning of the filename or
  # the current file offset of the IO.
  # The end position of the copy is the end of file.
  #
  # If <i>copy_length</i> is given,
  # No more than <i>copy_length</i> bytes are copied.
  #
  # If <i>src_offset</i> is given,
  # it specifies the start position of the copy.
  #
  # When <i>src_offset</i> is specified and
  # <i>src</i> is an IO,
  # IO.copy_stream doesn't move the current file offset.
  def self.copy_stream(*several_variants) end

  # Synonym for <code>IO.new</code>.
  def self.for_fd(*args) end

  # Executes the block for every line in the named I/O port, where lines
  # are separated by <em>sep</em>.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    IO.foreach("testfile") {|x| print "GOT ", x }
  #
  # <em>produces:</em>
  #
  #    GOT This is line one
  #    GOT This is line two
  #    GOT This is line three
  #    GOT And so on...
  #
  # If the last argument is a hash, it's the keyword argument to open.
  # See IO.readlines for details about getline_args.
  # And see also IO.read for details about open_args.
  def self.foreach(*several_variants) end

  # With no associated block, <code>IO.open</code> is a synonym for IO.new.  If
  # the optional code block is given, it will be passed +io+ as an argument,
  # and the IO object will automatically be closed when the block terminates.
  # In this instance, IO.open returns the value of the block.
  #
  # See IO.new for a description of the +fd+, +mode+ and +opt+ parameters.
  def self.open(*args) end

  # Creates a pair of pipe endpoints (connected to each other) and
  # returns them as a two-element array of <code>IO</code> objects:
  # <code>[</code> <i>read_io</i>, <i>write_io</i> <code>]</code>.
  #
  # If a block is given, the block is called and
  # returns the value of the block.
  # <i>read_io</i> and <i>write_io</i> are sent to the block as arguments.
  # If read_io and write_io are not closed when the block exits, they are closed.
  # i.e. closing read_io and/or write_io doesn't cause an error.
  #
  # Not available on all platforms.
  #
  # If an encoding (encoding name or encoding object) is specified as an optional argument,
  # read string from pipe is tagged with the encoding specified.
  # If the argument is a colon separated two encoding names "A:B",
  # the read string is converted from encoding A (external encoding)
  # to encoding B (internal encoding), then tagged with B.
  # If two optional arguments are specified, those must be
  # encoding objects or encoding names,
  # and the first one is the external encoding,
  # and the second one is the internal encoding.
  # If the external encoding and the internal encoding is specified,
  # optional hash argument specify the conversion option.
  #
  # In the example below, the two processes close the ends of the pipe
  # that they are not using. This is not just a cosmetic nicety. The
  # read end of a pipe will not generate an end of file condition if
  # there are any writers with the pipe still open. In the case of the
  # parent process, the <code>rd.read</code> will never return if it
  # does not first issue a <code>wr.close</code>.
  #
  #    rd, wr = IO.pipe
  #
  #    if fork
  #      wr.close
  #      puts "Parent got: <#{rd.read}>"
  #      rd.close
  #      Process.wait
  #    else
  #      rd.close
  #      puts "Sending message to parent"
  #      wr.write "Hi Dad"
  #      wr.close
  #    end
  #
  # <em>produces:</em>
  #
  #    Sending message to parent
  #    Parent got: <Hi Dad>
  def self.pipe(*several_variants) end

  # Runs the specified command as a subprocess; the subprocess's
  # standard input and output will be connected to the returned
  # <code>IO</code> object.
  #
  # The PID of the started process can be obtained by IO#pid method.
  #
  # _cmd_ is a string or an array as follows.
  #
  #   cmd:
  #     "-"                                      : fork
  #     commandline                              : command line string which is passed to a shell
  #     [env, cmdname, arg1, ..., opts]          : command name and zero or more arguments (no shell)
  #     [env, [cmdname, argv0], arg1, ..., opts] : command name, argv[0] and zero or more arguments (no shell)
  #   (env and opts are optional.)
  #
  # If _cmd_ is a +String+ ``<code>-</code>'',
  # then a new instance of Ruby is started as the subprocess.
  #
  # If <i>cmd</i> is an +Array+ of +String+,
  # then it will be used as the subprocess's +argv+ bypassing a shell.
  # The array can contain a hash at first for environments and
  # a hash at last for options similar to <code>spawn</code>.
  #
  # The default mode for the new file object is ``r'',
  # but <i>mode</i> may be set to any of the modes listed in the description for class IO.
  # The last argument <i>opt</i> qualifies <i>mode</i>.
  #
  #   # set IO encoding
  #   IO.popen("nkf -e filename", :external_encoding=>"EUC-JP") {|nkf_io|
  #     euc_jp_string = nkf_io.read
  #   }
  #
  #   # merge standard output and standard error using
  #   # spawn option.  See the document of Kernel.spawn.
  #   IO.popen(["ls", "/", :err=>[:child, :out]]) {|ls_io|
  #     ls_result_with_error = ls_io.read
  #   }
  #
  #   # spawn options can be mixed with IO options
  #   IO.popen(["ls", "/"], :err=>[:child, :out]) {|ls_io|
  #     ls_result_with_error = ls_io.read
  #   }
  #
  # Raises exceptions which <code>IO.pipe</code> and
  # <code>Kernel.spawn</code> raise.
  #
  # If a block is given, Ruby will run the command as a child connected
  # to Ruby with a pipe. Ruby's end of the pipe will be passed as a
  # parameter to the block.
  # At the end of block, Ruby closes the pipe and sets <code>$?</code>.
  # In this case <code>IO.popen</code> returns
  # the value of the block.
  #
  # If a block is given with a _cmd_ of ``<code>-</code>'',
  # the block will be run in two separate processes: once in the parent,
  # and once in a child. The parent process will be passed the pipe
  # object as a parameter to the block, the child version of the block
  # will be passed +nil+, and the child's standard in and
  # standard out will be connected to the parent through the pipe. Not
  # available on all platforms.
  #
  #    f = IO.popen("uname")
  #    p f.readlines
  #    f.close
  #    puts "Parent is #{Process.pid}"
  #    IO.popen("date") {|f| puts f.gets }
  #    IO.popen("-") {|f| $stderr.puts "#{Process.pid} is here, f is #{f.inspect}"}
  #    p $?
  #    IO.popen(%w"sed -e s|^|<foo>| -e s&$&;zot;&", "r+") {|f|
  #      f.puts "bar"; f.close_write; puts f.gets
  #    }
  #
  # <em>produces:</em>
  #
  #    ["Linux\n"]
  #    Parent is 21346
  #    Thu Jan 15 22:41:19 JST 2009
  #    21346 is here, f is #<IO:fd 3>
  #    21352 is here, f is nil
  #    #<Process::Status: pid 21352 exit 0>
  #    <foo>bar;zot;
  def self.popen(*args) end

  # Opens the file, optionally seeks to the given +offset+, then returns
  # +length+ bytes (defaulting to the rest of the file).  <code>read</code>
  # ensures the file is closed before returning.
  #
  # If +name+ starts with a pipe character (<code>"|"</code>), a subprocess is
  # created in the same way as Kernel#open, and its output is returned.
  #
  # === Options
  #
  # The options hash accepts the following keys:
  #
  # :encoding::
  #   string or encoding
  #
  #   Specifies the encoding of the read string.  +:encoding+ will be ignored
  #   if +length+ is specified.  See Encoding.aliases for possible encodings.
  #
  # :mode::
  #   string or integer
  #
  #   Specifies the <i>mode</i> argument for open().  It must start
  #   with an "r", otherwise it will cause an error.
  #   See IO.new for the list of possible modes.
  #
  # :open_args::
  #   array
  #
  #   Specifies arguments for open() as an array.  This key can not be used
  #   in combination with either +:encoding+ or +:mode+.
  #
  # Examples:
  #
  #   IO.read("testfile")              #=> "This is line one\nThis is line two\nThis is line three\nAnd so on...\n"
  #   IO.read("testfile", 20)          #=> "This is line one\nThi"
  #   IO.read("testfile", 20, 10)      #=> "ne one\nThis is line "
  #   IO.read("binfile", mode: "rb")   #=> "\xF7\x00\x00\x0E\x12"
  def self.read(p1, p2 = v2, p3 = v3, p4 = v4, p5 = {}) end

  # Reads the entire file specified by <i>name</i> as individual
  # lines, and returns those lines in an array. Lines are separated by
  # <i>sep</i>.
  #
  #    a = IO.readlines("testfile")
  #    a[0]   #=> "This is line one\n"
  #
  #    b = IO.readlines("testfile", chomp: true)
  #    b[0]   #=> "This is line one"
  #
  # If the last argument is a hash, it's the keyword argument to open.
  #
  # === Options for getline
  #
  # The options hash accepts the following keys:
  #
  # :chomp::
  #   When the optional +chomp+ keyword argument has a true value,
  #   <code>\n</code>, <code>\r</code>, and <code>\r\n</code>
  #   will be removed from the end of each line.
  #
  # See also IO.read for details about open_args.
  def self.readlines(*several_variants) end

  # Calls select(2) system call.
  # It monitors given arrays of <code>IO</code> objects, waits until one or more
  # of <code>IO</code> objects are ready for reading, are ready for writing,
  # and have pending exceptions respectively, and returns an array that
  # contains arrays of those IO objects.  It will return +nil+
  # if optional <i>timeout</i> value is given and no <code>IO</code> object
  # is ready in <i>timeout</i> seconds.
  #
  # <code>IO.select</code> peeks the buffer of <code>IO</code> objects for testing readability.
  # If the <code>IO</code> buffer is not empty,
  # <code>IO.select</code> immediately notifies readability.
  # This "peek" only happens for <code>IO</code> objects.
  # It does not happen for IO-like objects such as OpenSSL::SSL::SSLSocket.
  #
  # The best way to use <code>IO.select</code> is invoking it
  # after nonblocking methods such as <code>read_nonblock</code>, <code>write_nonblock</code>, etc.
  # The methods raise an exception which is extended by
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
  # The most likely situation is that <code>OpenSSL::SSL::SSLSocket</code> buffers some data.
  # <code>IO.select</code> doesn't see the buffer.
  # So <code>IO.select</code> can block when <code>OpenSSL::SSL::SSLSocket#readpartial</code> doesn't block.
  #
  # However, several more complicated situations exist.
  #
  # SSL is a protocol which is sequence of records.
  # The record consists of multiple bytes.
  # So, the remote side of SSL sends a partial record,
  # <code>IO.select</code> notifies readability but
  # <code>OpenSSL::SSL::SSLSocket</code> cannot decrypt a byte and
  # <code>OpenSSL::SSL::SSLSocket#readpartial</code> will blocks.
  #
  # Also, the remote side can request SSL renegotiation which forces
  # the local SSL engine to write some data.
  # This means <code>OpenSSL::SSL::SSLSocket#readpartial</code> may
  # invoke <code>write</code> system call and it can block.
  # In such a situation, <code>OpenSSL::SSL::SSLSocket#read_nonblock</code>
  # raises IO::WaitWritable instead of blocking.
  # So, the caller should wait for ready for writability as above example.
  #
  # The combination of nonblocking methods and <code>IO.select</code> is
  # also useful for streams such as tty, pipe socket socket when
  # multiple processes read from a stream.
  #
  # Finally, Linux kernel developers don't guarantee that
  # readability of select(2) means readability of following read(2) even
  # for a single process.
  # See select(2) manual on GNU/Linux system.
  #
  # Invoking <code>IO.select</code> before <code>IO#readpartial</code> works well as usual.
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

  # Opens the given path, returning the underlying file descriptor as a
  # <code>Integer</code>.
  #
  #    IO.sysopen("testfile")   #=> 3
  def self.sysopen(p1, p2 = v2, p3 = v3) end

  # Try to convert <i>obj</i> into an IO, using to_io method.
  # Returns converted IO or +nil+ if <i>obj</i> cannot be converted
  # for any reason.
  #
  #    IO.try_convert(STDOUT)     #=> STDOUT
  #    IO.try_convert("STDOUT")   #=> nil
  #
  #    require 'zlib'
  #    f = open("/tmp/zz.gz")       #=> #<File:/tmp/zz.gz>
  #    z = Zlib::GzipReader.open(f) #=> #<Zlib::GzipReader:0x81d8744>
  #    IO.try_convert(z)            #=> #<File:/tmp/zz.gz>
  def self.try_convert(obj) end

  # Opens the file, optionally seeks to the given <i>offset</i>, writes
  # <i>string</i>, then returns the length written.
  # <code>write</code> ensures the file is closed before returning.
  # If <i>offset</i> is not given in write mode, the file is truncated.
  # Otherwise, it is not truncated.
  #
  #   IO.write("testfile", "0123456789", 20)  #=> 10
  #   # File could contain:  "This is line one\nThi0123456789two\nThis is line three\nAnd so on...\n"
  #   IO.write("testfile", "0123456789")      #=> 10
  #   # File would now read: "0123456789"
  #
  # If the last argument is a hash, it specifies options for the internal
  # open().  It accepts the following keys:
  #
  # :encoding::
  #   string or encoding
  #
  #   Specifies the encoding of the read string.
  #   See Encoding.aliases for possible encodings.
  #
  # :mode::
  #   string or integer
  #
  #   Specifies the <i>mode</i> argument for open().  It must start
  #   with "w", "a", or "r+", otherwise it will cause an error.
  #   See IO.new for the list of possible modes.
  #
  # :perm::
  #   integer
  #
  #   Specifies the <i>perm</i> argument for open().
  #
  # :open_args::
  #   array
  #
  #   Specifies arguments for open() as an array.
  #   This key can not be used in combination with other keys.
  def self.write(*several_variants) end

  # Returns a new IO object (a stream) for the given integer file descriptor
  # +fd+ and +mode+ string.  +opt+ may be used to specify parts of +mode+ in a
  # more readable fashion.  See also IO.sysopen and IO.for_fd.
  #
  # IO.new is called by various File and IO opening methods such as IO::open,
  # Kernel#open, and File::open.
  #
  # === Open Mode
  #
  # When +mode+ is an integer it must be combination of the modes defined in
  # File::Constants (+File::RDONLY+, <code>File::WRONLY|File::CREAT</code>).
  # See the open(2) man page for more information.
  #
  # When +mode+ is a string it must be in one of the following forms:
  #
  #   fmode
  #   fmode ":" ext_enc
  #   fmode ":" ext_enc ":" int_enc
  #   fmode ":" "BOM|UTF-*"
  #
  # +fmode+ is an IO open mode string, +ext_enc+ is the external encoding for
  # the IO and +int_enc+ is the internal encoding.
  #
  # ==== IO Open Mode
  #
  # Ruby allows the following open modes:
  #
  #     "r"  Read-only, starts at beginning of file  (default mode).
  #
  #     "r+" Read-write, starts at beginning of file.
  #
  #     "w"  Write-only, truncates existing file
  #          to zero length or creates a new file for writing.
  #
  #     "w+" Read-write, truncates existing file to zero length
  #          or creates a new file for reading and writing.
  #
  #     "a"  Write-only, each write call appends data at end of file.
  #          Creates a new file for writing if file does not exist.
  #
  #     "a+" Read-write, each write call appends data at end of file.
  #          Creates a new file for reading and writing if file does
  #          not exist.
  #
  # The following modes must be used separately, and along with one or more of
  # the modes seen above.
  #
  #     "b"  Binary file mode
  #          Suppresses EOL <-> CRLF conversion on Windows. And
  #          sets external encoding to ASCII-8BIT unless explicitly
  #          specified.
  #
  #     "t"  Text file mode
  #
  # When the open mode of original IO is read only, the mode cannot be
  # changed to be writable.  Similarly, the open mode cannot be changed from
  # write only to readable.
  #
  # When such a change is attempted the error is raised in different locations
  # according to the platform.
  #
  # === IO Encoding
  #
  # When +ext_enc+ is specified, strings read will be tagged by the encoding
  # when reading, and strings output will be converted to the specified
  # encoding when writing.
  #
  # When +ext_enc+ and +int_enc+ are specified read strings will be converted
  # from +ext_enc+ to +int_enc+ upon input, and written strings will be
  # converted from +int_enc+ to +ext_enc+ upon output.  See Encoding for
  # further details of transcoding on input and output.
  #
  # If "BOM|UTF-8", "BOM|UTF-16LE" or "BOM|UTF16-BE" are used, Ruby checks for
  # a Unicode BOM in the input document to help determine the encoding.  For
  # UTF-16 encodings the file open mode must be binary.  When present, the BOM
  # is stripped and the external encoding from the BOM is used.  When the BOM
  # is missing the given Unicode encoding is used as +ext_enc+.  (The BOM-set
  # encoding option is case insensitive, so "bom|utf-8" is also valid.)
  #
  # === Options
  #
  # +opt+ can be used instead of +mode+ for improved readability.  The
  # following keys are supported:
  #
  # :mode ::
  #   Same as +mode+ parameter
  #
  # :flags ::
  #   Specifies file open flags as integer.
  #   If +mode+ parameter is given, this parameter will be bitwise-ORed.
  #
  # :\external_encoding ::
  #   External encoding for the IO.
  #
  # :\internal_encoding ::
  #   Internal encoding for the IO.  "-" is a synonym for the default internal
  #   encoding.
  #
  #   If the value is +nil+ no conversion occurs.
  #
  # :encoding ::
  #   Specifies external and internal encodings as "extern:intern".
  #
  # :textmode ::
  #   If the value is truth value, same as "t" in argument +mode+.
  #
  # :binmode ::
  #   If the value is truth value, same as "b" in argument +mode+.
  #
  # :autoclose ::
  #   If the value is +false+, the +fd+ will be kept open after this IO
  #   instance gets finalized.
  #
  # Also, +opt+ can have same keys in String#encode for controlling conversion
  # between the external encoding and the internal encoding.
  #
  # === Example 1
  #
  #   fd = IO.sysopen("/dev/tty", "w")
  #   a = IO.new(fd,"w")
  #   $stderr.puts "Hello"
  #   a.puts "World"
  #
  # Produces:
  #
  #   Hello
  #   World
  #
  # === Example 2
  #
  #   require 'fcntl'
  #
  #   fd = STDERR.fcntl(Fcntl::F_DUPFD)
  #   io = IO.new(fd, mode: 'w:UTF-16LE', cr_newline: true)
  #   io.puts "Hello, World!"
  #
  #   fd = STDERR.fcntl(Fcntl::F_DUPFD)
  #   io = IO.new(fd, mode: 'w', cr_newline: true,
  #               external_encoding: Encoding::UTF_16LE)
  #   io.puts "Hello, World!"
  #
  # Both of above print "Hello, World!" in UTF-16LE to standard error output
  # with converting EOL generated by <code>puts</code> to CR.
  def initialize(p1, p2 = v2, p3 = {}) end

  # String Output---Writes <i>obj</i> to <em>ios</em>.
  # <i>obj</i> will be converted to a string using
  # <code>to_s</code>.
  #
  #    $stdout << "Hello " << "world!\n"
  #
  # <em>produces:</em>
  #
  #    Hello world!
  def <<(obj) end

  # Announce an intention to access data from the current file in a
  # specific pattern. On platforms that do not support the
  # <em>posix_fadvise(2)</em> system call, this method is a no-op.
  #
  # _advice_ is one of the following symbols:
  #
  # :normal::     No advice to give; the default assumption for an open file.
  # :sequential:: The data will be accessed sequentially
  #               with lower offsets read before higher ones.
  # :random::     The data will be accessed in random order.
  # :willneed::   The data will be accessed in the near future.
  # :dontneed::   The data will not be accessed in the near future.
  # :noreuse::    The data will only be accessed once.
  #
  # The semantics of a piece of advice are platform-dependent. See
  # <em>man 2 posix_fadvise</em> for details.
  #
  # "data" means the region of the current file that begins at
  # _offset_ and extends for _len_ bytes. If _len_ is 0, the region
  # ends at the last byte of the file. By default, both _offset_ and
  # _len_ are 0, meaning that the advice applies to the entire file.
  #
  # If an error occurs, one of the following exceptions will be raised:
  #
  # <code>IOError</code>:: The <code>IO</code> stream is closed.
  # <code>Errno::EBADF</code>::
  #   The file descriptor of the current file is invalid.
  # <code>Errno::EINVAL</code>:: An invalid value for _advice_ was given.
  # <code>Errno::ESPIPE</code>::
  #   The file descriptor of the current file refers to a FIFO or
  #   pipe. (Linux raises <code>Errno::EINVAL</code> in this case).
  # <code>TypeError</code>::
  #   Either _advice_ was not a Symbol, or one of the
  #   other arguments was not an <code>Integer</code>.
  # <code>RangeError</code>:: One of the arguments given was too big/small.
  #
  # This list is not exhaustive; other Errno:: exceptions are also possible.
  def advise(advice, offset = 0, len = 0) end

  # Sets auto-close flag.
  #
  #    f = open("/dev/null")
  #    IO.for_fd(f.fileno)
  #    # ...
  #    f.gets # may cause IOError
  #
  #    f = open("/dev/null")
  #    IO.for_fd(f.fileno).autoclose = true
  #    # ...
  #    f.gets # won't cause IOError
  def autoclose=(bool) end

  # Returns +true+ if the underlying file descriptor of _ios_ will be
  # closed automatically at its finalization, otherwise +false+.
  def autoclose?; end

  def beep; end

  # Puts <em>ios</em> into binary mode.
  # Once a stream is in binary mode, it cannot be reset to nonbinary mode.
  #
  # - newline conversion disabled
  # - encoding conversion disabled
  # - content is treated as ASCII-8BIT
  def binmode; end

  # Returns <code>true</code> if <em>ios</em> is binmode.
  def binmode?; end

  # This is a deprecated alias for <code>each_byte</code>.
  def bytes; end

  # This is a deprecated alias for <code>each_char</code>.
  def chars; end

  # Closes <em>ios</em> and flushes any pending writes to the operating
  # system. The stream is unavailable for any further data operations;
  # an <code>IOError</code> is raised if such an attempt is made. I/O
  # streams are automatically closed when they are claimed by the
  # garbage collector.
  #
  # If <em>ios</em> is opened by <code>IO.popen</code>,
  # <code>close</code> sets <code>$?</code>.
  #
  # Calling this method on closed IO object is just ignored since Ruby 2.3.
  def close; end

  # Sets a close-on-exec flag.
  #
  #    f = open("/dev/null")
  #    f.close_on_exec = true
  #    system("cat", "/proc/self/fd/#{f.fileno}") # cat: /proc/self/fd/3: No such file or directory
  #    f.closed?                #=> false
  #
  # Ruby sets close-on-exec flags of all file descriptors by default
  # since Ruby 2.0.0.
  # So you don't need to set by yourself.
  # Also, unsetting a close-on-exec flag can cause file descriptor leak
  # if another thread use fork() and exec() (via system() method for example).
  # If you really needs file descriptor inheritance to child process,
  # use spawn()'s argument such as fd=>fd.
  def close_on_exec=(bool) end

  # Returns <code>true</code> if <em>ios</em> will be closed on exec.
  #
  #    f = open("/dev/null")
  #    f.close_on_exec?                 #=> false
  #    f.close_on_exec = true
  #    f.close_on_exec?                 #=> true
  #    f.close_on_exec = false
  #    f.close_on_exec?                 #=> false
  def close_on_exec?; end

  # Closes the read end of a duplex I/O stream (i.e., one that contains
  # both a read and a write stream, such as a pipe). Will raise an
  # <code>IOError</code> if the stream is not duplexed.
  #
  #    f = IO.popen("/bin/sh","r+")
  #    f.close_read
  #    f.readlines
  #
  # <em>produces:</em>
  #
  #    prog.rb:3:in `readlines': not opened for reading (IOError)
  #     from prog.rb:3
  #
  # Calling this method on closed IO object is just ignored since Ruby 2.3.
  def close_read; end

  # Closes the write end of a duplex I/O stream (i.e., one that contains
  # both a read and a write stream, such as a pipe). Will raise an
  # <code>IOError</code> if the stream is not duplexed.
  #
  #    f = IO.popen("/bin/sh","r+")
  #    f.close_write
  #    f.print "nowhere"
  #
  # <em>produces:</em>
  #
  #    prog.rb:3:in `write': not opened for writing (IOError)
  #     from prog.rb:3:in `print'
  #     from prog.rb:3
  #
  # Calling this method on closed IO object is just ignored since Ruby 2.3.
  def close_write; end

  # Returns <code>true</code> if <em>ios</em> is completely closed (for
  # duplex streams, both reader and writer), <code>false</code>
  # otherwise.
  #
  #    f = File.new("testfile")
  #    f.close         #=> nil
  #    f.closed?       #=> true
  #    f = IO.popen("/bin/sh","r+")
  #    f.close_write   #=> nil
  #    f.closed?       #=> false
  #    f.close_read    #=> nil
  #    f.closed?       #=> true
  def closed?; end

  # This is a deprecated alias for <code>each_codepoint</code>.
  def codepoints; end

  # Yields +self+ within cooked mode.
  #
  #   STDIN.cooked(&:gets)
  #
  # will read and return a line with echo back and line editing.
  #
  # You must require 'io/console' to use this method.
  def cooked; end

  # Enables cooked mode.
  #
  # If the terminal mode needs to be back, use io.cooked { ... }.
  #
  # You must require 'io/console' to use this method.
  def cooked!; end

  def cursor; end

  def cursor=(p1) end

  # Executes the block for every line in <em>ios</em>, where lines are
  # separated by <i>sep</i>. <em>ios</em> must be opened for
  # reading or an <code>IOError</code> will be raised.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    f = File.new("testfile")
  #    f.each {|line| puts "#{f.lineno}: #{line}" }
  #
  # <em>produces:</em>
  #
  #    1: This is line one
  #    2: This is line two
  #    3: This is line three
  #    4: And so on...
  #
  # See IO.readlines for details about getline_args.
  def each(*several_variants) end
  alias each_line each

  # Calls the given block once for each byte (0..255) in <em>ios</em>,
  # passing the byte as an argument. The stream must be opened for
  # reading or an <code>IOError</code> will be raised.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    f = File.new("testfile")
  #    checksum = 0
  #    f.each_byte {|x| checksum ^= x }   #=> #<File:testfile>
  #    checksum                           #=> 12
  def each_byte; end

  # Calls the given block once for each character in <em>ios</em>,
  # passing the character as an argument. The stream must be opened for
  # reading or an <code>IOError</code> will be raised.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    f = File.new("testfile")
  #    f.each_char {|c| print c, ' ' }   #=> #<File:testfile>
  def each_char; end

  # Passes the <code>Integer</code> ordinal of each character in <i>ios</i>,
  # passing the codepoint as an argument. The stream must be opened for
  # reading or an <code>IOError</code> will be raised.
  #
  # If no block is given, an enumerator is returned instead.
  def each_codepoint; end

  # Enables/disables echo back.
  # On some platforms, all combinations of this flags and raw/cooked
  # mode may not be valid.
  #
  # You must require 'io/console' to use this method.
  def echo=(flag) end

  # Returns +true+ if echo back is enabled.
  #
  # You must require 'io/console' to use this method.
  def echo?; end

  # Returns true if <em>ios</em> is at end of file that means
  # there are no more data to read.
  # The stream must be opened for reading or an <code>IOError</code> will be
  # raised.
  #
  #    f = File.new("testfile")
  #    dummy = f.readlines
  #    f.eof   #=> true
  #
  # If <em>ios</em> is a stream such as pipe or socket, <code>IO#eof?</code>
  # blocks until the other end sends some data or closes it.
  #
  #    r, w = IO.pipe
  #    Thread.new { sleep 1; w.close }
  #    r.eof?  #=> true after 1 second blocking
  #
  #    r, w = IO.pipe
  #    Thread.new { sleep 1; w.puts "a" }
  #    r.eof?  #=> false after 1 second blocking
  #
  #    r, w = IO.pipe
  #    r.eof?  # blocks forever
  #
  # Note that <code>IO#eof?</code> reads data to the input byte buffer.
  # So <code>IO#sysread</code> may not behave as you intend with
  # <code>IO#eof?</code>, unless you call <code>IO#rewind</code>
  # first (which is not available for some streams).
  def eof; end
  alias eof? eof

  # Returns the Encoding object that represents the encoding of the file.
  # If _io_ is in write mode and no encoding is specified, returns +nil+.
  def external_encoding; end

  # Provides a mechanism for issuing low-level commands to control or
  # query file-oriented I/O streams. Arguments and results are platform
  # dependent. If <i>arg</i> is a number, its value is passed
  # directly. If it is a string, it is interpreted as a binary sequence
  # of bytes (<code>Array#pack</code> might be a useful way to build this
  # string). On Unix platforms, see <code>fcntl(2)</code> for details.
  # Not implemented on all platforms.
  def fcntl(integer_cmd, arg) end

  # Immediately writes all buffered data in <em>ios</em> to disk.
  #
  # If the underlying operating system does not support <em>fdatasync(2)</em>,
  # <code>IO#fsync</code> is called instead (which might raise a
  # <code>NotImplementedError</code>).
  def fdatasync; end

  # Returns an integer representing the numeric file descriptor for
  # <em>ios</em>.
  #
  #    $stdin.fileno    #=> 0
  #    $stdout.fileno   #=> 1
  def fileno; end
  alias to_i fileno

  # Flushes any buffered data within <em>ios</em> to the underlying
  # operating system (note that this is Ruby internal buffering only;
  # the OS may buffer the data as well).
  #
  #    $stdout.print "no newline"
  #    $stdout.flush
  #
  # <em>produces:</em>
  #
  #    no newline
  def flush; end

  # Immediately writes all buffered data in <em>ios</em> to disk.
  # Note that <code>fsync</code> differs from
  # using <code>IO#sync=</code>. The latter ensures that data is flushed
  # from Ruby's buffers, but does not guarantee that the underlying
  # operating system actually writes it to disk.
  #
  # <code>NotImplementedError</code> is raised
  # if the underlying operating system does not support <em>fsync(2)</em>.
  def fsync; end

  # Gets the next 8-bit byte (0..255) from <em>ios</em>. Returns
  # +nil+ if called at end of file.
  #
  #    f = File.new("testfile")
  #    f.getbyte   #=> 84
  #    f.getbyte   #=> 104
  def getbyte; end

  # Reads a one-character string from <em>ios</em>. Returns
  # +nil+ if called at end of file.
  #
  #    f = File.new("testfile")
  #    f.getc   #=> "h"
  #    f.getc   #=> "e"
  def getc; end

  # Reads and returns a character in raw mode.
  #
  # See IO#raw for details on the parameters.
  #
  # You must require 'io/console' to use this method.
  def getch(min: nil, time: nil) end

  # Reads and returns a line without echo back.
  # Prints +prompt+ unless it is +nil+.
  #
  # You must require 'io/console' to use this method.
  def getpass(prompt = nil) end

  # Reads the next ``line'' from the I/O stream; lines are separated by
  # <i>sep</i>. A separator of +nil+ reads the entire
  # contents, and a zero-length separator reads the input a paragraph at
  # a time (two successive newlines in the input separate paragraphs).
  # The stream must be opened for reading or an <code>IOError</code>
  # will be raised. The line read in will be returned and also assigned
  # to <code>$_</code>. Returns +nil+ if called at end of
  # file.  If the first argument is an integer, or optional second
  # argument is given, the returning string would not be longer than the
  # given value in bytes.
  #
  #    File.new("testfile").gets   #=> "This is line one\n"
  #    $_                          #=> "This is line one\n"
  #
  #    File.new("testfile").gets(4)#=> "This"
  #
  # If IO contains multibyte characters byte then <code>gets(1)</code>
  # returns character entirely:
  #
  #    # Russian characters take 2 bytes
  #    File.write("testfile", "\u{442 435 441 442}")
  #    File.open("testfile") {|f|f.gets(1)} #=> "\u0442"
  #    File.open("testfile") {|f|f.gets(2)} #=> "\u0442"
  #    File.open("testfile") {|f|f.gets(3)} #=> "\u0442\u0435"
  #    File.open("testfile") {|f|f.gets(4)} #=> "\u0442\u0435"
  def gets(*several_variants) end

  def goto(p1, p2) end

  # Flushes input buffer in kernel.
  #
  # You must require 'io/console' to use this method.
  def iflush; end

  # Return a string describing this IO object.
  def inspect; end

  # Returns the Encoding of the internal string if conversion is
  # specified.  Otherwise returns +nil+.
  def internal_encoding; end

  # Provides a mechanism for issuing low-level commands to control or
  # query I/O devices. Arguments and results are platform dependent. If
  # <i>arg</i> is a number, its value is passed directly. If it is a
  # string, it is interpreted as a binary sequence of bytes. On Unix
  # platforms, see <code>ioctl(2)</code> for details. Not implemented on
  # all platforms.
  def ioctl(integer_cmd, arg) end

  # Flushes input and output buffers in kernel.
  #
  # You must require 'io/console' to use this method.
  def ioflush; end

  # Returns <code>true</code> if <em>ios</em> is associated with a
  # terminal device (tty), <code>false</code> otherwise.
  #
  #    File.new("testfile").isatty   #=> false
  #    File.new("/dev/tty").isatty   #=> true
  def isatty; end
  alias tty? isatty

  # Returns the current line number in <em>ios</em>.  The stream must be
  # opened for reading. <code>lineno</code> counts the number of times
  # #gets is called rather than the number of newlines encountered.  The two
  # values will differ if #gets is called with a separator other than newline.
  #
  # Methods that use <code>$/</code> like #each, #lines and #readline will
  # also increment <code>lineno</code>.
  #
  # See also the <code>$.</code> variable.
  #
  #    f = File.new("testfile")
  #    f.lineno   #=> 0
  #    f.gets     #=> "This is line one\n"
  #    f.lineno   #=> 1
  #    f.gets     #=> "This is line two\n"
  #    f.lineno   #=> 2
  def lineno; end

  # Manually sets the current line number to the given value.
  # <code>$.</code> is updated only on the next read.
  #
  #    f = File.new("testfile")
  #    f.gets                     #=> "This is line one\n"
  #    $.                         #=> 1
  #    f.lineno = 1000
  #    f.lineno                   #=> 1000
  #    $.                         #=> 1         # lineno of last read
  #    f.gets                     #=> "This is line two\n"
  #    $.                         #=> 1001      # lineno of last read
  def lineno=(integer) end

  # This is a deprecated alias for <code>each_line</code>.
  def lines(*args) end

  # Yields +self+ with disabling echo back.
  #
  #   STDIN.noecho(&:gets)
  #
  # will read and return a line without echo back.
  #
  # You must require 'io/console' to use this method.
  def noecho; end

  # Yields +self+ in non-blocking mode.
  #
  # When +false+ is given as an argument, +self+ is yielded in blocking mode.
  # The original mode is restored after the block is executed.
  def nonblock(*several_variants) end

  # Enables non-blocking mode on a stream when set to
  # +true+, and blocking mode when set to +false+.
  def nonblock=(boolean) end

  # Returns +true+ if an IO object is in non-blocking mode.
  def nonblock?; end

  # Returns number of bytes that can be read without blocking.
  # Returns zero if no information available.
  def nread; end

  # Flushes output buffer in kernel.
  #
  # You must require 'io/console' to use this method.
  def oflush; end

  # Returns pathname configuration variable using fpathconf().
  #
  # _name_ should be a constant under <code>Etc</code> which begins with <code>PC_</code>.
  #
  # The return value is an integer or nil.
  # nil means indefinite limit.  (fpathconf() returns -1 but errno is not set.)
  #
  #   require 'etc'
  #   IO.pipe {|r, w|
  #     p w.pathconf(Etc::PC_PIPE_BUF) #=> 4096
  #   }
  def pathconf(p1) end

  # Returns the process ID of a child process associated with
  # <em>ios</em>. This will be set by <code>IO.popen</code>.
  #
  #    pipe = IO.popen("-")
  #    if pipe
  #      $stderr.puts "In parent, child pid is #{pipe.pid}"
  #    else
  #      $stderr.puts "In child, pid is #{$$}"
  #    end
  #
  # <em>produces:</em>
  #
  #    In child, pid is 26209
  #    In parent, child pid is 26209
  def pid; end

  # Seeks to the given position (in bytes) in <em>ios</em>.
  # It is not guaranteed that seeking to the right position when <em>ios</em>
  # is textmode.
  #
  #    f = File.new("testfile")
  #    f.pos = 17
  #    f.gets   #=> "This is line two\n"
  def pos=(integer) end

  # Reads <i>maxlen</i> bytes from <em>ios</em> using the pread system call
  # and returns them as a string without modifying the underlying
  # descriptor offset.  This is advantageous compared to combining IO#seek
  # and IO#read in that it is atomic, allowing multiple threads/process to
  # share the same IO object for reading the file at various locations.
  # This bypasses any userspace buffering of the IO layer.
  # If the optional <i>outbuf</i> argument is present, it must
  # reference a String, which will receive the data.
  # Raises <code>SystemCallError</code> on error, <code>EOFError</code>
  # at end of file and <code>NotImplementedError</code> if platform does not
  # implement the system call.
  #
  #    File.write("testfile", "This is line one\nThis is line two\n")
  #    File.open("testfile") do |f|
  #      p f.read           # => "This is line one\nThis is line two\n"
  #      p f.pread(12, 0)   # => "This is line"
  #      p f.pread(9, 8)    # => "line one\n"
  #    end
  def pread(p1, p2, p3 = v3) end

  def pressed?(p1) end

  # Writes the given object(s) to <em>ios</em>. Returns +nil+.
  #
  # The stream must be opened for writing.
  # Each given object that isn't a string will be converted by calling
  # its <code>to_s</code> method.
  # When called without arguments, prints the contents of <code>$_</code>.
  #
  # If the output field separator (<code>$,</code>) is not +nil+,
  # it is inserted between objects.
  # If the output record separator (<code>$\\</code>) is not +nil+,
  # it is appended to the output.
  #
  #    $stdout.print("This is ", 100, " percent.\n")
  #
  # <em>produces:</em>
  #
  #    This is 100 percent.
  def print(*several_variants) end

  # Formats and writes to <em>ios</em>, converting parameters under
  # control of the format string. See <code>Kernel#sprintf</code>
  # for details.
  def printf(*args) end

  # If <i>obj</i> is <code>Numeric</code>, write the character whose code is
  # the least-significant byte of <i>obj</i>.
  # If <i>obj</i> is <code>String</code>, write the first character
  # of <i>obj</i> to <em>ios</em>.
  # Otherwise, raise <code>TypeError</code>.
  #
  #    $stdout.putc "A"
  #    $stdout.putc 65
  #
  # <em>produces:</em>
  #
  #    AA
  def putc(obj) end

  # Writes the given object(s) to <em>ios</em>.
  # Writes a newline after any that do not already end
  # with a newline sequence. Returns +nil+.
  #
  # The stream must be opened for writing.
  # If called with an array argument, writes each element on a new line.
  # Each given object that isn't a string or array will be converted
  # by calling its +to_s+ method.
  # If called without arguments, outputs a single newline.
  #
  #    $stdout.puts("this", "is", ["a", "test"])
  #
  # <em>produces:</em>
  #
  #    this
  #    is
  #    a
  #    test
  #
  # Note that +puts+ always uses newlines and is not affected
  # by the output record separator (<code>$\\</code>).
  def puts(obj = '', *arg) end

  # Writes the given string to <em>ios</em> at <i>offset</i> using pwrite()
  # system call.  This is advantageous to combining IO#seek and IO#write
  # in that it is atomic, allowing multiple threads/process to share the
  # same IO object for reading the file at various locations.
  # This bypasses any userspace buffering of the IO layer.
  # Returns the number of bytes written.
  # Raises <code>SystemCallError</code> on error and <code>NotImplementedError</code>
  # if platform does not implement the system call.
  #
  #    File.open("out", "w") do |f|
  #      f.pwrite("ABCDEF", 3)   #=> 6
  #    end
  #
  #    File.read("out")          #=> "\u0000\u0000\u0000ABCDEF"
  def pwrite(string, offset) end

  # Yields +self+ within raw mode.
  #
  #   STDIN.raw(&:gets)
  #
  # will read and return a line without echo back and line editing.
  #
  # The parameter +min+ specifies the minimum number of bytes that
  # should be received when a read operation is performed. (default: 1)
  #
  # The parameter +time+ specifies the timeout in _seconds_ with a
  # precision of 1/10 of a second. (default: 0)
  #
  # Refer to the manual page of termios for further details.
  #
  # You must require 'io/console' to use this method.
  def raw(min: nil, time: nil) end

  # Enables raw mode.
  #
  # If the terminal mode needs to be back, use io.raw { ... }.
  #
  # See IO#raw for details on the parameters.
  #
  # You must require 'io/console' to use this method.
  def raw!(min: nil, time: nil) end

  # Reads _length_ bytes from the I/O stream.
  #
  # _length_ must be a non-negative integer or +nil+.
  #
  # If _length_ is a positive integer, +read+ tries to read
  # _length_ bytes without any conversion (binary mode).
  # It returns +nil+ if an EOF is encountered before anything can be read.
  # Fewer than _length_ bytes are returned if an EOF is encountered during
  # the read.
  # In the case of an integer _length_, the resulting string is always
  # in ASCII-8BIT encoding.
  #
  # If _length_ is omitted or is +nil+, it reads until EOF
  # and the encoding conversion is applied, if applicable.
  # A string is returned even if EOF is encountered before any data is read.
  #
  # If _length_ is zero, it returns an empty string (<code>""</code>).
  #
  # If the optional _outbuf_ argument is present,
  # it must reference a String, which will receive the data.
  # The _outbuf_ will contain only the received data after the method call
  # even if it is not empty at the beginning.
  #
  # When this method is called at end of file, it returns +nil+
  # or <code>""</code>, depending on _length_:
  # +read+, <code>read(nil)</code>, and <code>read(0)</code> return
  # <code>""</code>,
  # <code>read(<i>positive_integer</i>)</code> returns +nil+.
  #
  #    f = File.new("testfile")
  #    f.read(16)   #=> "This is line one"
  #
  #    # read whole file
  #    open("file") do |f|
  #      data = f.read   # This returns a string even if the file is empty.
  #      # ...
  #    end
  #
  #    # iterate over fixed length records
  #    open("fixed-record-file") do |f|
  #      while record = f.read(256)
  #        # ...
  #      end
  #    end
  #
  #    # iterate over variable length records,
  #    # each record is prefixed by its 32-bit length
  #    open("variable-record-file") do |f|
  #      while len = f.read(4)
  #        len = len.unpack("N")[0]   # 32-bit length
  #        record = f.read(len)       # This returns a string even if len is 0.
  #      end
  #    end
  #
  # Note that this method behaves like the fread() function in C.
  # This means it retries to invoke read(2) system calls to read data
  # with the specified length (or until EOF).
  # This behavior is preserved even if <i>ios</i> is in non-blocking mode.
  # (This method is non-blocking flag insensitive as other methods.)
  # If you need the behavior like a single read(2) system call,
  # consider #readpartial, #read_nonblock, and #sysread.
  def read(p1 = v1, p2 = v2) end

  # Reads at most <i>maxlen</i> bytes from <em>ios</em> using
  # the read(2) system call after O_NONBLOCK is set for
  # the underlying file descriptor.
  #
  # If the optional <i>outbuf</i> argument is present,
  # it must reference a String, which will receive the data.
  # The <i>outbuf</i> will contain only the received data after the method call
  # even if it is not empty at the beginning.
  #
  # read_nonblock just calls the read(2) system call.
  # It causes all errors the read(2) system call causes: Errno::EWOULDBLOCK, Errno::EINTR, etc.
  # The caller should care such errors.
  #
  # If the exception is Errno::EWOULDBLOCK or Errno::EAGAIN,
  # it is extended by IO::WaitReadable.
  # So IO::WaitReadable can be used to rescue the exceptions for retrying
  # read_nonblock.
  #
  # read_nonblock causes EOFError on EOF.
  #
  # If the read byte buffer is not empty,
  # read_nonblock reads from the buffer like readpartial.
  # In this case, the read(2) system call is not called.
  #
  # When read_nonblock raises an exception kind of IO::WaitReadable,
  # read_nonblock should not be called
  # until io is readable for avoiding busy loop.
  # This can be done as follows.
  #
  #   # emulates blocking read (readpartial).
  #   begin
  #     result = io.read_nonblock(maxlen)
  #   rescue IO::WaitReadable
  #     IO.select([io])
  #     retry
  #   end
  #
  # Although IO#read_nonblock doesn't raise IO::WaitWritable.
  # OpenSSL::Buffering#read_nonblock can raise IO::WaitWritable.
  # If IO and SSL should be used polymorphically,
  # IO::WaitWritable should be rescued too.
  # See the document of OpenSSL::Buffering#read_nonblock for sample code.
  #
  # Note that this method is identical to readpartial
  # except the non-blocking flag is set.
  #
  # By specifying a keyword argument _exception_ to +false+, you can indicate
  # that read_nonblock should not raise an IO::WaitReadable exception, but
  # return the symbol +:wait_readable+ instead. At EOF, it will return nil
  # instead of raising EOFError.
  def read_nonblock(*several_variants) end

  # Reads a byte as with <code>IO#getbyte</code>, but raises an
  # <code>EOFError</code> on end of file.
  def readbyte; end

  # Reads a one-character string from <em>ios</em>. Raises an
  # <code>EOFError</code> on end of file.
  #
  #    f = File.new("testfile")
  #    f.readchar   #=> "h"
  #    f.readchar   #=> "e"
  def readchar; end

  # Reads a line as with <code>IO#gets</code>, but raises an
  # <code>EOFError</code> on end of file.
  def readline(*several_variants) end

  # Reads all of the lines in <em>ios</em>, and returns them in
  # an array. Lines are separated by the optional <i>sep</i>. If
  # <i>sep</i> is +nil+, the rest of the stream is returned
  # as a single record.
  # If the first argument is an integer, or an
  # optional second argument is given, the returning string would not be
  # longer than the given value in bytes. The stream must be opened for
  # reading or an <code>IOError</code> will be raised.
  #
  #    f = File.new("testfile")
  #    f.readlines[0]   #=> "This is line one\n"
  #
  #    f = File.new("testfile", chomp: true)
  #    f.readlines[0]   #=> "This is line one"
  #
  # See IO.readlines for details about getline_args.
  def readlines(*several_variants) end

  # Reads at most <i>maxlen</i> bytes from the I/O stream.
  # It blocks only if <em>ios</em> has no data immediately available.
  # It doesn't block if some data available.
  #
  # If the optional _outbuf_ argument is present,
  # it must reference a String, which will receive the data.
  # The _outbuf_ will contain only the received data after the method call
  # even if it is not empty at the beginning.
  #
  # It raises <code>EOFError</code> on end of file.
  #
  # readpartial is designed for streams such as pipe, socket, tty, etc.
  # It blocks only when no data immediately available.
  # This means that it blocks only when following all conditions hold.
  # * the byte buffer in the IO object is empty.
  # * the content of the stream is empty.
  # * the stream is not reached to EOF.
  #
  # When readpartial blocks, it waits data or EOF on the stream.
  # If some data is reached, readpartial returns with the data.
  # If EOF is reached, readpartial raises EOFError.
  #
  # When readpartial doesn't blocks, it returns or raises immediately.
  # If the byte buffer is not empty, it returns the data in the buffer.
  # Otherwise if the stream has some content,
  # it returns the data in the stream.
  # Otherwise if the stream is reached to EOF, it raises EOFError.
  #
  #    r, w = IO.pipe           #               buffer          pipe content
  #    w << "abc"               #               ""              "abc".
  #    r.readpartial(4096)      #=> "abc"       ""              ""
  #    r.readpartial(4096)      # blocks because buffer and pipe is empty.
  #
  #    r, w = IO.pipe           #               buffer          pipe content
  #    w << "abc"               #               ""              "abc"
  #    w.close                  #               ""              "abc" EOF
  #    r.readpartial(4096)      #=> "abc"       ""              EOF
  #    r.readpartial(4096)      # raises EOFError
  #
  #    r, w = IO.pipe           #               buffer          pipe content
  #    w << "abc\ndef\n"        #               ""              "abc\ndef\n"
  #    r.gets                   #=> "abc\n"     "def\n"         ""
  #    w << "ghi\n"             #               "def\n"         "ghi\n"
  #    r.readpartial(4096)      #=> "def\n"     ""              "ghi\n"
  #    r.readpartial(4096)      #=> "ghi\n"     ""              ""
  #
  # Note that readpartial behaves similar to sysread.
  # The differences are:
  # * If the byte buffer is not empty, read from the byte buffer instead of "sysread for buffered IO (IOError)".
  # * It doesn't cause Errno::EWOULDBLOCK and Errno::EINTR.  When readpartial meets EWOULDBLOCK and EINTR by read system call, readpartial retry the system call.
  #
  # The latter means that readpartial is nonblocking-flag insensitive.
  # It blocks on the situation IO#sysread causes Errno::EWOULDBLOCK as if the fd is blocking mode.
  def readpartial(*several_variants) end

  # Returns true if input available without blocking, or false.
  # Returns nil if no information available.
  def ready?; end

  # Reassociates <em>ios</em> with the I/O stream given in
  # <i>other_IO</i> or to a new stream opened on <i>path</i>. This may
  # dynamically change the actual class of this stream.
  #
  #    f1 = File.new("testfile")
  #    f2 = File.new("testfile")
  #    f2.readlines[0]   #=> "This is line one\n"
  #    f2.reopen(f1)     #=> #<File:testfile>
  #    f2.readlines[0]   #=> "This is line one\n"
  def reopen(*several_variants) end

  # Positions <em>ios</em> to the beginning of input, resetting
  # <code>lineno</code> to zero.
  #
  #    f = File.new("testfile")
  #    f.readline   #=> "This is line one\n"
  #    f.rewind     #=> 0
  #    f.lineno     #=> 0
  #    f.readline   #=> "This is line one\n"
  #
  # Note that it cannot be used with streams such as pipes, ttys, and sockets.
  def rewind; end

  # Seeks to a given offset <i>anInteger</i> in the stream according to
  # the value of <i>whence</i>:
  #
  #   :CUR or IO::SEEK_CUR  | Seeks to _amount_ plus current position
  #   ----------------------+--------------------------------------------------
  #   :END or IO::SEEK_END  | Seeks to _amount_ plus end of stream (you
  #                         | probably want a negative value for _amount_)
  #   ----------------------+--------------------------------------------------
  #   :SET or IO::SEEK_SET  | Seeks to the absolute location given by _amount_
  #
  # Example:
  #
  #    f = File.new("testfile")
  #    f.seek(-13, IO::SEEK_END)   #=> 0
  #    f.readline                  #=> "And so on...\n"
  def seek(amount, whence = IO::SEEK_SET) end

  # If single argument is specified, read string from io is tagged
  # with the encoding specified.  If encoding is a colon separated two
  # encoding names "A:B", the read string is converted from encoding A
  # (external encoding) to encoding B (internal encoding), then tagged
  # with B.  If two arguments are specified, those must be encoding
  # objects or encoding names, and the first one is the external encoding, and the
  # second one is the internal encoding.
  # If the external encoding and the internal encoding is specified,
  # optional hash argument specify the conversion option.
  def set_encoding(*several_variants) end

  # Returns status information for <em>ios</em> as an object of type
  # <code>File::Stat</code>.
  #
  #    f = File.new("testfile")
  #    s = f.stat
  #    "%o" % s.mode   #=> "100644"
  #    s.blksize       #=> 4096
  #    s.atime         #=> Wed Apr 09 08:53:54 CDT 2003
  def stat; end

  # Returns the current ``sync mode'' of <em>ios</em>. When sync mode is
  # true, all output is immediately flushed to the underlying operating
  # system and is not buffered by Ruby internally. See also
  # <code>IO#fsync</code>.
  #
  #    f = File.new("testfile")
  #    f.sync   #=> false
  def sync; end

  # Sets the ``sync mode'' to <code>true</code> or <code>false</code>.
  # When sync mode is true, all output is immediately flushed to the
  # underlying operating system and is not buffered internally. Returns
  # the new state. See also <code>IO#fsync</code>.
  #
  #    f = File.new("testfile")
  #    f.sync = true
  def sync=(boolean) end

  # Reads <i>maxlen</i> bytes from <em>ios</em> using a low-level
  # read and returns them as a string.  Do not mix with other methods
  # that read from <em>ios</em> or you may get unpredictable results.
  #
  # If the optional _outbuf_ argument is present,
  # it must reference a String, which will receive the data.
  # The _outbuf_ will contain only the received data after the method call
  # even if it is not empty at the beginning.
  #
  # Raises <code>SystemCallError</code> on error and
  # <code>EOFError</code> at end of file.
  #
  #    f = File.new("testfile")
  #    f.sysread(16)   #=> "This is line one"
  def sysread(p1, p2 = v2) end

  # Seeks to a given <i>offset</i> in the stream according to the value
  # of <i>whence</i> (see <code>IO#seek</code> for values of
  # <i>whence</i>). Returns the new offset into the file.
  #
  #    f = File.new("testfile")
  #    f.sysseek(-13, IO::SEEK_END)   #=> 53
  #    f.sysread(10)                  #=> "And so on."
  def sysseek(offset, whence = IO::SEEK_SET) end

  # Writes the given string to <em>ios</em> using a low-level write.
  # Returns the number of bytes written. Do not mix with other methods
  # that write to <em>ios</em> or you may get unpredictable results.
  # Raises <code>SystemCallError</code> on error.
  #
  #    f = File.new("out", "w")
  #    f.syswrite("ABCDEF")   #=> 6
  def syswrite(string) end

  # Returns the current offset (in bytes) of <em>ios</em>.
  #
  #    f = File.new("testfile")
  #    f.pos    #=> 0
  #    f.gets   #=> "This is line one\n"
  #    f.pos    #=> 17
  def tell; end
  alias pos tell

  # Returns <em>ios</em>.
  def to_io; end

  # Pushes back bytes (passed as a parameter) onto <em>ios</em>,
  # such that a subsequent buffered read will return it. Only one byte
  # may be pushed back before a subsequent read operation (that is,
  # you will be able to read only the last of several bytes that have been pushed
  # back). Has no effect with unbuffered reads (such as <code>IO#sysread</code>).
  #
  #    f = File.new("testfile")   #=> #<File:testfile>
  #    b = f.getbyte              #=> 0x38
  #    f.ungetbyte(b)             #=> nil
  #    f.getbyte                  #=> 0x38
  def ungetbyte(*several_variants) end

  # Pushes back one character (passed as a parameter) onto <em>ios</em>,
  # such that a subsequent buffered character read will return it. Only one character
  # may be pushed back before a subsequent read operation (that is,
  # you will be able to read only the last of several characters that have been pushed
  # back). Has no effect with unbuffered reads (such as <code>IO#sysread</code>).
  #
  #    f = File.new("testfile")   #=> #<File:testfile>
  #    c = f.getc                 #=> "8"
  #    f.ungetc(c)                #=> nil
  #    f.getc                     #=> "8"
  def ungetc(string) end

  # Waits until IO is readable or writable without blocking and returns
  # +self+, or +nil+ when times out.
  # Returns +true+ immediately when buffered data is available.
  # Optional parameter +mode+ is one of +:read+, +:write+, or
  # +:read_write+.
  def wait(timeout = nil, mode = :read) end

  # Waits until IO is readable without blocking and returns +self+, or
  # +nil+ when times out.
  # Returns +true+ immediately when buffered data is available.
  def wait_readable(*several_variants) end

  # Waits until IO is writable without blocking and returns +self+ or
  # +nil+ when times out.
  def wait_writable(*several_variants) end

  # Returns console size.
  #
  # You must require 'io/console' to use this method.
  def winsize; end

  # Tries to set console size.  The effect depends on the platform and
  # the running environment.
  #
  # You must require 'io/console' to use this method.
  def winsize=(p1) end

  # Writes the given strings to <em>ios</em>. The stream must be opened
  # for writing. Arguments that are not a string will be converted
  # to a string using <code>to_s</code>. Returns the number of bytes
  # written in total.
  #
  #    count = $stdout.write("This is", " a test\n")
  #    puts "That was #{count} bytes of data"
  #
  # <em>produces:</em>
  #
  #    This is a test
  #    That was 15 bytes of data
  def write(string, *args) end

  # Writes the given string to <em>ios</em> using
  # the write(2) system call after O_NONBLOCK is set for
  # the underlying file descriptor.
  #
  # It returns the number of bytes written.
  #
  # write_nonblock just calls the write(2) system call.
  # It causes all errors the write(2) system call causes: Errno::EWOULDBLOCK, Errno::EINTR, etc.
  # The result may also be smaller than string.length (partial write).
  # The caller should care such errors and partial write.
  #
  # If the exception is Errno::EWOULDBLOCK or Errno::EAGAIN,
  # it is extended by IO::WaitWritable.
  # So IO::WaitWritable can be used to rescue the exceptions for retrying write_nonblock.
  #
  #   # Creates a pipe.
  #   r, w = IO.pipe
  #
  #   # write_nonblock writes only 65536 bytes and return 65536.
  #   # (The pipe size is 65536 bytes on this environment.)
  #   s = "a" * 100000
  #   p w.write_nonblock(s)     #=> 65536
  #
  #   # write_nonblock cannot write a byte and raise EWOULDBLOCK (EAGAIN).
  #   p w.write_nonblock("b")   # Resource temporarily unavailable (Errno::EAGAIN)
  #
  # If the write buffer is not empty, it is flushed at first.
  #
  # When write_nonblock raises an exception kind of IO::WaitWritable,
  # write_nonblock should not be called
  # until io is writable for avoiding busy loop.
  # This can be done as follows.
  #
  #   begin
  #     result = io.write_nonblock(string)
  #   rescue IO::WaitWritable, Errno::EINTR
  #     IO.select(nil, [io])
  #     retry
  #   end
  #
  # Note that this doesn't guarantee to write all data in string.
  # The length written is reported as result and it should be checked later.
  #
  # On some platforms such as Windows, write_nonblock is not supported
  # according to the kind of the IO object.
  # In such cases, write_nonblock raises <code>Errno::EBADF</code>.
  #
  # By specifying a keyword argument _exception_ to +false+, you can indicate
  # that write_nonblock should not raise an IO::WaitWritable exception, but
  # return the symbol +:wait_writable+ instead.
  def write_nonblock(*several_variants) end

  class EAGAINWaitReadable < Errno::EAGAIN
    include IO::WaitReadable
  end

  class EAGAINWaitWritable < Errno::EAGAIN
    include IO::WaitWritable
  end

  class EINPROGRESSWaitReadable < Errno::EINPROGRESS
    include IO::WaitReadable
  end

  class EINPROGRESSWaitWritable < Errno::EINPROGRESS
    include IO::WaitWritable
  end

  class EWOULDBLOCKWaitReadable < Errno::EWOULDBLOCK
    include IO::WaitReadable
  end

  class EWOULDBLOCKWaitWritable < Errno::EWOULDBLOCK
    include IO::WaitWritable
  end

  module WaitReadable
  end

  module WaitWritable
  end
end
