# frozen_string_literal: true

# == Description
#
# A libffi wrapper.
module Fiddle
  # TYPE_CHAR
  #
  # C type - char
  TYPE_CHAR = _
  # TYPE_DOUBLE
  #
  # C type - double
  TYPE_DOUBLE = _
  # TYPE_FLOAT
  #
  # C type - float
  TYPE_FLOAT = _
  # TYPE_INT
  #
  # C type - int
  TYPE_INT = _
  # TYPE_LONG
  #
  # C type - long
  TYPE_LONG = _
  # TYPE_LONG_LONG
  #
  # C type - long long
  TYPE_LONG_LONG = _
  # TYPE_SHORT
  #
  # C type - short
  TYPE_SHORT = _
  # TYPE_VOID
  #
  # C type - void
  TYPE_VOID = _
  # TYPE_VOIDP
  #
  # C type - void*
  TYPE_VOIDP = _
  # Returns a boolean regarding whether the host is WIN32
  WINDOWS = _

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
  #   => #<#<Class:0x0000000150d308>:0x0000000150d240>
  #   func = Fiddle::Function.new(closure, [], Fiddle::TYPE_INT)
  #   => #<Fiddle::Function:0x00000001516e58>
  #   func.call
  #   => 10
  class Closure
    # Construct a new Closure object.
    #
    # * +ret+ is the C type to be returned
    # * +args+ are passed the callback
    # * +abi+ is the abi of the closure
    #
    # If there is an error in preparing the ffi_cif or ffi_prep_closure,
    # then a RuntimeError will be raised.
    def initialize(ret, args, abi = Fiddle::DEFAULT) end

    # Returns the memory address for this closure
    def to_i; end
  end

  # == Description
  #
  # A representation of a C function
  #
  # == Examples
  #
  # === 'strcpy'
  #
  #   @libc = DL.dlopen "/lib/libc.so.6"
  #   => #<DL::Handle:0x00000001d7a8d8>
  #   f = Fiddle::Function.new(@libc['strcpy'], [TYPE_VOIDP, TYPE_VOIDP], TYPE_VOIDP)
  #   => #<Fiddle::Function:0x00000001d8ee00>
  #   buff = "000"
  #   => "000"
  #   str = f.call(buff, "123")
  #   => #<DL::CPtr:0x00000001d0c380 ptr=0x000000018a21b8 size=0 free=0x00000000000000>
  #   str.to_s
  #   => "123"
  #
  # === ABI check
  #
  #   @libc = DL.dlopen "/lib/libc.so.6"
  #   => #<DL::Handle:0x00000001d7a8d8>
  #   f = Fiddle::Function.new(@libc['strcpy'], [TYPE_VOIDP, TYPE_VOIDP], TYPE_VOIDP)
  #   => #<Fiddle::Function:0x00000001d8ee00>
  #   f.abi == Fiddle::Function::DEFAULT
  #   => true
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
    # * +ptr+ is a referenced function, of a DL::Handle
    # * +args+ is an Array of arguments, passed to the +ptr+ function
    # * +ret_type+ is the return type of the function
    # * +abi+ is the ABI of the function
    def initialize(ptr, args, ret_type, abi = DEFAULT) end

    # Calls the constructed Function, with +args+
    #
    # For an example see Fiddle::Function
    def call(*args) end
  end
end
