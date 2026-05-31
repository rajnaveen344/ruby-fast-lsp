# frozen_string_literal: true

# A bridge to the dlopen() or dynamic library linker function.
#
# == Example
#
#   bash $> cat > sum.c <<EOF
#   double sum(double *arry, int len)
#   {
#           double ret = 0;
#           int i;
#           for(i = 0; i < len; i++){
#                   ret = ret + arry[i];
#           }
#           return ret;
#   }
#
#   double split(double num)
#   {
#           double ret = 0;
#           ret = num / 2;
#           return ret;
#   }
#   EOF
#   bash $> gcc -o libsum.so -shared sum.c
#   bash $> cat > sum.rb <<EOF
#   require 'dl'
#   require 'dl/import'
#
#   module LibSum
#           extend DL::Importer
#           dlload './libsum.so'
#           extern 'double sum(double*, int)'
#           extern 'double split(double)'
#   end
#
#   a = [2.0, 3.0, 4.0]
#
#   sum = LibSum.sum(a.pack("d*"), a.count)
#   p LibSum.split(sum)
#   EOF
#   bash $> ruby sum.rb
#   4.5
#
# WIN! :-)
module DL
  # ALIGN_CHAR
  #
  # The alignment size of a char
  ALIGN_CHAR = _
  # ALIGN_DOUBLE
  #
  # The alignment size of a double
  ALIGN_DOUBLE = _
  # ALIGN_FLOAT
  #
  # The alignment size of a float
  ALIGN_FLOAT = _
  # ALIGN_INT
  #
  # The alignment size of an int
  ALIGN_INT = _
  # ALIGN_INTPTR_T
  #
  # The alignment size of a intptr_t
  ALIGN_INTPTR_T = _
  # ALIGN_LONG
  #
  # The alignment size of a long
  ALIGN_LONG = _
  # ALIGN_LONG_LONG
  #
  # The alignment size of a long long
  ALIGN_LONG_LONG = _
  # ALIGN_PTRDIFF_T
  #
  # The alignment size of a ptrdiff_t
  ALIGN_PTRDIFF_T = _
  # ALIGN_SHORT
  #
  # The alignment size of a short
  ALIGN_SHORT = _
  # ALIGN_SIZE_T
  #
  # The alignment size of a size_t
  ALIGN_SIZE_T = _
  # ALIGN_SSIZE_T
  #
  # The alignment size of a ssize_t
  ALIGN_SSIZE_T = _
  # ALIGN_UINTPTR_T
  #
  # The alignment size of a uintptr_t
  ALIGN_UINTPTR_T = _
  # ALIGN_VOIDP
  #
  # The alignment size of a void*
  ALIGN_VOIDP = _
  # BUILD_RUBY_PLATFORM
  #
  # Platform built against (i.e. "x86_64-linux", etc.)
  #
  # See also RUBY_PLATFORM
  BUILD_RUBY_PLATFORM = _
  # BUILD_RUBY_VERSION
  #
  # Ruby Version built. (i.e. "1.9.3")
  #
  # See also RUBY_VERSION
  BUILD_RUBY_VERSION = _
  # DLSTACK_SIZE
  #
  # Dynamic linker stack size
  DLSTACK_SIZE = _
  # MAX_CALLBACK
  #
  # Maximum number of callbacks
  MAX_CALLBACK = _
  # RTLD_GLOBAL
  #
  # rtld DL::Handle flag.
  #
  # The symbols defined by this library will be made available for symbol
  # resolution of subsequently loaded libraries.
  RTLD_GLOBAL = _
  # RTLD_LAZY
  #
  # rtld DL::Handle flag.
  #
  # Perform lazy binding.  Only resolve symbols as the code that references
  # them is executed.  If the  symbol is never referenced, then it is never
  # resolved.  (Lazy binding is only performed for function references;
  # references to variables are always immediately bound when the library
  # is loaded.)
  RTLD_LAZY = _
  # RTLD_NOW
  #
  # rtld DL::Handle flag.
  #
  # If this value is specified or the environment variable LD_BIND_NOW is
  # set to a nonempty string, all undefined symbols in the library are
  # resolved before dlopen() returns.  If this cannot be done an error is
  # returned.
  RTLD_NOW = _
  # RUBY_FREE
  #
  # Address of the ruby_xfree() function
  RUBY_FREE = _
  # SIZEOF_CHAR
  #
  # size of a char
  SIZEOF_CHAR = _
  # SIZEOF_DOUBLE
  #
  # size of a double
  SIZEOF_DOUBLE = _
  # SIZEOF_FLOAT
  #
  # size of a float
  SIZEOF_FLOAT = _
  # SIZEOF_INT
  #
  # size of an int
  SIZEOF_INT = _
  # SIZEOF_INTPTR_T
  #
  # size of a intptr_t
  SIZEOF_INTPTR_T = _
  # SIZEOF_LONG
  #
  # size of a long
  SIZEOF_LONG = _
  # SIZEOF_LONG_LONG
  #
  # size of a long long
  SIZEOF_LONG_LONG = _
  # SIZEOF_PTRDIFF_T
  #
  # size of a ptrdiff_t
  SIZEOF_PTRDIFF_T = _
  # SIZEOF_SHORT
  #
  # size of a short
  SIZEOF_SHORT = _
  # SIZEOF_SIZE_T
  #
  # size of a size_t
  SIZEOF_SIZE_T = _
  # SIZEOF_SSIZE_T
  #
  # size of a ssize_t
  SIZEOF_SSIZE_T = _
  # SIZEOF_UINTPTR_T
  #
  # size of a uintptr_t
  SIZEOF_UINTPTR_T = _
  # SIZEOF_VOIDP
  #
  # size of a void*
  SIZEOF_VOIDP = _
  # TYPE_CHAR
  #
  # DL::CFunc type - char
  TYPE_CHAR = _
  # TYPE_DOUBLE
  #
  # DL::CFunc type - double
  TYPE_DOUBLE = _
  # TYPE_FLOAT
  #
  # DL::CFunc type - float
  TYPE_FLOAT = _
  # TYPE_INT
  #
  # DL::CFunc type - int
  TYPE_INT = _
  # TYPE_INTPTR_T
  #
  # DL::CFunc type - intptr_t
  TYPE_INTPTR_T = _
  # TYPE_LONG
  #
  # DL::CFunc type - long
  TYPE_LONG = _
  # TYPE_LONG_LONG
  #
  # DL::CFunc type - long long
  TYPE_LONG_LONG = _
  # TYPE_PTRDIFF_T
  #
  # DL::CFunc type - ptrdiff_t
  TYPE_PTRDIFF_T = _
  # TYPE_SHORT
  #
  # DL::CFunc type - short
  TYPE_SHORT = _
  # TYPE_SIZE_T
  #
  # DL::CFunc type - size_t
  TYPE_SIZE_T = _
  # TYPE_SSIZE_T
  #
  # DL::CFunc type - ssize_t
  TYPE_SSIZE_T = _
  # TYPE_UINTPTR_T
  #
  # DL::CFunc type - uintptr_t
  TYPE_UINTPTR_T = _
  # TYPE_VOID
  #
  # DL::CFunc type - void
  TYPE_VOID = _
  # TYPE_VOIDP
  #
  # DL::CFunc type - void*
  TYPE_VOIDP = _

  # An interface to the dynamic linking loader
  #
  # This is a shortcut to DL::Handle.new and takes the same arguments.
  #
  # Example:
  #
  #   libc_so = "/lib64/libc.so.6"
  #   => "/lib64/libc.so.6"
  #
  #   libc = DL.dlopen(libc_so)
  #   => #<DL::Handle:0x00000000e05b00>
  def self.dlopen(so_lib) end

  # Returns the hexadecimal representation of a memory pointer address +addr+
  #
  # Example:
  #
  #   lib = DL.dlopen('/lib64/libc-2.15.so')
  #   => #<DL::Handle:0x00000001342460>
  #
  #   lib['strcpy'].to_s(16)
  #   => "7f59de6dd240"
  #
  #   DL.dlunwrap(DL.dlwrap(lib['strcpy'].to_s(16)))
  #   => "7f59de6dd240"
  def self.dlunwrap(addr) end

  # Returns a memory pointer of a function's hexadecimal address location +val+
  #
  # Example:
  #
  #   lib = DL.dlopen('/lib64/libc-2.15.so')
  #   => #<DL::Handle:0x00000001342460>
  #
  #   DL.dlwrap(lib['strcpy'].to_s(16))
  #   => 25522520
  def self.dlwrap(val) end

  # Free the memory at address +addr+
  def self.free(addr) end

  # Allocate +size+ bytes of memory and return the integer memory address
  # for the allocated memory.
  def self.malloc(size) end

  # Change the size of the memory allocated at the memory location +addr+ to
  # +size+ bytes.  Returns the memory address of the reallocated memory, which
  # may be different than the address passed in.
  def self.realloc(addr, size) end

  private

  # An interface to the dynamic linking loader
  #
  # This is a shortcut to DL::Handle.new and takes the same arguments.
  #
  # Example:
  #
  #   libc_so = "/lib64/libc.so.6"
  #   => "/lib64/libc.so.6"
  #
  #   libc = DL.dlopen(libc_so)
  #   => #<DL::Handle:0x00000000e05b00>
  def dlopen(so_lib) end

  # Returns the hexadecimal representation of a memory pointer address +addr+
  #
  # Example:
  #
  #   lib = DL.dlopen('/lib64/libc-2.15.so')
  #   => #<DL::Handle:0x00000001342460>
  #
  #   lib['strcpy'].to_s(16)
  #   => "7f59de6dd240"
  #
  #   DL.dlunwrap(DL.dlwrap(lib['strcpy'].to_s(16)))
  #   => "7f59de6dd240"
  def dlunwrap(addr) end

  # Returns a memory pointer of a function's hexadecimal address location +val+
  #
  # Example:
  #
  #   lib = DL.dlopen('/lib64/libc-2.15.so')
  #   => #<DL::Handle:0x00000001342460>
  #
  #   DL.dlwrap(lib['strcpy'].to_s(16))
  #   => 25522520
  def dlwrap(val) end

  # Free the memory at address +addr+
  def free(addr) end

  # Allocate +size+ bytes of memory and return the integer memory address
  # for the allocated memory.
  def malloc(size) end

  # Change the size of the memory allocated at the memory location +addr+ to
  # +size+ bytes.  Returns the memory address of the reallocated memory, which
  # may be different than the address passed in.
  def realloc(addr, size) end

  # standard dynamic load exception
  class DLError < StandardError
  end

  # dynamic load incorrect type exception
  class DLTypeError < DLError
  end

  # The DL::Handle is the manner to access the dynamic library
  #
  # == Example
  #
  # === Setup
  #
  #   libc_so = "/lib64/libc.so.6"
  #   => "/lib64/libc.so.6"
  #   @handle = DL::Handle.new(libc_so)
  #   => #<DL::Handle:0x00000000d69ef8>
  #
  # === Setup, with flags
  #
  #   libc_so = "/lib64/libc.so.6"
  #   => "/lib64/libc.so.6"
  #   @handle = DL::Handle.new(libc_so, DL::RTLD_LAZY | DL::RTLD_GLOBAL)
  #   => #<DL::Handle:0x00000000d69ef8>
  #
  # === Addresses to symbols
  #
  #   strcpy_addr = @handle['strcpy']
  #   => 140062278451968
  #
  # or
  #
  #   strcpy_addr = @handle.sym('strcpy')
  #   => 140062278451968
  class Handle
    # DEFAULT
    #
    # A predefined pseudo-handle of RTLD_DEFAULT
    #
    # Which will find the first occurrence of the desired symbol using the
    # default library search order
    DEFAULT = _
    # NEXT
    #
    # A predefined pseudo-handle of RTLD_NEXT
    #
    # Which will find the next occurrence of a function in the search order
    # after the current library.
    NEXT = _

    # Get the address as an Integer for the function named +name+.
    def self.[](p1) end

    # Get the address as an Integer for the function named +name+.
    def self.sym(name) end

    # Create a new handler that opens library named +lib+ with +flags+.  If no
    # library is specified, RTLD_DEFAULT is used.
    def initialize(lib = nil, flags = DL::RTLD_LAZY | DL::RTLD_GLOBAL) end

    # Close this DL::Handle.  Calling close more than once will raise a
    # DL::DLError exception.
    def close; end

    # Returns +true+ if dlclose() will be called when this DL::Handle is
    # garbage collected.
    def close_enabled?; end

    # Disable a call to dlclose() when this DL::Handle is garbage collected.
    def disable_close; end

    # Enable a call to dlclose() when this DL::Handle is garbage collected.
    def enable_close; end

    # Get the address as an Integer for the function named +name+.
    def sym(name) end
    alias [] sym

    # Returns the memory address for this handle.
    def to_i; end
  end
end
