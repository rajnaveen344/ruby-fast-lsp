# frozen_string_literal: true

module DL
  ALIGN_DOUBLE = _
  ALIGN_FLOAT = _
  ALIGN_INT = _
  ALIGN_LONG = _
  ALIGN_SHORT = _
  ALIGN_VOIDP = _
  DLSTACK = _
  FREE = _
  FuncTable = _
  MAX_ARG = _
  RTLD_GLOBAL = _
  RTLD_LAZY = _
  RTLD_NOW = _

  def self.callback(p1, p2 = v2) end

  def self.define_callback(p1, p2 = v2) end

  def self.dlopen(*args) end

  def self.malloc(p1) end

  def self.remove_callback(p1) end

  def self.sizeof(p1) end

  def self.strdup(p1) end

  private

  def callback(p1, p2 = v2) end
  alias define_callback callback

  def dlopen(*args) end

  def malloc(p1) end

  def remove_callback(p1) end

  def sizeof(p1) end

  def strdup(p1) end

  class DLError < StandardError
  end

  class DLTypeError < DLError
  end

  class Handle
    def initialize(p1, p2 = v2) end

    def close; end

    def disable_close; end

    def enable_close; end

    def sym(p1, p2 = v2) end
    alias [] sym

    def to_i; end

    def to_ptr; end
  end

  module MemorySpace
    MemoryTable = _

    def self.each; end

    private

    def each; end
  end

  class PtrData
    def self.malloc(p1, p2 = v2) end

    def initialize(p1, p2 = v2, p3 = v3) end

    def +(other) end

    def -(other) end

    def <=>(other) end

    def ==(other) end
    alias eql? ==

    def [](p1, p2 = v2) end

    def []=(p1, p2, p3 = v3) end

    def data_type; end

    def define_data_type(p1, p2 = v2, *args) end

    def free; end

    def free=(p1) end

    def inspect; end

    def null?; end

    def ptr; end
    alias +@ ptr

    def ref; end
    alias -@ ref

    def size(p1 = v1) end
    alias size= size

    def struct!(*args) end

    def to_a(p1, p2 = v2) end

    def to_i; end

    def to_s(p1 = v1) end

    def to_str(p1 = v1) end

    def union!(*args) end
  end

  class Symbol
    def self.char2type(p1) end

    def initialize(p1, p2 = v2, p3 = v3) end

    # defined(DLSTACK_GUARD)
    def call(*args) end
    alias [] call

    def cproto; end
    alias to_s cproto

    def inspect; end

    def name; end

    def proto; end

    def to_i; end

    def to_ptr; end
  end
end
