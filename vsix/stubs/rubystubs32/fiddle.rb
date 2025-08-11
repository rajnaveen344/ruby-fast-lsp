# frozen_string_literal: true

# A libffi wrapper for Ruby.
#
# == Description
#
# Fiddle is an extension to translate a foreign function interface (FFI)
# with ruby.
#
# It wraps {libffi}[http://sourceware.org/libffi/], a popular C library
# which provides a portable interface that allows code written in one
# language to call code written in another language.
#
# == Example
#
# Here we will use Fiddle::Function to wrap {floor(3) from
# libm}[http://linux.die.net/man/3/floor]
#
#      require 'fiddle'
#
#      libm = Fiddle.dlopen('/lib/libm.so.6')
#
#      floor = Fiddle::Function.new(
#        libm['floor'],
#        [Fiddle::TYPE_DOUBLE],
#        Fiddle::TYPE_DOUBLE
#      )
#
#      puts floor.call(3.14159) #=> 3.0
module Fiddle
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
  # ALIGN_INT16_T
  #
  # The alignment size of a int16_t
  ALIGN_INT16_T = _
  # ALIGN_INT32_T
  #
  # The alignment size of a int32_t
  ALIGN_INT32_T = _
  # ALIGN_INT64_T
  #
  # The alignment size of a int64_t
  ALIGN_INT64_T = _
  # ALIGN_INT8_T
  #
  # The alignment size of a int8_t
  ALIGN_INT8_T = _
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
  # Qfalse
  #
  # The value of Qfalse
  Qfalse = _
  # Qnil
  #
  # The value of Qnil
  Qnil = _
  # Qtrue
  #
  # The value of Qtrue
  Qtrue = _
  # Qundef
  #
  # The value of Qundef
  Qundef = _
  # RUBY_FREE
  #
  # Address of the ruby_xfree() function
  RUBY_FREE = _
  # SIZEOF_CHAR
  #
  # size of a char
  SIZEOF_CHAR = _
  # SIZEOF_CONST_STRING
  #
  # size of a const char*
  SIZEOF_CONST_STRING = _
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
  # SIZEOF_INT16_T
  #
  # size of a int16_t
  SIZEOF_INT16_T = _
  # SIZEOF_INT32_T
  #
  # size of a int32_t
  SIZEOF_INT32_T = _
  # SIZEOF_INT64_T
  #
  # size of a int64_t
  SIZEOF_INT64_T = _
  # SIZEOF_INT8_T
  #
  # size of a int8_t
  SIZEOF_INT8_T = _
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
  # SIZEOF_UCHAR
  #
  # size of a unsigned char
  SIZEOF_UCHAR = _
  # SIZEOF_UINT
  #
  # size of an unsigned int
  SIZEOF_UINT = _
  # SIZEOF_UINT16_T
  #
  # size of a uint16_t
  SIZEOF_UINT16_T = _
  # SIZEOF_UINT32_T
  #
  # size of a uint32_t
  SIZEOF_UINT32_T = _
  # SIZEOF_UINT64_T
  #
  # size of a uint64_t
  SIZEOF_UINT64_T = _
  # SIZEOF_UINT8_T
  #
  # size of a uint8_t
  SIZEOF_UINT8_T = _
  # SIZEOF_UINTPTR_T
  #
  # size of a uintptr_t
  SIZEOF_UINTPTR_T = _
  # SIZEOF_ULONG
  #
  # size of a unsigned long
  SIZEOF_ULONG = _
  # SIZEOF_ULONG_LONG
  #
  # size of a unsigned long long
  SIZEOF_ULONG_LONG = _
  # SIZEOF_USHORT
  #
  # size of a unsigned short
  SIZEOF_USHORT = _
  # SIZEOF_VOIDP
  #
  # size of a void*
  SIZEOF_VOIDP = _
  # Returns a boolean regarding whether the host is WIN32
  WINDOWS = _

  # Returns the hexadecimal representation of a memory pointer address +addr+
  #
  # Example:
  #
  #   lib = Fiddle.dlopen('/lib64/libc-2.15.so')
  #   => #<Fiddle::Handle:0x00000001342460>
  #
  #   lib['strcpy'].to_s(16)
  #   => "7f59de6dd240"
  #
  #   Fiddle.dlunwrap(Fiddle.dlwrap(lib['strcpy'].to_s(16)))
  #   => "7f59de6dd240"
  def self.dlunwrap(addr) end

  # Returns a memory pointer of a function's hexadecimal address location +val+
  #
  # Example:
  #
  #   lib = Fiddle.dlopen('/lib64/libc-2.15.so')
  #   => #<Fiddle::Handle:0x00000001342460>
  #
  #   Fiddle.dlwrap(lib['strcpy'].to_s(16))
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

  # Returns the hexadecimal representation of a memory pointer address +addr+
  #
  # Example:
  #
  #   lib = Fiddle.dlopen('/lib64/libc-2.15.so')
  #   => #<Fiddle::Handle:0x00000001342460>
  #
  #   lib['strcpy'].to_s(16)
  #   => "7f59de6dd240"
  #
  #   Fiddle.dlunwrap(Fiddle.dlwrap(lib['strcpy'].to_s(16)))
  #   => "7f59de6dd240"
  def dlunwrap(addr) end

  # Returns a memory pointer of a function's hexadecimal address location +val+
  #
  # Example:
  #
  #   lib = Fiddle.dlopen('/lib64/libc-2.15.so')
  #   => #<Fiddle::Handle:0x00000001342460>
  #
  #   Fiddle.dlwrap(lib['strcpy'].to_s(16))
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

  # Cleared reference exception
  class ClearedReferenceError < Error
  end

  # == Description
  #
  # An FFI closure wrapper, for handling callbacks.
  #
  # == Example
  #
  #   closure = Class.new(Fiddle::Closure) {
  #     def call
  #       10
  #     end
  #   }.new(Fiddle::TYPE_INT, [])
  #      #=> #<#<Class:0x0000000150d308>:0x0000000150d240>
  #   func = Fiddle::Function.new(closure, [], Fiddle::TYPE_INT)
  #      #=> #<Fiddle::Function:0x00000001516e58>
  #   func.call
  #      #=> 10
  class Closure
    # Construct a new Closure object.
    #
    # * +ret+ is the C type to be returned
    # * +args+ is an Array of arguments, passed to the callback function
    # * +abi+ is the abi of the closure
    #
    # If there is an error in preparing the ffi_cif or ffi_prep_closure,
    # then a RuntimeError will be raised.
    def initialize(ret, args, abi = Fiddle::DEFAULT) end

    # Free this closure explicitly. You can't use this closure anymore.
    #
    # If this closure is already freed, this does nothing.
    def free; end

    # Whether this closure was freed explicitly.
    def freed?; end

    # Returns the memory address for this closure.
    def to_i; end
  end

  # standard dynamic load exception
  class DLError < Error
  end

  # Generic error class for Fiddle
  class Error < StandardError
  end

  # == Description
  #
  # A representation of a C function
  #
  # == Examples
  #
  # === 'strcpy'
  #
  #   @libc = Fiddle.dlopen "/lib/libc.so.6"
  #      #=> #<Fiddle::Handle:0x00000001d7a8d8>
  #   f = Fiddle::Function.new(
  #     @libc['strcpy'],
  #     [Fiddle::TYPE_VOIDP, Fiddle::TYPE_VOIDP],
  #     Fiddle::TYPE_VOIDP)
  #      #=> #<Fiddle::Function:0x00000001d8ee00>
  #   buff = "000"
  #      #=> "000"
  #   str = f.call(buff, "123")
  #      #=> #<Fiddle::Pointer:0x00000001d0c380 ptr=0x000000018a21b8 size=0 free=0x00000000000000>
  #   str.to_s
  #   => "123"
  #
  # === ABI check
  #
  #   @libc = Fiddle.dlopen "/lib/libc.so.6"
  #      #=> #<Fiddle::Handle:0x00000001d7a8d8>
  #   f = Fiddle::Function.new(@libc['strcpy'], [TYPE_VOIDP, TYPE_VOIDP], TYPE_VOIDP)
  #      #=> #<Fiddle::Function:0x00000001d8ee00>
  #   f.abi == Fiddle::Function::DEFAULT
  #      #=> true
  class Function
    # DEFAULT
    #
    # Default ABI
    DEFAULT = _
    # STDCALL
    #
    # FFI implementation of WIN32 stdcall convention
    STDCALL = _

    # Constructs a Function object.
    # * +ptr+ is a referenced function, of a Fiddle::Handle
    # * +args+ is an Array of arguments, passed to the +ptr+ function
    # * +ret_type+ is the return type of the function
    # * +abi+ is the ABI of the function
    # * +name+ is the name of the function
    # * +need_gvl+ is whether GVL is needed to call the function
    def initialize(p1, p2, p3, p4 = v4, p5 = {}) end

    # Calls the constructed Function, with +args+.
    # Caller must ensure the underlying function is called in a
    # thread-safe manner if running in a multi-threaded process.
    #
    # Note that it is not thread-safe to use this method to
    # directly or indirectly call many Ruby C-extension APIs unless
    # you don't pass +need_gvl: true+ to Fiddle::Function#new.
    #
    # For an example see Fiddle::Function
    def call(*args) end
  end

  # The Fiddle::Handle is the manner to access the dynamic library
  #
  # == Example
  #
  # === Setup
  #
  #   libc_so = "/lib64/libc.so.6"
  #   => "/lib64/libc.so.6"
  #   @handle = Fiddle::Handle.new(libc_so)
  #   => #<Fiddle::Handle:0x00000000d69ef8>
  #
  # === Setup, with flags
  #
  #   libc_so = "/lib64/libc.so.6"
  #   => "/lib64/libc.so.6"
  #   @handle = Fiddle::Handle.new(libc_so, Fiddle::RTLD_LAZY | Fiddle::RTLD_GLOBAL)
  #   => #<Fiddle::Handle:0x00000000d69ef8>
  #
  # See RTLD_LAZY and RTLD_GLOBAL
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
    # RTLD_GLOBAL
    #
    # rtld Fiddle::Handle flag.
    #
    # The symbols defined by this library will be made available for symbol
    # resolution of subsequently loaded libraries.
    RTLD_GLOBAL = _
    # RTLD_LAZY
    #
    # rtld Fiddle::Handle flag.
    #
    # Perform lazy binding.  Only resolve symbols as the code that references
    # them is executed.  If the  symbol is never referenced, then it is never
    # resolved.  (Lazy binding is only performed for function references;
    # references to variables are always immediately bound when the library
    # is loaded.)
    RTLD_LAZY = _
    # RTLD_NOW
    #
    # rtld Fiddle::Handle flag.
    #
    # If this value is specified or the environment variable LD_BIND_NOW is
    # set to a nonempty string, all undefined symbols in the library are
    # resolved before Fiddle.dlopen returns.  If this cannot be done an error
    # is returned.
    RTLD_NOW = _

    # Get the address as an Integer for the function named +name+.  The function
    # is searched via dlsym on RTLD_NEXT.
    #
    # See man(3) dlsym() for more info.
    def self.[](p1) end

    # Get the address as an Integer for the function named +name+.
    def self.sym(name) end

    def self.sym_defined?(p1) end

    # Create a new handler that opens +library+ with +flags+.
    #
    # If no +library+ is specified or +nil+ is given, DEFAULT is used, which is
    # the equivalent to RTLD_DEFAULT. See <code>man 3 dlopen</code> for more.
    #
    #      lib = Fiddle::Handle.new
    #
    # The default is dependent on OS, and provide a handle for all libraries
    # already loaded. For example, in most cases you can use this to access +libc+
    # functions, or ruby functions like +rb_str_new+.
    def initialize(library = nil, flags = Fiddle::RTLD_LAZY | Fiddle::RTLD_GLOBAL) end

    # Close this handle.
    #
    # Calling close more than once will raise a Fiddle::DLError exception.
    def close; end

    # Returns +true+ if dlclose() will be called when this handle is garbage collected.
    #
    # See man(3) dlclose() for more info.
    def close_enabled?; end

    # Disable a call to dlclose() when this handle is garbage collected.
    def disable_close; end

    # Enable a call to dlclose() when this handle is garbage collected.
    def enable_close; end

    # Returns the file name of this handle.
    def file_name; end

    # Get the address as an Integer for the function named +name+.
    def sym(name) end
    alias [] sym

    def sym_defined?(p1) end

    # Returns the memory address for this handle.
    def to_i; end

    # Returns the Fiddle::Pointer of this handle.
    def to_ptr; end
  end

  class MemoryView
    def self.export(p1) end

    def initialize(p1) end

    def [](*args) end

    def byte_size; end

    def format; end

    def item_size; end

    def ndim; end

    def obj; end

    def readonly?; end

    def release; end

    def shape; end

    def strides; end

    def sub_offsets; end

    def to_s; end
  end

  class Pinned
    # Create a new pinned object reference.  The Fiddle::Pinned instance will
    # prevent the GC from moving +object+.
    def initialize(object) end

    # Clear the reference to the object this is pinning.
    def clear; end

    # Returns true if the reference has been cleared, otherwise returns false.
    def cleared?; end

    # Return the object that this pinned instance references.
    def ref; end
  end

  # Fiddle::Pointer is a class to handle C pointers
  class Pointer
    # Get the underlying pointer for ruby object +val+ and return it as a
    # Fiddle::Pointer object.
    def self.[](val) end

    # == Examples
    #
    #    # Automatically freeing the pointer when the block is exited - recommended
    #    Fiddle::Pointer.malloc(size, Fiddle::RUBY_FREE) do |pointer|
    #      ...
    #    end
    #
    #    # Manually freeing but relying on the garbage collector otherwise
    #    pointer = Fiddle::Pointer.malloc(size, Fiddle::RUBY_FREE)
    #    ...
    #    pointer.call_free
    #
    #    # Relying on the garbage collector - may lead to unlimited memory allocated before freeing any, but safe
    #    pointer = Fiddle::Pointer.malloc(size, Fiddle::RUBY_FREE)
    #    ...
    #
    #    # Only manually freeing
    #    pointer = Fiddle::Pointer.malloc(size)
    #    begin
    #      ...
    #    ensure
    #      Fiddle.free pointer
    #    end
    #
    #    # No free function and no call to free - the native memory will leak if the pointer is garbage collected
    #    pointer = Fiddle::Pointer.malloc(size)
    #    ...
    #
    # Allocate +size+ bytes of memory and associate it with an optional
    # +freefunc+.
    #
    # If a block is supplied, the pointer will be yielded to the block instead of
    # being returned, and the return value of the block will be returned. A
    # +freefunc+ must be supplied if a block is.
    #
    # If a +freefunc+ is supplied it will be called once, when the pointer is
    # garbage collected or when the block is left if a block is supplied or
    # when the user calls +call_free+, whichever happens first. +freefunc+ must be
    # an address pointing to a function or an instance of +Fiddle::Function+.
    def self.malloc(...) end

    # Get the underlying pointer for ruby object +val+ and return it as a
    # Fiddle::Pointer object.
    def self.to_ptr(val) end

    # Create a new pointer to +address+ with an optional +size+ and +freefunc+.
    #
    # +freefunc+ will be called when the instance is garbage collected.
    def initialize(...) end

    # Returns a new pointer instance that has been advanced +n+ bytes.
    def +(other) end

    # Returns a new pointer instance that has been moved back +n+ bytes.
    def -(other) end

    # Returns -1 if less than, 0 if equal to, 1 if greater than +other+.
    #
    # Returns nil if +ptr+ cannot be compared to +other+.
    def <=>(other) end

    # Returns true if +other+ wraps the same pointer, otherwise returns
    # false.
    def ==(other) end
    alias eql? ==

    # Returns integer stored at _index_.
    #
    # If _start_ and _length_ are given, a string containing the bytes from
    # _start_ of _length_ will be returned.
    def [](...) end

    # Set the value at +index+ to +int+.
    #
    # Or, set the memory at +start+ until +length+ with the contents of +string+,
    # the memory from +dl_cptr+, or the memory pointed at by the memory address
    # +addr+.
    def []=(index, int) end

    # Call the free function for this pointer. Calling more than once will do
    # nothing. Does nothing if there is no free function attached.
    def call_free; end

    # Get the free function for this pointer.
    #
    # Returns a new instance of Fiddle::Function.
    #
    # See Fiddle::Function.new
    def free; end

    # Set the free function for this pointer to +function+ in the given
    # Fiddle::Function.
    def free=(function) end

    # Returns if the free function for this pointer has been called.
    def freed?; end

    # Returns a string formatted with an easily readable representation of the
    # internal state of the pointer.
    def inspect; end

    # Returns +true+ if this is a null pointer.
    def null?; end

    # Returns a new Fiddle::Pointer instance that is a dereferenced pointer for
    # this pointer.
    #
    # Analogous to the star operator in C.
    def ptr; end
    alias +@ ptr

    # Returns a new Fiddle::Pointer instance that is a reference pointer for this
    # pointer.
    #
    # Analogous to the ampersand operator in C.
    def ref; end
    alias -@ ref

    # Get the size of this pointer.
    def size; end

    # Set the size of this pointer to +size+
    def size=(size) end

    # Returns the integer memory location of this pointer.
    def to_i; end
    alias to_int to_i

    #    ptr.to_s        => string
    #    ptr.to_s(len)   => string
    #
    # Returns the pointer contents as a string.
    #
    # When called with no arguments, this method will return the contents until
    # the first NULL byte.
    #
    # When called with +len+, a string of +len+ bytes will be returned.
    #
    # See to_str
    def to_s(p1 = v1) end

    #    ptr.to_str        => string
    #    ptr.to_str(len)   => string
    #
    # Returns the pointer contents as a string.
    #
    # When called with no arguments, this method will return the contents with the
    # length of this pointer's +size+.
    #
    # When called with +len+, a string of +len+ bytes will be returned.
    #
    # See to_s
    def to_str(p1 = v1) end

    # Cast this pointer to a ruby object.
    def to_value; end
  end

  module Types
    # Fiddle::Types::CHAR
    #
    # C type - char
    CHAR = _
    # Fiddle::Types::CONST_STRING
    #
    # C type - const char* ('\0' terminated const char*)
    CONST_STRING = _
    # Fiddle::Types::DOUBLE
    #
    # C type - double
    DOUBLE = _
    # Fiddle::Types::FLOAT
    #
    # C type - float
    FLOAT = _
    # Fiddle::Types::INT
    #
    # C type - int
    INT = _
    # Fiddle::Types::INT16_T
    #
    # C type - int16_t
    INT16_T = _
    # Fiddle::Types::INT32_T
    #
    # C type - int32_t
    INT32_T = _
    # Fiddle::Types::INT64_T
    #
    # C type - int64_t
    INT64_T = _
    # Fiddle::Types::INT8_T
    #
    # C type - int8_t
    INT8_T = _
    # Fiddle::Types::INTPTR_T
    #
    # C type - intptr_t
    INTPTR_T = _
    # Fiddle::Types::LONG
    #
    # C type - long
    LONG = _
    # Fiddle::Types::LONG_LONG
    #
    # C type - long long
    LONG_LONG = _
    # Fiddle::Types::PTRDIFF_T
    #
    # C type - ptrdiff_t
    PTRDIFF_T = _
    # Fiddle::Types::SHORT
    #
    # C type - short
    SHORT = _
    # Fiddle::Types::SIZE_T
    #
    # C type - size_t
    SIZE_T = _
    # Fiddle::Types::SSIZE_T
    #
    # C type - ssize_t
    SSIZE_T = _
    # Fiddle::Types::UCHAR
    #
    # C type - unsigned char
    UCHAR = _
    # Fiddle::Types::UINT
    #
    # C type - unsigned int
    UINT = _
    # Fiddle::Types::UINT16_T
    #
    # C type - uint16_t
    UINT16_T = _
    # Fiddle::Types::UINT32_T
    #
    # C type - uint32_t
    UINT32_T = _
    # Fiddle::Types::UINT64_T
    #
    # C type - uint64_t
    UINT64_T = _
    # Fiddle::Types::UINT8_T
    #
    # C type - uint8_t
    UINT8_T = _
    # Fiddle::Types::UINTPTR_T
    #
    # C type - uintptr_t
    UINTPTR_T = _
    # Fiddle::Types::ULONG
    #
    # C type - long
    ULONG = _
    # Fiddle::Types::ULONG_LONG
    #
    # C type - long long
    ULONG_LONG = _
    # Fiddle::Types::USHORT
    #
    # C type - unsigned short
    USHORT = _
    # Fiddle::Types::VARIADIC
    #
    # C type - ...
    VARIADIC = _
    # Fiddle::Types::VOID
    #
    # C type - void
    VOID = _
    # Fiddle::Types::VOIDP
    #
    # C type - void*
    VOIDP = _
  end
end
