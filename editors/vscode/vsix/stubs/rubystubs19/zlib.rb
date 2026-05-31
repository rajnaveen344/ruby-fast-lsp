# frozen_string_literal: true

# == Overview
#
# Access to the zlib library.
#
# == Class tree
#
# - Zlib::Deflate
# - Zlib::Inflate
# - Zlib::ZStream
# - Zlib::Error
#   - Zlib::StreamEnd
#   - Zlib::NeedDict
#   - Zlib::DataError
#   - Zlib::StreamError
#   - Zlib::MemError
#   - Zlib::BufError
#   - Zlib::VersionError
#
# (if you have GZIP_SUPPORT)
# - Zlib::GzipReader
# - Zlib::GzipWriter
# - Zlib::GzipFile
# - Zlib::GzipFile::Error
#   - Zlib::GzipFile::LengthError
#   - Zlib::GzipFile::CRCError
#   - Zlib::GzipFile::NoFooter
#
# see also zlib.h
module Zlib
  # Integer representing date types which
  # ZStream#data_type method returns
  ASCII = _
  # compression level 9
  #
  # Which is an argument for Deflate.new, Deflate#deflate, and so on.
  BEST_COMPRESSION = _
  # compression level 1
  #
  # Which is an argument for Deflate.new, Deflate#deflate, and so on.
  BEST_SPEED = _
  # Integer representing date types which
  # ZStream#data_type method returns
  BINARY = _
  # compression level -1
  #
  # Which is an argument for Deflate.new, Deflate#deflate, and so on.
  DEFAULT_COMPRESSION = _
  # compression method 0
  #
  # Which is an argument for Deflate.new and Deflate#params.
  DEFAULT_STRATEGY = _
  # Default value is 8
  #
  # The integer representing memory levels.
  # Which are an argument for Deflate.new, Deflate#params, and so on.
  DEF_MEM_LEVEL = _
  # compression method 1
  #
  # Which is an argument for Deflate.new and Deflate#params.
  FILTERED = _
  # Oputput control - 4
  #
  # The integers to control the output of the deflate stream, which are
  # an argument for Deflate#deflate and so on.
  FINISH = _
  # Output control - 3
  #
  # The integers to control the output of the deflate stream, which are
  # an argument for Deflate#deflate and so on.
  FULL_FLUSH = _
  # compression method 2
  #
  # Which is an argument for Deflate.new and Deflate#params.
  HUFFMAN_ONLY = _
  # Maximum level is 9
  #
  # The integers representing memory levels which are an argument for
  # Deflate.new, Deflate#params, and so on.
  MAX_MEM_LEVEL = _
  # The default value of windowBits which is an argument for
  # Deflate.new and Inflate.new.
  MAX_WBITS = _
  # compression level 0
  #
  # Which is an argument for Deflate.new, Deflate#deflate, and so on.
  NO_COMPRESSION = _
  # Output control - 0
  #
  # The integers to control the output of the deflate stream, which are
  # an argument for Deflate#deflate and so on.
  NO_FLUSH = _
  # From GzipFile#os_code - 0x01
  OS_AMIGA = _
  # From GzipFile#os_code - 0x05
  OS_ATARI = _
  # From GzipFile#os_code - code of current host
  OS_CODE = _
  # From GzipFile#os_code - 0x09
  OS_CPM = _
  # From GzipFile#os_code - 0x07
  OS_MACOS = _
  # From GzipFile#os_code - 0x00
  OS_MSDOS = _
  # From GzipFile#os_code - 0x06
  OS_OS2 = _
  # From GzipFile#os_code - 0x0c
  OS_QDOS = _
  # From GzipFile#os_code - 0x0d
  OS_RISCOS = _
  # From GzipFile#os_code - 0x0a
  OS_TOPS20 = _
  # From GzipFile#os_code - 0x03
  OS_UNIX = _
  # From GzipFile#os_code - 0xff
  OS_UNKNOWN = _
  # From GzipFile#os_code - 0x04
  OS_VMCMS = _
  # From GzipFile#os_code - 0x02
  OS_VMS = _
  # From GzipFile#os_code - 0x0b
  OS_WIN32 = _
  # From GzipFile#os_code - 0x08
  OS_ZSYSTEM = _
  # Output control - 2
  #
  # The integers to control the output of the deflate stream, which are
  # an argument for Deflate#deflate and so on.
  SYNC_FLUSH = _
  # Integer representing date types which
  # ZStream#data_type method returns
  UNKNOWN = _
  # The Ruby/zlib version string.
  VERSION = _
  # The string which represents the version of zlib.h
  ZLIB_VERSION = _

  # Calculates Adler-32 checksum for +string+, and returns updated value of
  # +adler+. If +string+ is omitted, it returns the Adler-32 initial value. If
  # +adler+ is omitted, it assumes that the initial value is given to +adler+.
  #
  # FIXME: expression.
  def self.adler32(string = nil, adler = nil) end

  # Combine two Adler-32 check values in to one.  +alder1+ is the first Adler-32
  # value, +adler2+ is the second Adler-32 value.  +len2+ is the length of the
  # string used to generate +adler2+.
  def self.adler32_combine(adler1, adler2, len2) end

  # Calculates CRC checksum for +string+, and returns updated value of +crc+. If
  # +string+ is omitted, it returns the CRC initial value. If +crc+ is omitted, it
  # assumes that the initial value is given to +crc+.
  #
  # FIXME: expression.
  def self.crc32(string = nil, adler = nil) end

  # Combine two CRC-32 check values in to one.  +crc1+ is the first CRC-32
  # value, +crc2+ is the second CRC-32 value.  +len2+ is the length of the
  # string used to generate +crc2+.
  def self.crc32_combine(crc1, crc2, len2) end

  # Returns the table for calculating CRC checksum as an array.
  def self.crc_table; end

  # Compresses the given +string+. Valid values of level are
  # <tt>NO_COMPRESSION</tt>, <tt>BEST_SPEED</tt>,
  # <tt>BEST_COMPRESSION</tt>, <tt>DEFAULT_COMPRESSION</tt>, and an
  # integer from 0 to 9 (the default is 6).
  #
  # This method is almost equivalent to the following code:
  #
  #   def deflate(string, level)
  #     z = Zlib::Deflate.new(level)
  #     dst = z.deflate(string, Zlib::NO_FLUSH)
  #     z.close
  #     dst
  #   end
  #
  # See also Zlib.inflate
  def self.deflate(string, level = nil) end

  # Decompresses +string+. Raises a Zlib::NeedDict exception if a preset
  # dictionary is needed for decompression.
  #
  # This method is almost equivalent to the following code:
  #
  #   def inflate(string)
  #     zstream = Zlib::Inflate.new
  #     buf = zstream.inflate(string)
  #     zstream.finish
  #     zstream.close
  #     buf
  #   end
  #
  # See also Zlib.deflate
  def self.inflate(string) end

  # Returns the string which represents the version of zlib library.
  def self.zlib_version; end

  private

  # Calculates Adler-32 checksum for +string+, and returns updated value of
  # +adler+. If +string+ is omitted, it returns the Adler-32 initial value. If
  # +adler+ is omitted, it assumes that the initial value is given to +adler+.
  #
  # FIXME: expression.
  def adler32(string, adler) end

  # Combine two Adler-32 check values in to one.  +alder1+ is the first Adler-32
  # value, +adler2+ is the second Adler-32 value.  +len2+ is the length of the
  # string used to generate +adler2+.
  def adler32_combine(adler1, adler2, len2) end

  # Calculates CRC checksum for +string+, and returns updated value of +crc+. If
  # +string+ is omitted, it returns the CRC initial value. If +crc+ is omitted, it
  # assumes that the initial value is given to +crc+.
  #
  # FIXME: expression.
  def crc32(string, adler) end

  # Combine two CRC-32 check values in to one.  +crc1+ is the first CRC-32
  # value, +crc2+ is the second CRC-32 value.  +len2+ is the length of the
  # string used to generate +crc2+.
  def crc32_combine(crc1, crc2, len2) end

  # Returns the table for calculating CRC checksum as an array.
  def crc_table; end

  # Returns the string which represents the version of zlib library.
  def zlib_version; end

  # Subclass of Zlib::Error when zlib returns a Z_BUF_ERROR.
  #
  # Usually if no progress is possible.
  class BufError < Error
  end

  # Subclass of Zlib::Error when zlib returns a Z_DATA_ERROR.
  #
  # Usually if a stream was prematurely freed.
  class DataError < Error
  end

  # Zlib::Deflate is the class for compressing data.  See Zlib::ZStream for more
  # information.
  class Deflate < ZStream
    # Compresses the given +string+. Valid values of level are
    # <tt>NO_COMPRESSION</tt>, <tt>BEST_SPEED</tt>,
    # <tt>BEST_COMPRESSION</tt>, <tt>DEFAULT_COMPRESSION</tt>, and an
    # integer from 0 to 9 (the default is 6).
    #
    # This method is almost equivalent to the following code:
    #
    #   def deflate(string, level)
    #     z = Zlib::Deflate.new(level)
    #     dst = z.deflate(string, Zlib::NO_FLUSH)
    #     z.close
    #     dst
    #   end
    #
    # See also Zlib.inflate
    def self.deflate(string, level = nil) end

    # == Arguments
    #
    # +level+::
    #   An Integer compression level between
    #   BEST_SPEED and BEST_COMPRESSION
    # +windowBits+::
    #   An Integer for the windowBits size. Should be
    #   in the range 8..15, larger values of this parameter
    #   result in better at the expense of memory usage.
    # +memlevel+::
    #   Specifies how much memory should be allocated for
    #   the internal compression state.
    #   Between DEF_MEM_LEVEL and MAX_MEM_LEVEL
    # +strategy+::
    #   A parameter to tune the compression algorithm. Use the
    #   DEFAULT_STRATEGY for normal data, FILTERED for data produced by a
    #   filter (or predictor), HUFFMAN_ONLY to force Huffman encoding only (no
    #   string match).
    #
    # == Description
    #
    # Creates a new deflate stream for compression. See zlib.h for details of
    # each argument. If an argument is nil, the default value of that argument is
    # used.
    #
    # == examples
    #
    # === basic
    #
    #   f = File.new("compressed.file","w+")
    #   #=> #<File:compressed.file>
    #   f << Zlib::Deflate.new().deflate(File.read("big.file"))
    #   #=> #<File:compressed.file>
    #   f.close
    #   #=> nil
    #
    # === a little more robust
    #
    #   compressed_file = File.open("compressed.file", "w+")
    #   #=> #<File:compressed.file>
    #   zd = Zlib::Deflate.new(Zlib::BEST_COMPRESSION, 15, Zlib::MAX_MEM_LEVEL, Zlib::HUFFMAN_ONLY)
    #   #=> #<Zlib::Deflate:0x000000008610a0>
    #   compressed_file << zd.deflate(File.read("big.file"))
    #   #=> "\xD4z\xC6\xDE\b\xA1K\x1Ej\x8A ..."
    #   compressed_file.close
    #   #=> nil
    #   zd.close
    #   #=> nil
    #
    # (while this example will work, for best optimization the flags need to be reviewed for your specific function)
    def initialize(level = nil, window_bits = nil, memlevel = nil, strategy = nil) end

    # Same as IO.
    def <<(p1) end

    # == Arguments
    #
    # +string+::
    #   String
    #
    # +flush+::
    #   Integer representing a flush code. Either NO_FLUSH,
    #   SYNC_FLUSH, FULL_FLUSH, or FINISH. See zlib.h for details.
    #   Normally the parameter flush is set to Z_NO_FLUSH, which allows deflate to
    #   decide how much data to accumulate before producing output, in order to
    #   maximize compression.
    #
    # == Description
    #
    # Inputs +string+ into the deflate stream and returns the output from the
    # stream.  On calling this method, both the input and the output buffers of
    # the stream are flushed.
    #
    # If +string+ is nil, this method finishes the
    # stream, just like Zlib::ZStream#finish.
    #
    # == Usage
    #
    #   comp = Zlib.deflate(File.read("big.file"))
    # or
    #   comp = Zlib.deflate(File.read("big.file"), Zlib::FULL_FLUSH)
    def deflate(string, *flush) end

    # This method is equivalent to <tt>deflate('', flush)</tt>.  If flush is omitted,
    # <tt>SYNC_FLUSH</tt> is used as flush.  This method is just provided
    # to improve the readability of your Ruby program.
    #
    # Please visit your zlib.h for a deeper detail on NO_FLUSH, SYNC_FLUSH, FULL_FLUSH, and FINISH
    def flush(flush) end

    # Duplicates the deflate stream.
    def initialize_copy(p1) end

    # Changes the parameters of the deflate stream. See zlib.h for details. The
    # output from the stream by changing the params is preserved in output
    # buffer.
    #
    # +level+::
    #   An Integer compression level between
    #   BEST_SPEED and BEST_COMPRESSION
    # +strategy+::
    #   A parameter to tune the compression algorithm. Use the
    #   DEFAULT_STRATEGY for normal data, FILTERED for data produced by a
    #   filter (or predictor), HUFFMAN_ONLY to force Huffman encoding only (no
    #   string match).
    def params(level, strategy) end

    # Sets the preset dictionary and returns +string+. This method is available
    # just only after Zlib::Deflate.new or Zlib::ZStream#reset method was called.
    # See zlib.h for details.
    #
    # Can raise errors of Z_STREAM_ERROR if a parameter is invalid (such as
    # NULL dictionary) or the stream state is inconsistent, Z_DATA_ERROR if
    # the given dictionary doesn't match the expected one (incorrect adler32 value)
    def set_dictionary(string) end
  end

  # The superclass for all exceptions raised by Ruby/zlib.
  #
  # The following exceptions are defined as subclasses of Zlib::Error. These
  # exceptions are raised when zlib library functions return with an error
  # status.
  #
  # - Zlib::StreamEnd
  # - Zlib::NeedDict
  # - Zlib::DataError
  # - Zlib::StreamError
  # - Zlib::MemError
  # - Zlib::BufError
  # - Zlib::VersionError
  class Error < StandardError
  end

  # Zlib::GzipFile is an abstract class for handling a gzip formatted
  # compressed file. The operations are defined in the subclasses,
  # Zlib::GzipReader for reading, and Zlib::GzipWriter for writing.
  #
  # GzipReader should be used by associating an IO, or IO-like, object.
  #
  # == Method Catalogue
  #
  # - ::wrap
  # - ::open (Zlib::GzipReader::open and Zlib::GzipWriter::open)
  # - #close
  # - #closed?
  # - #comment
  # - comment= (Zlib::GzipWriter#comment=)
  # - #crc
  # - eof? (Zlib::GzipReader#eof?)
  # - #finish
  # - #level
  # - lineno (Zlib::GzipReader#lineno)
  # - lineno= (Zlib::GzipReader#lineno=)
  # - #mtime
  # - mtime= (Zlib::GzipWriter#mtime=)
  # - #orig_name
  # - orig_name (Zlib::GzipWriter#orig_name=)
  # - #os_code
  # - path (when the underlying IO supports #path)
  # - #sync
  # - #sync=
  # - #to_io
  #
  # (due to internal structure, documentation may appear under Zlib::GzipReader
  # or Zlib::GzipWriter)
  class GzipFile
    # Creates a GzipFile object associated with +io+, and
    # executes the block with the newly created GzipFile object,
    # just like File.open. The GzipFile object will be closed
    # automatically after executing the block. If you want to keep
    # the associated IO object opening, you may call
    # +Zlib::GzipFile#finish+ method in the block.
    def self.wrap(io) end

    # Closes the GzipFile object. This method calls close method of the
    # associated IO object. Returns the associated IO object.
    def close; end

    # Same as IO#closed?
    def closed?; end

    # Returns comments recorded in the gzip file header, or nil if the comments
    # is not present.
    def comment; end

    # Returns CRC value of the uncompressed data.
    def crc; end

    # Closes the GzipFile object. Unlike Zlib::GzipFile#close, this method never
    # calls the close method of the associated IO object. Returns the associated IO
    # object.
    def finish; end

    # Returns compression level.
    def level; end

    # Returns last modification time recorded in the gzip file header.
    def mtime; end

    # Returns original filename recorded in the gzip file header, or +nil+ if
    # original filename is not present.
    def orig_name; end

    # Returns OS code number recorded in the gzip file header.
    def os_code; end

    # Same as IO#sync
    def sync; end

    # Same as IO.  If flag is +true+, the associated IO object must respond to the
    # +flush+ method.  While +sync+ mode is +true+, the compression ratio
    # decreases sharply.
    def sync=(flag) end

    # Same as IO.
    def to_io; end

    # Raised when the CRC checksum recorded in gzip file footer is not equivalent
    # to the CRC checksum of the actual uncompressed data.
    class CRCError < Error
    end

    # Base class of errors that occur when processing GZIP files.
    class Error < Error
      # Constructs a String of the GzipFile Error
      def inspect; end
    end

    # Raised when the data length recorded in the gzip file footer is not equivalent
    # to the length of the actual uncompressed data.
    class LengthError < Error
    end

    # Raised when gzip file footer is not found.
    class NoFooter < Error
    end
  end

  # Zlib::GzipReader is the class for reading a gzipped file.  GzipReader should
  # be used an IO, or -IO-lie, object.
  #
  #   Zlib::GzipReader.open('hoge.gz') {|gz|
  #     print gz.read
  #   }
  #
  #   File.open('hoge.gz') do |f|
  #     gz = Zlib::GzipReader.new(f)
  #     print gz.read
  #     gz.close
  #   end
  #
  # == Method Catalogue
  #
  # The following methods in Zlib::GzipReader are just like their counterparts
  # in IO, but they raise Zlib::Error or Zlib::GzipFile::Error exception if an
  # error was found in the gzip file.
  # - #each
  # - #each_line
  # - #each_byte
  # - #gets
  # - #getc
  # - #lineno
  # - #lineno=
  # - #read
  # - #readchar
  # - #readline
  # - #readlines
  # - #ungetc
  #
  # Be careful of the footer of the gzip file. A gzip file has the checksum of
  # pre-compressed data in its footer. GzipReader checks all uncompressed data
  # against that checksum at the following cases, and if it fails, raises
  # <tt>Zlib::GzipFile::NoFooter</tt>, <tt>Zlib::GzipFile::CRCError</tt>, or
  # <tt>Zlib::GzipFile::LengthError</tt> exception.
  #
  # - When an reading request is received beyond the end of file (the end of
  #   compressed data). That is, when Zlib::GzipReader#read,
  #   Zlib::GzipReader#gets, or some other methods for reading returns nil.
  # - When Zlib::GzipFile#close method is called after the object reaches the
  #   end of file.
  # - When Zlib::GzipReader#unused method is called after the object reaches
  #   the end of file.
  #
  # The rest of the methods are adequately described in their own
  # documentation.
  class GzipReader < GzipFile
    include Enumerable

    # Opens a file specified by +filename+ as a gzipped file, and returns a
    # GzipReader object associated with that file.  Further details of this method
    # are in Zlib::GzipReader.new and ZLib::GzipFile.wrap.
    def self.open(filename) end

    # Creates a GzipReader object associated with +io+. The GzipReader object reads
    # gzipped data from +io+, and parses/decompresses them.  At least, +io+ must have
    # a +read+ method that behaves same as the +read+ method in IO class.
    #
    # If the gzip file header is incorrect, raises an Zlib::GzipFile::Error
    # exception.
    def initialize(io) end

    # See Zlib::GzipReader documentation for a description.
    def each(*args) end
    alias each_line each
    alias lines each

    # See Zlib::GzipReader documentation for a description.
    def each_byte; end
    alias bytes each_byte

    # See Zlib::GzipReader documentation for a description.
    def each_char; end

    # Returns +true+ or +false+ whether the stream has reached the end.
    def eof; end
    alias eof? eof

    # See Zlib::GzipReader documentation for a description.
    def getbyte; end

    # See Zlib::GzipReader documentation for a description.
    def getc; end

    # See Zlib::GzipReader documentation for a description.
    def gets(*args) end

    # The line number of the last row read from this file.
    def lineno; end

    # Specify line number of the last row read from this file.
    def lineno=(p1) end

    # Total number of output bytes output so far.
    def pos; end
    alias tell pos

    # See Zlib::GzipReader documentation for a description.
    def read(p1 = v1) end

    # See Zlib::GzipReader documentation for a description.
    def readbyte; end

    # See Zlib::GzipReader documentation for a description.
    def readchar; end

    # See Zlib::GzipReader documentation for a description.
    def readline(*args) end

    # See Zlib::GzipReader documentation for a description.
    def readlines(*args) end

    # Reads at most <i>maxlen</i> bytes from the gziped stream but
    # it blocks only if <em>gzipreader</em> has no data immediately available.
    # If the optional <i>outbuf</i> argument is present,
    # it must reference a String, which will receive the data.
    # It raises <code>EOFError</code> on end of file.
    def readpartial(p1, p2 = v2) end

    # Resets the position of the file pointer to the point created the GzipReader
    # object.  The associated IO object needs to respond to the +seek+ method.
    def rewind; end

    # See Zlib::GzipReader documentation for a description.
    def ungetbyte(p1) end

    # See Zlib::GzipReader documentation for a description.
    def ungetc(p1) end

    # Returns the rest of the data which had read for parsing gzip format, or
    # +nil+ if the whole gzip file is not parsed yet.
    def unused; end
  end

  # Zlib::GzipWriter is a class for writing gzipped files.  GzipWriter should
  # be used with an instance of IO, or IO-like, object.
  #
  # Following two example generate the same result.
  #
  #   Zlib::GzipWriter.open('hoge.gz') do |gz|
  #     gz.write 'jugemu jugemu gokou no surikire...'
  #   end
  #
  #   File.open('hoge.gz', 'w') do |f|
  #     gz = Zlib::GzipWriter.new(f)
  #     gz.write 'jugemu jugemu gokou no surikire...'
  #     gz.close
  #   end
  #
  # To make like gzip(1) does, run following:
  #
  #   orig = 'hoge.txt'
  #   Zlib::GzipWriter.open('hoge.gz') do |gz|
  #     gz.mtime = File.mtime(orig)
  #     gz.orig_name = orig
  #     gz.write IO.binread(orig)
  #   end
  #
  # NOTE: Due to the limitation of Ruby's finalizer, you must explicitly close
  # GzipWriter objects by Zlib::GzipWriter#close etc.  Otherwise, GzipWriter
  # will be not able to write the gzip footer and will generate a broken gzip
  # file.
  class GzipWriter < GzipFile
    # Opens a file specified by +filename+ for writing gzip compressed data, and
    # returns a GzipWriter object associated with that file.  Further details of
    # this method are found in Zlib::GzipWriter.new and Zlib::GzipFile.wrap.
    def self.open(filename, level = nil, strategy = nil) end

    # Creates a GzipWriter object associated with +io+. +level+ and +strategy+
    # should be the same as the arguments of Zlib::Deflate.new.  The GzipWriter
    # object writes gzipped data to +io+.  At least, +io+ must respond to the
    # +write+ method that behaves same as write method in IO class.
    def initialize(io, level, strategy) end

    # Same as IO.
    def <<(p1) end

    # Specify the comment (+str+) in the gzip header.
    def comment=(p1) end

    # Flushes all the internal buffers of the GzipWriter object.  The meaning of
    # +flush+ is same as in Zlib::Deflate#deflate.  <tt>Zlib::SYNC_FLUSH</tt> is used if
    # +flush+ is omitted.  It is no use giving flush <tt>Zlib::NO_FLUSH</tt>.
    def flush(flush = nil) end

    # Specify the modification time (+mtime+) in the gzip header.
    # Using a Fixnum or Integer
    def mtime=(p1) end

    # Specify the original name (+str+) in the gzip header.
    def orig_name=(p1) end

    # Total number of input bytes read so far.
    def pos; end
    alias tell pos

    # Same as IO.
    def print(*args) end

    # Same as IO.
    def printf(*args) end

    # Same as IO.
    def putc(p1) end

    # Same as IO.
    def puts(*args) end

    # Same as IO.
    def write(p1) end
  end

  # Zlib:Inflate is the class for decompressing compressed data.  Unlike
  # Zlib::Deflate, an instance of this class is not able to duplicate (clone,
  # dup) itself.
  class Inflate < ZStream
    # Decompresses +string+. Raises a Zlib::NeedDict exception if a preset
    # dictionary is needed for decompression.
    #
    # This method is almost equivalent to the following code:
    #
    #   def inflate(string)
    #     zstream = Zlib::Inflate.new
    #     buf = zstream.inflate(string)
    #     zstream.finish
    #     zstream.close
    #     buf
    #   end
    #
    # See also Zlib.deflate
    def self.inflate(string) end

    # == Arguments
    #
    # +windowBits+::
    #   An Integer for the windowBits size. Should be
    #   in the range 8..15, larger values of this parameter
    #   result in better at the expense of memory usage.
    #
    # == Description
    #
    # Creates a new inflate stream for decompression. See zlib.h for details
    # of the argument.  If +window_bits+ is +nil+, the default value is used.
    #
    # == Example
    #
    #   cf = File.open("compressed.file")
    #   ucf = File.open("uncompressed.file", "w+")
    #   zi = Zlib::Inflate.new(Zlib::MAX_WBITS)
    #
    #   ucf << zi.inflate(cf.read)
    #
    #   ucf.close
    #   zi.close
    #   cf.close
    #
    # or
    #
    #   File.open("compressed.file") {|cf|
    #     zi = Zlib::Inflate.new
    #     File.open("uncompressed.file", "w+") {|ucf|
    #       ucf << zi.inflate(cf.read)
    #     }
    #     zi.close
    #   }
    def initialize(window_bits) end

    # Same as IO.
    def <<(p1) end

    # Inputs +string+ into the inflate stream and returns the output from the
    # stream.  Calling this method, both the input and the output buffer of the
    # stream are flushed.  If string is +nil+, this method finishes the stream,
    # just like Zlib::ZStream#finish.
    #
    # Raises a Zlib::NeedDict exception if a preset dictionary is needed to
    # decompress.  Set the dictionary by Zlib::Inflate#set_dictionary and then
    # call this method again with an empty string to flush the stream:
    #
    #   inflater = Zlib::Inflate.new
    #
    #   begin
    #     out = inflater.inflate compressed
    #   rescue Zlib::NeedDict
    #     # ensure the dictionary matches the stream's required dictionary
    #     raise unless inflater.adler == Zlib.adler32(dictionary)
    #
    #     inflater.set_dictionary dictionary
    #     inflater.inflate ''
    #   end
    #
    #   # ...
    #
    #   inflater.close
    #
    # See also Zlib::Inflate.new
    def inflate(string) end

    # Sets the preset dictionary and returns +string+.  This method is available just
    # only after a Zlib::NeedDict exception was raised.  See zlib.h for details.
    def set_dictionary(p1) end

    # Inputs +string+ into the end of input buffer and skips data until a full
    # flush point can be found.  If the point is found in the buffer, this method
    # flushes the buffer and returns false.  Otherwise it returns +true+ and the
    # following data of full flush point is preserved in the buffer.
    def sync(string) end

    # Quoted verbatim from original documentation:
    #
    #   What is this?
    #
    # <tt>:)</tt>
    def sync_point?; end
  end

  # Subclass of Zlib::Error
  #
  # When zlib returns a Z_MEM_ERROR,
  # usually if there was not enough memory.
  class MemError < Error
  end

  # Subclass of Zlib::Error
  #
  # When zlib returns a Z_NEED_DICT
  # if a preset dictionary is needed at this point.
  #
  # Used by Zlib::Inflate.inflate and <tt>Zlib.inflate</tt>
  class NeedDict < Error
  end

  # Subclass of Zlib::Error
  #
  # When zlib returns a Z_STREAM_END
  # is return if the end of the compressed data has been reached
  # and all uncompressed out put has been produced.
  class StreamEnd < Error
  end

  # Subclass of Zlib::Error
  #
  # When zlib returns a Z_STREAM_ERROR,
  # usually if the stream state was inconsistent.
  class StreamError < Error
  end

  # Subclass of Zlib::Error
  #
  # When zlib returns a Z_VERSION_ERROR,
  # usually if the zlib library version is incompatible with the
  # version assumed by the caller.
  class VersionError < Error
  end

  # Zlib::ZStream is the abstract class for the stream which handles the
  # compressed data. The operations are defined in the subclasses:
  # Zlib::Deflate for compression, and Zlib::Inflate for decompression.
  #
  # An instance of Zlib::ZStream has one stream (struct zstream in the source)
  # and two variable-length buffers which associated to the input (next_in) of
  # the stream and the output (next_out) of the stream. In this document,
  # "input buffer" means the buffer for input, and "output buffer" means the
  # buffer for output.
  #
  # Data input into an instance of Zlib::ZStream are temporally stored into
  # the end of input buffer, and then data in input buffer are processed from
  # the beginning of the buffer until no more output from the stream is
  # produced (i.e. until avail_out > 0 after processing).  During processing,
  # output buffer is allocated and expanded automatically to hold all output
  # data.
  #
  # Some particular instance methods consume the data in output buffer and
  # return them as a String.
  #
  # Here is an ascii art for describing above:
  #
  #    +================ an instance of Zlib::ZStream ================+
  #    ||                                                            ||
  #    ||     +--------+          +-------+          +--------+      ||
  #    ||  +--| output |<---------|zstream|<---------| input  |<--+  ||
  #    ||  |  | buffer |  next_out+-------+next_in   | buffer |   |  ||
  #    ||  |  +--------+                             +--------+   |  ||
  #    ||  |                                                      |  ||
  #    +===|======================================================|===+
  #        |                                                      |
  #        v                                                      |
  #    "output data"                                         "input data"
  #
  # If an error occurs during processing input buffer, an exception which is a
  # subclass of Zlib::Error is raised.  At that time, both input and output
  # buffer keep their conditions at the time when the error occurs.
  #
  # == Method Catalogue
  #
  # Many of the methods in this class are fairly low-level and unlikely to be
  # of interest to users.  In fact, users are unlikely to use this class
  # directly; rather they will be interested in Zlib::Inflate and
  # Zlib::Deflate.
  #
  # The higher level methods are listed below.
  #
  # - #total_in
  # - #total_out
  # - #data_type
  # - #adler
  # - #reset
  # - #finish
  # - #finished?
  # - #close
  # - #closed?
  class ZStream
    # Returns the adler-32 checksum.
    def adler; end

    # Returns bytes of data in the input buffer. Normally, returns 0.
    def avail_in; end

    # Returns number of bytes of free spaces in output buffer.  Because the free
    # space is allocated automatically, this method returns 0 normally.
    def avail_out; end

    # Allocates +size+ bytes of free space in the output buffer. If there are more
    # than +size+ bytes already in the buffer, the buffer is truncated. Because
    # free space is allocated automatically, you usually don't need to use this
    # method.
    def avail_out=(p1) end

    # Closes the stream. All operations on the closed stream will raise an
    # exception.
    def close; end
    alias end close

    # Returns true if the stream is closed.
    def closed?; end
    alias ended? closed?

    # Guesses the type of the data which have been inputed into the stream. The
    # returned value is either <tt>BINARY</tt>, <tt>ASCII</tt>, or
    # <tt>UNKNOWN</tt>.
    def data_type; end

    # Finishes the stream and flushes output buffer. See Zlib::Deflate#finish and
    # Zlib::Inflate#finish for details of this behavior.
    def finish; end

    # Returns true if the stream is finished.
    def finished?; end
    alias stream_end? finished?

    # Flushes input buffer and returns all data in that buffer.
    def flush_next_in; end

    # Flushes output buffer and returns all data in that buffer.
    def flush_next_out; end

    # Resets and initializes the stream. All data in both input and output buffer
    # are discarded.
    def reset; end

    # Returns the total bytes of the input data to the stream.  FIXME
    def total_in; end

    # Returns the total bytes of the output data from the stream.  FIXME
    def total_out; end
  end
end
